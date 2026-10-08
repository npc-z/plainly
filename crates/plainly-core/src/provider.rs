//! The provider seam: asking one model for an Explanation.
//!
//! Everything provider-specific lives behind [`Provider`]: which request shape a
//! runtime wants, whether a schema can be enforced at all, what its empty answer
//! looks like, and — through [`ProviderError`] — which *class* of failure it hit,
//! because "the model answered with nothing usable" and "the endpoint is
//! rate-limiting us" call for different things from the retry policy
//! ([`crate::retry`], tickets/04).
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
    /// descriptors ([`crate::Prompt`]) — so a provider is handed the finished
    /// text rather than assembling one of its own.
    pub system_prompt: String,
    /// The SHA-256 of the prompt data in effect — the factory prompt plus the
    /// user's appendix and the level descriptors ([`crate::Prompt`]). It is what
    /// makes two runs comparable.
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

/// Why a provider could not produce content, as a class the policy can branch on.
///
/// A provider failing is still a different outcome from a provider answering
/// with something unusable (tickets/02): the first is this type, the second
/// arrives as a [`ContractError`](crate::ContractError) from the explain path.
/// The adapter does its half of the classifying on its own side of the seam —
/// the HTTP status, the transport error, the shape of what came back — and
/// [`crate::retry`] reads both halves, because the class is what decides whether
/// asking again is worth anything. Anything not listed here is a bug in the
/// adapter rather than a case to handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderErrorKind {
    /// The endpoint rejected the request's shape: HTTP 400 or 422 that names it.
    /// The assumed capability is wrong, so asking again is pointless and the
    /// capability is re-probed instead.
    UnsupportedParameter,
    /// 429, 5xx, a timeout, or a connection that dropped. Transient: the same
    /// request may work later.
    Unavailable,
    /// Asking again cannot help: a bad URL, an unknown host, a model the
    /// endpoint does not have, a credential it rejected.
    Misconfigured,
    /// The model answered with nothing usable.
    Empty,
    /// The model declined to answer.
    Refused,
    /// The answer was cut off by the token budget.
    Truncated,
}

/// Why a provider could not produce content.
///
/// The kind is the machine-readable part — the retry policy branches on it and
/// tickets/05 downgrades a capability on [`ProviderErrorKind::UnsupportedParameter`]
/// — while the message is the part a person reads.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ProviderError {
    kind: ProviderErrorKind,
    message: String,
    /// The HTTP status the endpoint answered with, when it answered at all.
    ///
    /// A class does not say whether anything was listening — 401 and 404 are both
    /// [`ProviderErrorKind::Misconfigured`] — and a surface that has to word the
    /// difference ("it refused us" against "nothing is there") needs the status
    /// rather than a search through the message (tickets/06).
    status: Option<u16>,
}

/// One constructor per class. What each class means is on
/// [`ProviderErrorKind`]; the message is what a person reads.
impl ProviderError {
    pub fn unsupported_parameter(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::UnsupportedParameter, message)
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Unavailable, message)
    }

    pub fn misconfigured(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Misconfigured, message)
    }

    pub fn empty(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Empty, message)
    }

    pub fn refused(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Refused, message)
    }

    pub fn truncated(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Truncated, message)
    }

    /// Which class of failure this is.
    pub fn kind(&self) -> ProviderErrorKind {
        self.kind
    }

    /// The HTTP status the endpoint answered with, or `None` when it never
    /// answered: a transport failure, a body that was not JSON, an empty answer.
    ///
    /// It is what tells a credential rejection (401, 403) from an endpoint that
    /// is not a model server (404), which the class alone does not: both are
    /// [`ProviderErrorKind::Misconfigured`], and only one of them is fixed by
    /// setting a key.
    pub fn status(&self) -> Option<u16> {
        self.status
    }

    /// One failure with the status that produced it. The HTTP adapter's own
    /// constructor: nothing else knows a status.
    pub(crate) fn answered(
        kind: ProviderErrorKind,
        message: impl Into<String>,
        status: u16,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            status: Some(status),
        }
    }

    fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            status: None,
        }
    }
}
