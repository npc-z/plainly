//! The retry policy: what Plainly does after an attempt fails.
//!
//! The four documented classes each go their own way (spec §7):
//!
//! - an unusable **answer** — malformed JSON, a schema violation, nothing usable
//!   at all — is asked for again, up to twice more, because a model at
//!   temperature 0 can still miss once;
//! - **the same answer twice** ends it there: a contract violation that
//!   reproduces is systematic drift, not sampling noise, and the measured
//!   example (a renamed field, 3 times out of 3) says the third call is wasted;
//! - a **4xx that rejects the request's shape** is not retried at all: the
//!   capability we assumed for that endpoint is wrong, and tickets/05 downgrades
//!   it and probes again;
//! - **429, 5xx, timeouts and dropped connections** are transient, so they back
//!   off — about a second, then about four — and that schedule runs even when
//!   the error repeats, because a rate limit that repeats is exactly what
//!   backing off is for. Repeat detection belongs to the answer's shape, not to
//!   the network.
//!
//! Everything the policy decides is injected: the waiting is a closure, so a test
//! can read the schedule instead of sleeping through it, and the clock is an
//! instant the caller chose (tickets/02).

use std::time::Duration;

use crate::artifact::{Artifact, Timestamp};
use crate::explain::{ExplainError, explain as attempt};
use crate::explanation::ContractError;
use crate::provider::{ExplainRequest, Provider, ProviderErrorKind};

/// How many extra attempts any class is allowed. The policy's business, not the
/// callers': what a surface needs to know is on [`Failure`].
const MAX_EXTRA_ATTEMPTS: u32 = 2;

/// How long to wait before each backoff retry: about a second, then about four.
const BACKOFF: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(4)];

/// The schedule is exactly as long as the attempt budget: every extra attempt a
/// class is allowed gets a wait, so an index inside the budget is in range and
/// the loop below never has to guess what to do past the end of it.
const _: () = assert!(BACKOFF.len() == MAX_EXTRA_ATTEMPTS as usize);

/// Produce the Artifact for one Passage, retrying as the policy allows.
///
/// `pause` is called before every retry with the wait the policy chose — zero
/// for a class that retries at once. Whether that wait is slept through is the
/// caller's move, and the CLI reports progress there too.
///
/// The instant belongs to the caller and stands for the whole run: a retry that
/// spans the backoff does not move it, because an Explanation is stamped with
/// when its query was made, not with how long the network took.
pub fn explain(
    provider: &dyn Provider,
    request: &ExplainRequest,
    now: Timestamp,
    pause: &dyn Fn(Duration),
) -> Result<Artifact, Failure> {
    let mut attempts = 0;
    let mut backoffs = 0;
    let mut previous: Option<Signature> = None;

    loop {
        attempts += 1;

        let error = match attempt(provider, request, now) {
            Ok(artifact) => return Ok(artifact),
            Err(error) => error,
        };
        let kind = FailureKind::classify(&error);
        let reason = error.to_string();
        let signature = Signature::of(&error);

        // The same unusable answer twice: the model is not wavering, it is
        // answering something else. A third call would buy the same answer.
        if kind.repeats_mean_drift() && previous.as_ref() == Some(&signature) {
            return Err(Failure {
                kind,
                reason,
                attempts,
                stopped: Stopped::Repeated,
            });
        }
        previous = Some(signature);

        let wait = match kind.verdict() {
            Verdict::Never => {
                return Err(Failure {
                    kind,
                    reason,
                    attempts,
                    stopped: Stopped::NotRetryable,
                });
            }
            _ if attempts > MAX_EXTRA_ATTEMPTS => {
                return Err(Failure {
                    kind,
                    reason,
                    attempts,
                    stopped: Stopped::Exhausted,
                });
            }
            Verdict::AtOnce => Duration::ZERO,
            Verdict::Backoff => {
                let wait = BACKOFF[backoffs];
                backoffs += 1;
                wait
            }
        };

        pause(wait);
    }
}

/// Why an Explanation was not produced, after the policy gave up.
///
/// The surface has everything it needs to say what happened: the class, the
/// reason to show, and how many requests were made. A `Failure` is the only
/// outcome of a failed run — there is no Artifact — and an Artifact is the only
/// thing a Record can be written from (tickets/08), so nothing was stored, and
/// neither the Passage nor anything it came from was touched. The sentence the
/// panel puts around that (spec §12) is the panel's copy (tickets/15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// The class, for a caller that branches rather than prints.
    pub kind: FailureKind,
    /// The reason to show a person, from the last attempt.
    pub reason: String,
    /// How many requests were made. One means nothing was retried.
    pub attempts: u32,
    /// Why the last attempt was not followed by another.
    pub stopped: Stopped,
}

impl Failure {
    /// Whether anything was asked twice.
    pub fn retried(&self) -> bool {
        self.attempts > 1
    }
}

/// The class of a failure, which is what the policy branches on.
///
/// The transport's own view of a provider failure is [`ProviderErrorKind`]; this
/// adds the contract failure and names each class by what it means *for the
/// policy*, which is the sentence each variant carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// The endpoint rejected the request's shape; tickets/05 downgrades the
    /// capability and probes again.
    UnsupportedParameter,
    /// Transient trouble — 429, 5xx, a timeout, a dropped connection — so the
    /// same request is worth making again later.
    Unavailable,
    /// Asking again cannot help until the configuration changes.
    Misconfigured,
    /// The model declined; asking again is not a fix.
    Refused,
    /// The answer was not an Explanation: malformed JSON or a schema violation
    /// on this side, or a response that is not a chat completion at all.
    Malformed,
    /// The model produced nothing usable: empty content, a chain of thought
    /// where the answer should have been, a response with no choices.
    Empty,
    /// Cut off by the token budget; the same request with the same budget is
    /// cut off again.
    Truncated,
}

/// What makes two failures "the same one" for the drift rule.
///
/// The class, plus the contract's own structure where it has any. A provider's
/// message is deliberately not part of it: messages carry response bodies,
/// request ids and excerpts, so the same trouble can be worded differently on
/// every attempt, and comparing the text would let a systematic failure be
/// retried to the end of the budget.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Signature {
    /// JSON that is not JSON, identified by where the parser stopped.
    MalformedJson(String),
    /// A schema violation, identified by the problems it reported.
    Schema(Vec<String>),
    /// A provider failure, identified by its class alone.
    Provider(ProviderErrorKind),
}

impl Signature {
    fn of(error: &ExplainError) -> Self {
        match error {
            ExplainError::Contract(ContractError::MalformedJson { message }) => {
                Signature::MalformedJson(message.clone())
            }
            ExplainError::Contract(ContractError::Schema { problems }) => {
                Signature::Schema(problems.clone())
            }
            ExplainError::Provider(error) => Signature::Provider(error.kind()),
        }
    }
}

/// What the policy does after a failure of a given class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Ask again at once: an answer's shape can waver without the network
    /// having anything to do with it.
    AtOnce,
    /// Ask again after a backoff: the endpoint's trouble is transient.
    Backoff,
    /// Do not ask again.
    Never,
}

impl FailureKind {
    fn classify(error: &ExplainError) -> Self {
        match error {
            ExplainError::Contract(_) => FailureKind::Malformed,
            ExplainError::Provider(error) => match error.kind() {
                ProviderErrorKind::UnsupportedParameter => FailureKind::UnsupportedParameter,
                ProviderErrorKind::Unavailable => FailureKind::Unavailable,
                ProviderErrorKind::Misconfigured => FailureKind::Misconfigured,
                ProviderErrorKind::Empty => FailureKind::Empty,
                ProviderErrorKind::Refused => FailureKind::Refused,
                ProviderErrorKind::Truncated => FailureKind::Truncated,
            },
        }
    }

    fn verdict(self) -> Verdict {
        match self {
            FailureKind::Malformed | FailureKind::Empty => Verdict::AtOnce,
            FailureKind::Unavailable => Verdict::Backoff,
            FailureKind::UnsupportedParameter
            | FailureKind::Misconfigured
            | FailureKind::Refused
            | FailureKind::Truncated => Verdict::Never,
        }
    }

    /// Whether the model's *answer* failed the contract: JSON that did not
    /// parse, or a document that did not hold the schema.
    ///
    /// [`FailureKind::Empty`] is deliberately not included, even though it
    /// repeats as drift alongside this one. An empty answer also covers an
    /// endpoint that did not answer with a chat completion at all — no choices,
    /// no message, a 200 whose body is not JSON — and blaming the user's prompt
    /// for that (tickets/07 answers a contract failure with "your prompt did not
    /// pass the contract" and a retry on the factory prompt) would be wrong, on
    /// a retry that could not differ.
    pub fn is_contract(self) -> bool {
        matches!(self, FailureKind::Malformed)
    }

    /// Whether two of these in a row mean drift rather than noise.
    ///
    /// The answer's shape, malformed or empty: a rate limit or a 5xx that
    /// repeats is the case backing off exists for, and cutting that short would
    /// make the documented schedule dead code.
    fn repeats_mean_drift(self) -> bool {
        matches!(self, FailureKind::Malformed | FailureKind::Empty)
    }
}

/// Why the policy stopped asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// The same unusable answer came back twice: systematic, not noise.
    Repeated,
    /// This class is not worth retrying at all.
    NotRetryable,
    /// The extra attempts this class was allowed are used up.
    Exhausted,
}
