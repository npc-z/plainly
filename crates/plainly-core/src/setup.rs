//! The provider a run will actually talk to: the shipped preset merged with the
//! user's `[providers.<name>]` table.
//!
//! Two sources, one answer. A preset supplies what is known without being told —
//! the endpoint of a known service, its call-surface shape, and a model where
//! one is meaningful. The configuration file supplies what only the user knows,
//! and wins wherever it speaks. A name that is neither a preset nor configured
//! is an error rather than an empty setup, so a typo in `app.provider` is told
//! apart from "there is nothing to talk to".
//!
//! The result carries the provider *profile* the Lookup Key is made of (spec §9)
//! beside everything the wire call needs, so the whole answer travels as one
//! value instead of being reassembled at each call site.

use crate::config::{Config, Thinking};
use crate::presets::{self, KeyRequirement, SchemaSupport, ThinkingSwitch};

/// A provider with every choice resolved: the thing one request is sent to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSetup {
    /// The provider name, as it appears in configuration and env variables.
    pub name: String,
    /// The human-readable name, for messages meant for people.
    pub label: String,
    /// Base URL without the trailing `/chat/completions`.
    pub endpoint: String,
    /// The model id. Part of the Lookup Key, so it is never optional.
    pub model: String,
    /// The user's choice of reasoning mode.
    pub thinking: Thinking,
    /// The shape to start from on the wire.
    pub schema: SchemaSupport,
    /// How thinking is turned off, if at all.
    pub thinking_switch: ThinkingSwitch,
    /// Whether a key has to be found before the request is worth sending.
    pub key: KeyRequirement,
}

/// Why a provider name could not be resolved into a setup.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetupError {
    /// The name is neither shipped nor configured.
    #[error(
        "no provider named {name:?}: it is neither one of the shipped presets ({}) \
         nor a [providers.{name}] table",
        presets::names().join(", ")
    )]
    Unknown { name: String },
    /// A provider with no endpoint anywhere.
    #[error(
        "[providers.{name}] has no endpoint, and {name:?} is not a shipped preset, \
         so there is nowhere to send the request"
    )]
    NoEndpoint { name: String },
    /// A local preset, whose model only the user's machine knows.
    #[error(
        "[providers.{name}] has no model, and the {label} preset does not name one. \
         Set providers.{name}.model to the model to use"
    )]
    NoModel { name: String, label: String },
}

impl ProviderSetup {
    /// Resolve `name` against the presets and the configuration.
    pub fn resolve(name: &str, config: &Config) -> Result<Self, SetupError> {
        let profile = config.providers.get(name);
        let shipped = presets::preset(name);

        if profile.is_none() && shipped.is_none() {
            return Err(SetupError::Unknown {
                name: name.to_string(),
            });
        }

        let configured_endpoint =
            profile.and_then(|profile| configured(profile.endpoint.as_deref()));
        let endpoint = configured_endpoint
            .or_else(|| shipped.map(|preset| preset.endpoint))
            .ok_or_else(|| SetupError::NoEndpoint {
                name: name.to_string(),
            })?;

        // A preset describes one *service's* call surface, not a name. Point the
        // name at a different authority and what the preset knew no longer
        // applies: a `json_object` request to LM Studio is a 400, and the
        // canonical thinking field is a 400 on anything strict. The spec's probe
        // cache is invalidated by an endpoint change for the same reason
        // (spec §7). Until tickets/05 probes the endpoint, an unknown one gets
        // the cautious shape — ask for a schema, send nothing that might be
        // rejected, demand no key — which is also what a provider we ship no
        // preset for gets.
        let preset = shipped.filter(|preset| {
            configured_endpoint.is_none_or(|endpoint| same_service(endpoint, preset.endpoint))
        });

        // Part of the Lookup Key, so an unset model is refused rather than
        // defaulted to something no one chose. The *shipped* preset names it,
        // not the endpoint-filtered one: a model id is a default the user can
        // override, and pointing the endpoint elsewhere does not withdraw it.
        let model = profile
            .and_then(|profile| configured(profile.model.as_deref()))
            .or_else(|| shipped.and_then(|preset| preset.model))
            .ok_or_else(|| SetupError::NoModel {
                name: name.to_string(),
                label: shipped
                    .map(|preset| preset.label)
                    .unwrap_or(name)
                    .to_string(),
            })?;

        // The label names the vendor only while the request actually goes to the
        // vendor's service. After an endpoint override, an error that says
        // "DeepSeek answered HTTP 400" would blame a company the user never
        // talked to; the configured name is the honest thing to print.
        let label = preset.map(|preset| preset.label).unwrap_or(name);

        Ok(Self {
            name: name.to_string(),
            label: label.to_string(),
            endpoint: endpoint.trim_end_matches('/').to_string(),
            model: model.to_string(),
            thinking: profile.map(|profile| profile.thinking).unwrap_or_default(),
            schema: preset
                .map(|preset| preset.schema)
                .unwrap_or(SchemaSupport::Enforced),
            thinking_switch: preset
                .map(|preset| preset.thinking)
                .unwrap_or(ThinkingSwitch::Unsupported),
            // A provider we ship no preset for could be anything: a hosted
            // service that will answer 401, or someone's own box that answers
            // nothing. Demanding a key we cannot know is needed would lock the
            // second out; a key that exists is sent either way, so the first
            // fails with the endpoint's own complaint.
            key: preset
                .map(|preset| preset.key)
                .unwrap_or(KeyRequirement::Optional),
        })
    }
}

/// A configured string, treating an empty one as "not configured".
fn configured(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// Whether two endpoints name the same service: same scheme-less authority
/// (host and port), whatever path each carries.
///
/// `https://api.deepseek.com` and `https://api.deepseek.com/v1` are the same
/// place, so a user who writes out the base URL differently does not lose what
/// the preset knows. A different host or port is a different place, and is
/// treated as unknown.
fn same_service(one: &str, other: &str) -> bool {
    authority(one).eq_ignore_ascii_case(authority(other))
}

/// The authority of a URL: everything after the scheme and before the first
/// path separator. Deliberately crude — this decides how cautious to be, not who
/// to trust, and a full URL parser would be a dependency for one comparison.
fn authority(endpoint: &str) -> &str {
    let after_scheme = endpoint
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(endpoint);
    after_scheme.split('/').next().unwrap_or(after_scheme)
}
