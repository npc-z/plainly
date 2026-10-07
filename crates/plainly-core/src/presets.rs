//! The shipped provider presets: a known endpoint plus the capability shape we
//! start from for that endpoint.
//!
//! A preset is a starting point, not a capability table. The shape here is what
//! these endpoints are known to look like *today*; tickets/05 replaces it with
//! what a probe of the actual endpoint finds, because the same vendor changes
//! under you — DeepSeek answers `json_object` and rejects `json_schema`, and no
//! amount of knowing the name tells you that. The endpoint and the model can be
//! overridden in configuration; the capability shape belongs to the endpoint, so
//! [`crate::setup`] stops trusting it the moment the endpoint names another
//! service.
//!
//! Presets for the local runtimes deliberately name no model: llama.cpp router,
//! Ollama and LM Studio all serve whatever the user happens to have, and ticket
//! 06 discovers the list rather than this table inventing a name that would 404.

use serde::{Deserialize, Serialize};

/// Which call surface an endpoint speaks.
///
/// This is vendor knowledge — the same way an endpoint's default port is — and
/// not a capability: Ollama serves both surfaces, and the native one is the one
/// its `format` sits on. A preset that ships no surface is assumed to speak the
/// OpenAI-compatible route, because that is the only one Plainly can describe
/// for an endpoint nobody has heard of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    /// `POST {endpoint}/chat/completions`, the OpenAI shape.
    OpenAi,
    /// Ollama's own `POST /api/chat`, preferred over its `/v1` compatibility
    /// layer because `format` is where its structured output lives.
    Ollama,
}

/// Whether an endpoint can be held to the Explanation contract's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SchemaSupport {
    /// The endpoint takes a JSON Schema and constrains decoding to it, so the
    /// answer's shape is the endpoint's job and ours is a check.
    Enforced,
    /// The endpoint can only be asked for JSON. The answer is still held to the
    /// contract here, so a violation means asking again.
    BestEffort,
}

/// Whether an endpoint takes the canonical thinking switch.
///
/// The canonical form is `{"thinking":{"type":"disabled"}}`. The near misses are
/// not equivalent: `{"thinking":false}` is a 422 and `{"enable_thinking":false}`
/// is accepted but does nothing (spec §7), so a provider either speaks this form
/// or is not sent the field at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingSwitch {
    /// The endpoint understands `{"thinking":{"type":"disabled"}}`.
    Canonical,
    /// The endpoint has no switch we know of; thinking is left alone.
    Unsupported,
}

/// Whether an endpoint authenticates at all.
///
/// A hosted API rejects a keyless request, and the not-configured exit code
/// should say so before a request is made. A loopback runtime commonly
/// authenticates nothing, and demanding a key for it would be a lock with no
/// door: the user would have to invent a secret to talk to their own machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRequirement {
    /// A request without a key is rejected; stop before sending it.
    Required,
    /// The endpoint may authenticate; a key that exists is still sent.
    Optional,
}

/// One shipped endpoint and what is known about its call surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    /// The name used in `app.provider` and in `[providers.<name>]`.
    pub name: &'static str,
    /// The human-readable name, for messages on stderr.
    pub label: &'static str,
    /// The base URL, without the trailing `/chat/completions`.
    pub endpoint: &'static str,
    /// A sensible model, where one exists that does not depend on the machine.
    pub model: Option<&'static str>,
    /// Which call surface the vendor serves.
    pub surface: Surface,
    /// What to assume when the endpoint cannot be probed (tickets/05). A probe
    /// replaces it: the same vendor changes what it accepts, so knowing the
    /// name is not knowing the call surface.
    pub schema: SchemaSupport,
    /// How thinking is turned off, if it can be.
    pub thinking: ThinkingSwitch,
    /// Whether the endpoint needs a key.
    pub key: KeyRequirement,
}

/// Every shipped preset. The first is the default provider.
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "deepseek",
        label: "DeepSeek",
        endpoint: "https://api.deepseek.com/v1",
        model: Some("deepseek-flash"),
        surface: Surface::OpenAi,
        // DeepSeek's `response_format.type` accepts text and json_object only
        // (a probe finds this out; the value here is the offline fallback).
        // The call is still held to the contract — by us, after the fact.
        schema: SchemaSupport::BestEffort,
        // The one endpoint where the canonical form is verified: it drops
        // reasoning_tokens to zero, while {"thinking":false} is a 422.
        thinking: ThinkingSwitch::Canonical,
        key: KeyRequirement::Required,
    },
    Preset {
        name: "openai",
        label: "OpenAI",
        endpoint: "https://api.openai.com/v1",
        // OpenAI's own docs lead with a model in the GPT-6 line for structured
        // outputs; the schema is enforced, so any strict-capable model works.
        model: Some("gpt-6-astra"),
        surface: Surface::OpenAi,
        schema: SchemaSupport::Enforced,
        // Chat Completions has no thinking switch of this shape. If a model
        // reasons, it does so because it is that kind of model.
        thinking: ThinkingSwitch::Unsupported,
        key: KeyRequirement::Required,
    },
    Preset {
        name: "llamacpp",
        label: "llama.cpp",
        // llama.cpp's own default. A router on another port is found by the
        // probe in tickets/06, and the endpoint is editable either way.
        endpoint: "http://127.0.0.1:8080/v1",
        // The model comes from the user's server; `/v1/models` lists it.
        model: None,
        surface: Surface::OpenAi,
        // Grammar-constrained sampling, but only through the *nested*
        // response_format.json_schema.schema: the flat shape its own README
        // documents is silently ignored and degrades to "any JSON object".
        schema: SchemaSupport::Enforced,
        thinking: ThinkingSwitch::Unsupported,
        key: KeyRequirement::Optional,
    },
    Preset {
        name: "ollama",
        label: "Ollama",
        endpoint: "http://127.0.0.1:11434/v1",
        // Model tags are whatever has been pulled, e.g. qwen3.5:4b.
        model: None,
        // The native route, where `format` carries structured output; `/v1` is
        // the fallback when the native one is not there (tickets/05).
        surface: Surface::Ollama,
        // Structured outputs are documented on the OpenAI-compatible surface.
        // Thinking is deliberately left alone here: disabling it silently
        // disabled `format` below Ollama 0.31.2, and the local path (tickets/06)
        // is where that version floor belongs.
        schema: SchemaSupport::Enforced,
        thinking: ThinkingSwitch::Unsupported,
        key: KeyRequirement::Optional,
    },
    Preset {
        name: "lmstudio",
        label: "LM Studio",
        endpoint: "http://127.0.0.1:1234/v1",
        model: None,
        surface: Surface::OpenAi,
        // `json_object` is rejected here with a 400, so a JSON Schema is the
        // only form that constrains anything.
        schema: SchemaSupport::Enforced,
        thinking: ThinkingSwitch::Unsupported,
        key: KeyRequirement::Optional,
    },
];

/// The preset for a provider name, if Plainly ships one.
pub fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.name == name)
}

/// The preset names, for a message that lists what can be chosen.
pub fn names() -> Vec<&'static str> {
    PRESETS.iter().map(|preset| preset.name).collect()
}
