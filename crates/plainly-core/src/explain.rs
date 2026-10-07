//! The explain path: ask a provider, hold the answer to the contract, attach
//! everything the model is not trusted with.

use crate::artifact::{ARTIFACT_VERSION, Artifact, Timestamp};
use crate::explanation::{ContractError, Explanation};
use crate::provider::{ExplainRequest, Provider, ProviderError};

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

/// Produce one Explanation for one Passage.
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
