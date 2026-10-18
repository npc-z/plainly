//! Running the Passages, off the main loop.
//!
//! One thread may touch GTK widgets, so the run happens on another. What is left
//! here is the thread, the channel, the events, and the mapping from a
//! [`Turn`](plainly_core::Turn) to a chunk: everything a run needs — the paths,
//! the configuration, the provider, the credential, the prompt, the history store
//! (which is also the cache), the capability cache and the instant — is opened by
//! [`plainly_core::Runner`] and owned by it (ADR 0003). The panel cannot drift
//! from the CLI's run again because it no longer assembles a run of its own.

use async_channel::Sender;
use plainly_core::{
    Artifact, Failure, LookupKey, PromptChoice, Runner, Timestamp, TurnFailure,
};

use crate::provenance::Provenance;
use crate::state::{first_occurrences, unreported};
use crate::strings;

/// What a trigger tells the run about the cache, and about which provider.
///
/// This is core's value rather than a second copy of it: the runner decides what
/// a `Reuse` or a `Bypass` means, and the panel only chooses which one a press
/// asks for (`prefs::regenerate_recall`). A key press reuses the cache and the
/// configured provider; ⟳ bypasses the cache — optionally on the cloud provider
/// the note under a local model promises (spec §12).
pub use plainly_core::Recall;

/// What came of one Passage.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// It has an Explanation, from the cache or from the provider, and the
    /// provenance of the answer that is actually in hand.
    Explained {
        artifact: Box<Artifact>,
        provenance: Provenance,
        /// Facts about this chunk that are not failures and do not cost the
        /// answer: today, that the Explanation was produced but the history
        /// could not keep it (ADR 0003). Worded here through [`strings`] so the
        /// UI carries no copy of its own.
        warnings: Vec<String>,
    },
    /// It has none, and this says why.
    Failed(String),
}

/// One thing that happened to one Passage, or to the run as a whole.
#[derive(Debug, Clone)]
pub enum Event {
    /// A Passage's turn came out.
    Chunk { index: usize, outcome: Outcome },
    /// A run-level fact to show **once**, in the status line. It is not a chunk
    /// warning: a credential store that cannot answer is a fact about this
    /// machine, and a sentence repeated under every Passage would read as part of
    /// each Explanation (the same reason the note under the content is one line).
    Notice(String),
    /// The run could not start at all: configuration, key or store.
    Failed(String),
}

/// Explain the Passages in turn, reporting each one as it lands.
///
/// The channel belongs to one trigger: the surface that started this run ignores
/// whatever arrives after a later trigger has taken over.
pub fn start(passages: Vec<String>, recall: Recall, sender: Sender<Event>) {
    std::thread::spawn(move || {
        let send = |event: Event| {
            let _ = sender.send_blocking(event);
        };

        if let Err(message) = run(&passages, recall, &send) {
            send(Event::Failed(message));
        }
    });
}

/// The whole run: everything that can fail before the first Passage is a failure
/// of the run rather than of a chunk.
fn run(passages: &[String], recall: Recall, send: &dyn Fn(Event)) -> Result<(), String> {
    // The clock is still the surface's: core takes the instant as a value, and
    // `Timestamp::now` is the crate's single conversion for it.
    let now = Timestamp::now().map_err(|error| error.to_string())?;
    // Everything a run needs is opened once, here, by the one module that owns it.
    // The credential wording is the panel's own (ADR 0003): core's message says
    // which provider and variable, not which surface is reading it.
    let mut runner = Runner::open(PromptChoice::Configured, now)
        .map_err(|error| strings::runner_error(&error))?;

    // A run-level notice goes out once, before the turns. `shown` is how the
    // panel hears about a notice a later turn adds — a `Bypass` provider whose
    // credential store cannot answer — without hearing the earlier ones again.
    let mut shown = 0;
    report_new_notices(&runner, &mut shown, send);

    // A Passage that repeats in one trigger is the same question — the Lookup
    // Key is over the Passage and this run's own settings — so it is asked once,
    // and every later occurrence shows the answer the first one got. That is the
    // surface's dedup, by the same key rule the CLI plans a batch with (spec
    // §3); the runner still owns exactly one Passage per call.
    let keys: Vec<LookupKey> = passages
        .iter()
        .map(|passage| runner.lookup(passage))
        .collect();
    let repeats = first_occurrences(&keys);
    let mut outcomes: Vec<Option<Outcome>> = vec![None; passages.len()];

    for (index, passage) in passages.iter().enumerate() {
        // A repeat is planned after the turn it repeats, so that turn has
        // already reported and its Outcome is in hand.
        let (outcome, asked) = match repeats[index] {
            Some(earlier) => (
                outcomes[earlier].clone().ok_or_else(|| {
                    "a repeat was planned after the turn it repeats".to_string()
                })?,
                false,
            ),
            None => {
                let outcome = match runner.explain(passage, recall.clone()) {
                    Ok(turn) => Outcome::Explained {
                        // Whether the endpoint that answered is on this machine
                        // is the turn's own fact: a `Bypass` provider can point
                        // somewhere else than the configured one, and only the
                        // runner knows which setup was resolved.
                        provenance: Provenance::of(&turn.artifact, turn.local),
                        artifact: Box::new(turn.artifact),
                        // Only turn notices reach here. They are warnings, not
                        // failures: `NotStored` keeps an answer the person
                        // already paid for.
                        warnings: turn.notices.iter().map(strings::turn_notice).collect(),
                    },
                    // The probe / retry / downgrade path gave up: the same
                    // sentence the panel has always shown, plus the "nothing was
                    // stored" line the rendering adds.
                    Err(TurnFailure::Run(failure)) => Outcome::Failed(describe(&failure.failure)),
                    // A `Bypass` provider the panel named could not be used. The
                    // turn is the failure, not the click, so the chunk says which
                    // provider and why while the rest of the stack carries on.
                    Err(TurnFailure::Unavailable { provider, error }) => {
                        Outcome::Failed(strings::provider_unavailable(&provider, &error))
                    }
                    // The history could not be read, so no stored answer could
                    // be found. The provider was never asked; the chunk fails
                    // with the store's own reason rather than paying for a
                    // generation that may already exist.
                    Err(TurnFailure::Store(error)) => {
                        Outcome::Failed(strings::history_unreadable(&error))
                    }
                };
                (outcome, true)
            }
        };

        outcomes[index] = Some(outcome.clone());
        send(Event::Chunk { index, outcome });
        // A fresh turn may add a run-level notice (a `Bypass` provider's
        // credential store could not answer); a repeat makes no request and so
        // adds none.
        if asked {
            report_new_notices(&runner, &mut shown, send);
        }
    }

    Ok(())
}

/// Send every run-level notice the panel has not been told about yet.
///
/// One at a time and never twice: the panel appends them to its status line, and
/// the fact is about the machine rather than about a Passage, so a repeat under
/// each chunk would be noise.
fn report_new_notices(runner: &Runner, shown: &mut usize, send: &dyn Fn(Event)) {
    for notice in unreported(runner.notices(), *shown) {
        send(Event::Notice(strings::run_notice(notice)));
    }
    *shown = runner.notices().len();
}

/// What a person is told when the policy gives up: the reason, and whether
/// anything was retried. The surface adds "nothing was stored" itself.
fn describe(failure: &Failure) -> String {
    if failure.retried() {
        format!("{}（重试了 {} 次）", failure.reason, failure.attempts)
    } else {
        failure.reason.clone()
    }
}
