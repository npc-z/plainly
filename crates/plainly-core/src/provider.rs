//! The provider seam: asking one model for an Explanation.
//!
//! Everything provider-specific lives behind [`Provider`]: which request shape a
//! runtime wants, whether a schema can be enforced at all, what its empty answer
//! looks like. The explain path above it knows only that a provider either hands
//! back content or fails.
//!
//! Presets, capability probing and local discovery — tickets/03 through
//! tickets/06 — build on this seam rather than widening it: [`crate::presets`]
//! and [`crate::setup`] decide *what* a run talks to, and
//! [`crate::chat::ChatCompletions`] is the HTTP implementation. The one field
//! they added to the request is the prompt itself, because the app owns the
//! prompt and a provider should only have to send it.

use crate::{Level, Thinking};

/// Everything one Explanation needs from the caller, minus the model's answer.
///
/// `provider`, `model` and `thinking` are not instructions to the provider — an
/// implementation already knows what it is — they travel with the request so the
/// Artifact can be stamped with who answered it. Those same three values are what
/// the Lookup Key is made of (spec §9), which is why they are carried together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplainRequest {
    /// The Passage to make comprehensible, verbatim.
    pub passage: String,
    pub level: Level,
    pub source_language: String,
    pub native_language: String,
    pub provider: String,
    pub model: String,
    pub thinking: Thinking,
    /// The system prompt to send, already filled in for this request. The app
    /// owns the prompt — its factory text, the user's appendix, the level
    /// descriptors — so a provider is handed the finished text rather than
    /// assembling one of its own (tickets/07).
    pub system_prompt: String,
    /// The SHA-256 of the prompt data in effect — the factory prompt today,
    /// plus the user's appendix and the level descriptors once tickets/07 folds
    /// them in. It is what makes two runs comparable.
    pub prompt_version: String,
    /// That prompt's human-readable name.
    pub prompt_label: String,
}

/// Something that can turn an [`ExplainRequest`] into model output.
///
/// Implementations return the assistant's raw text rather than a parsed
/// [`Explanation`](crate::Explanation): parsing and validating are the app's job,
/// so a provider that cannot enforce a schema at all (DeepSeek) is not a
/// different kind of provider — only a different rate of contract failures.
pub trait Provider {
    /// Send one request and return the assistant's content.
    fn generate(&self, request: &ExplainRequest) -> Result<String, ProviderError>;
}

/// Why a provider could not produce content.
///
/// One classification, deliberately. The provider layer's own taxonomy —
/// unsupported parameters, rate limits, refusals, timeouts — is the retry
/// policy's business and arrives with it in tickets/04. What matters at this seam
/// is that a provider failing is a different outcome from a provider answering
/// with something unusable: the first is nobody's fault, the second is a model
/// that will be asked again.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ProviderError {
    message: String,
}

impl ProviderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
