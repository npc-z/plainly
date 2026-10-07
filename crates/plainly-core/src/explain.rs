//! The explain path: one attempt, and the whole run around it.
//!
//! [`explain`] is a single attempt — ask, hold the answer to the contract, attach
//! everything the model is not trusted with — and [`run`] is what a surface
//! actually calls: probe the endpoint's capability ([`crate::probe`]), drive the
//! retry policy ([`crate::retry`]), and, when the endpoint rejects the request's
//! shape, correct the capability and ask once more. Both live here because both
//! answer the same question — what happened to this one Passage — and neither
//! belongs in the modules that decide what an endpoint is or how long to wait.

use std::time::Duration;

use crate::Thinking;
use crate::artifact::{ARTIFACT_VERSION, Artifact, Timestamp};
use crate::explanation::{ContractError, Explanation};
use crate::presets::{Surface, ThinkingSwitch};
use crate::probe::{Cache, Capability, Endpoint, Origin, Resolution, capability_for, reprobe};
use crate::provider::{ExplainRequest, Provider, ProviderError};
use crate::retry::{self, Failure, FailureKind};
use crate::setup::ProviderSetup;

/// The Passage's language. v0 explains English and never detects it: a detector
/// would either cost a model call or fork the cache on a guess, so the field is
/// a constant that is still stored, for provenance and for a later v1 (spec §9).
pub const SOURCE_LANGUAGE: &str = "en";

/// Why an Explanation was not produced.
///
/// This distinction is the one the whole error policy rests on: the provider
/// failed, or it answered and the answer did not hold the contract. They read
/// differently to the learner and they are handled differently — one is retried
/// or rerouted, the other questions the prompt (tickets/04, tickets/07).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExplainError {
    /// The provider answered, but not with an Explanation.
    #[error(transparent)]
    Contract(#[from] ContractError),
    /// The provider could not answer at all.
    #[error(transparent)]
    Provider(#[from] ProviderError),
}

/// Produce the Artifact for one Passage, in one attempt.
///
/// A surface does not call this directly: [`crate::retry::explain`] drives it,
/// decides whether a failure is worth asking about again, and reports what
/// happened. This is the attempt itself — ask, hold the answer to the contract,
/// attach everything the model is not trusted with.
///
/// `now` is a value rather than a call to the system clock: the only use this
/// path has for the time is stamping the Artifact, so a caller — and a test —
/// can hand over a fixed instant instead of a clock seam being invented for it.
pub fn explain(
    provider: &dyn Provider,
    request: &ExplainRequest,
    now: Timestamp,
) -> Result<Artifact, ExplainError> {
    let answer = provider.generate(request)?;
    let explanation = Explanation::parse(&answer)?;

    Ok(Artifact {
        passage: request.passage.clone(),
        level: request.level,
        source_language: request.source_language.clone(),
        native_language: request.native_language.clone(),
        provider: request.provider.clone(),
        model: request.model.clone(),
        thinking: request.thinking,
        artifact_version: ARTIFACT_VERSION,
        prompt_version: request.prompt_version.clone(),
        prompt_label: request.prompt_label.clone(),
        // Nothing has been stored or reused yet, so the two timestamps agree.
        // They part ways once a Record is looked up a second time (tickets/08).
        created_at: now,
        generated_at: now,
        explanation,
    })
}

/// A capability corrected because the endpoint rejected the shape in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downgrade {
    /// The capability the rejected attempt was made with.
    pub from: Capability,
    /// What the endpoint said when it rejected that shape.
    pub reason: String,
}

/// One Passage explained, with the capability that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub artifact: Artifact,
    /// The capability the successful attempt used.
    pub resolution: Resolution,
    /// `Some` when the endpoint rejected the shape first assumed.
    pub downgrade: Option<Downgrade>,
}

/// Why a run produced no Explanation, and what was learned on the way.
///
/// Boxed at the return site rather than trimmed to fit: it carries the whole
/// capability in effect, which is what a surface needs to say *which* shape was
/// refused, and a failed run is a once-per-request event rather than a value in
/// a hot loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFailure {
    /// The retry policy's own account of the last attempt.
    pub failure: Failure,
    /// The resolution behind the attempt that failed — the capability it was
    /// *sent* under, which is the one a surface has to report. A later probe's
    /// conclusion is not what this error is about.
    pub resolution: Resolution,
    /// A forced re-probe that ran and did not change what this run could send,
    /// when there was one. It is what a surface needs to explain why the Passage
    /// was not asked for a second time.
    pub reprobe: Option<Resolution>,
    /// The correction made before the last attempt, if one was made.
    pub downgrade: Option<Downgrade>,
}

/// Produce the Artifact for one Passage: probe, run, and downgrade once if the
/// endpoint rejects the shape.
///
/// `connect` builds a transport for a capability-resolved setup. It is a closure
/// because the API key belongs to the surface — the CLI holds it, and the panel
/// will hold its own — while everything this function decides is the same for
/// both. It is called at most four times: once to read the capability and once
/// for the attempt, then the same pair again after a rejected shape — and only
/// when the re-probe turns up something the next request can actually use.
///
/// A 4xx that rejects the request's shape is not retried as-is ([`crate::retry`]
/// stops there on purpose). It is instead the evidence that the capability in
/// effect is wrong: the endpoint is probed again, and only if that probe *answers*
/// and reaches a different conclusion is the Passage asked for a second time. An
/// endpoint that rejects a shape its own probe says it accepts is contradicting
/// itself, and asking it the same question again is exactly the wasted call the
/// policy refuses to make; an endpoint that will not answer the probe either has
/// told us nothing to ask with.
pub fn run(
    setup: &ProviderSetup,
    request: &ExplainRequest,
    connect: &dyn Fn(&ProviderSetup) -> Box<dyn Endpoint>,
    cache: &Cache,
    now: Timestamp,
    pause: &dyn Fn(Duration),
) -> Result<Run, Box<RunFailure>> {
    let first = capability_for(&*connect(setup), setup, cache, now);
    let active = setup.with_capability(first.capability.schema, first.capability.thinking);
    let failure = match retry::explain(&*connect(&active), request, now, pause) {
        Ok(artifact) => {
            return Ok(Run {
                artifact,
                resolution: first,
                downgrade: None,
            });
        }
        Err(failure) => failure,
    };

    if failure.kind != FailureKind::UnsupportedParameter {
        return Err(Box::new(RunFailure {
            failure,
            resolution: first,
            reprobe: None,
            downgrade: None,
        }));
    }

    // The conclusion in hand is what the endpoint just refused, so it is dropped
    // before the probe runs: a cache that answered wrongly once must not be
    // allowed to answer wrongly again.
    cache.forget(&setup.name);
    let second = reprobe(&*connect(setup), setup, cache, now);

    // Asking again takes a *conclusion*, not merely a different guess: a probe
    // that learned nothing (the endpoint did not answer it either) leaves the
    // cautious shape in hand, and reporting that as the outcome of a re-probe
    // would put words in the endpoint's mouth. The failure stands — reported
    // under the capability the request actually carried — with the re-probe
    // alongside it.
    let learned = second.origin == Origin::Probed
        && changes_the_request(setup, &first.capability, &second.capability);
    if !learned {
        return Err(Box::new(RunFailure {
            failure,
            resolution: first,
            reprobe: Some(second),
            downgrade: None,
        }));
    }

    let downgrade = Downgrade {
        from: first.capability,
        reason: failure.reason.clone(),
    };
    let active = setup.with_capability(second.capability.schema, second.capability.thinking);
    match retry::explain(&*connect(&active), request, now, pause) {
        Ok(artifact) => Ok(Run {
            artifact,
            resolution: second,
            downgrade: Some(downgrade),
        }),
        Err(failure) => Err(Box::new(RunFailure {
            failure,
            resolution: second,
            reprobe: None,
            downgrade: Some(downgrade),
        })),
    }
}

/// Whether `to` would make the next attempt a *different request* than `from`
/// did.
///
/// A capability the request cannot reflect is not a correction. The thinking
/// switch reaches the wire only when the user asked for thinking off, on the
/// route that carries the canonical spelling; a change to it under any other
/// circumstances would buy a byte-identical second ask, which is exactly the
/// wasted call [`crate::retry`] exists to refuse. The schema, by contrast, is
/// always on the wire in one of its two forms.
fn changes_the_request(setup: &ProviderSetup, from: &Capability, to: &Capability) -> bool {
    let sends_switch = |capability: &Capability| {
        setup.thinking == Thinking::Off
            && capability.thinking == ThinkingSwitch::Canonical
            && setup.surface == Surface::OpenAi
    };

    from.schema != to.schema || sends_switch(from) != sends_switch(to)
}
