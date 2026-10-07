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

use crate::config::{Config, ProviderProfile, Thinking};
use crate::presets::{self, KeyRequirement, Preset, SchemaSupport, Surface, ThinkingSwitch};

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
    /// Which call surface the endpoint is spoken to on.
    pub surface: Surface,
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
    /// Where `name` would send a request, whether or not a model has been
    /// chosen yet.
    ///
    /// The half of [`ProviderSetup::resolve`] that does not need a model. Local
    /// discovery asks *where* a provider is before it can ask what that runtime
    /// serves, and a local preset deliberately names no model until the user
    /// picks one (tickets/06), so resolution that insisted on a model could not
    /// describe the provider whose model is the thing being chosen.
    pub fn endpoint_of(name: &str, config: &Config) -> Result<String, SetupError> {
        Ok(sources(name, config)?.endpoint)
    }

    /// Resolve `name` against the presets and the configuration.
    pub fn resolve(name: &str, config: &Config) -> Result<Self, SetupError> {
        let Sources {
            profile,
            shipped,
            configured_endpoint,
            endpoint,
        } = sources(name, config)?;

        // A preset describes one *service's* call surface, not a name. Point the
        // name at a different authority and what the preset knew no longer
        // applies: a `json_object` request to LM Studio is a 400, and the
        // canonical thinking field is a 400 on anything strict. The spec's probe
        // cache is invalidated by an endpoint change for the same reason
        // (spec §7). What is left here is the shape to *start* from when the
        // endpoint cannot be probed at all (`crate::probe` takes over the moment
        // it can be): ask for a schema, send nothing that might be rejected,
        // demand no key — which is also what a provider we ship no preset for
        // gets.
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
            endpoint,
            model: model.to_string(),
            thinking: profile.map(|profile| profile.thinking).unwrap_or_default(),
            // The surface is *how Plainly talks to this provider*, not a claim
            // about what the endpoint accepts, so an endpoint override does not
            // withdraw it: someone running Ollama on another host still wants
            // `/api/chat`, and an endpoint that turns out not to serve it is
            // answered by the compatibility fallback in `chat`, not by guessing
            // the route from a host name.
            surface: shipped
                .map(|preset| preset.surface)
                .unwrap_or(Surface::OpenAi),
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

    /// The same setup, with the capability a probe (or its cache) concluded.
    ///
    /// Only the two fields a probe can answer for are replaced. Everything else
    /// is a fact about the run rather than about the endpoint — which model, how
    /// the user wants to think, whether a key is needed — and a probe has no say
    /// in those. Keeping this as a method on the setup is what stops a caller
    /// from rebuilding the whole resolution and silently dropping a field.
    ///
    /// The two values and not a `Capability`: a setup is what a capability is
    /// *about*, and taking the conclusion type as an argument would make the two
    /// modules each other's business for no gain in safety — the pair is exactly
    /// what this method documents itself as replacing.
    pub fn with_capability(&self, schema: SchemaSupport, thinking: ThinkingSwitch) -> Self {
        Self {
            schema,
            thinking_switch: thinking,
            ..self.clone()
        }
    }

    /// Whether this setup talks to a runtime on this machine.
    ///
    /// A fact about the endpoint rather than about the provider's name, for the
    /// same reason the surface is the other way round: `ollama` pointed at a
    /// remote host is not local, and a custom name pointed at `127.0.0.1` is.
    /// Provenance is not only *which* model answered but whether anyone can
    /// vouch for it: the CLI says so beside the model, and the panel's own
    /// warning sentence hangs off the same fact (spec §12, tickets/06, 15).
    pub fn is_local(&self) -> bool {
        is_loopback(&self.endpoint)
    }
}

/// Whether an endpoint names this machine.
///
/// `localhost`, or an IP literal in a loopback range — `127.0.0.0/8` or `::1`.
/// Deliberately not "any private address": a LAN box is somebody else's machine,
/// and a local model is the one whose mistakes nobody can be warned about by a
/// vendor's reputation.
pub fn is_loopback(endpoint: &str) -> bool {
    let host = host(authority(endpoint));

    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// The host part of an authority, without its port and without IPv6 brackets.
///
/// `[::1]:11434` is a host and a port; a bare `::1` is a host with more than one
/// colon and no port, which is the only case the split has to leave alone.
fn host(authority: &str) -> &str {
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }

    match authority.split_once(':') {
        Some((host, port)) if !port.contains(':') => host,
        _ => authority,
    }
}

/// A configured string, treating an empty one as "not configured".
fn configured(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// What one name resolves from, looked up once.
///
/// [`ProviderSetup::endpoint_of`] and [`ProviderSetup::resolve`] ask the same
/// question at different depths — where a provider is, and everything a request
/// to it needs — so they read the same two sources here. One lookup is what stops
/// the two from disagreeing about which endpoint a name means, and it is where
/// the two errors about a name live: a name that is neither shipped nor
/// configured, and one that has nowhere to send a request.
struct Sources<'a> {
    /// The user's table for this name, if there is one.
    profile: Option<&'a ProviderProfile>,
    /// The shipped preset for this name, if there is one.
    shipped: Option<&'static Preset>,
    /// The endpoint as the user wrote it, before the preset fills in for it.
    /// What remains interesting about it after `endpoint` is resolved is whether
    /// it was configured at all: a preset describes one service's call surface,
    /// and pointing the name somewhere else withdraws what it knew.
    configured_endpoint: Option<&'a str>,
    /// The endpoint a request would go to, without its trailing slash.
    endpoint: String,
}

/// Resolve a name into the sources it comes from.
fn sources<'a>(name: &str, config: &'a Config) -> Result<Sources<'a>, SetupError> {
    let profile = config.providers.get(name);
    let shipped = presets::preset(name);

    if profile.is_none() && shipped.is_none() {
        return Err(SetupError::Unknown {
            name: name.to_string(),
        });
    }

    let configured_endpoint = profile.and_then(|profile| configured(profile.endpoint.as_deref()));
    let endpoint = configured_endpoint
        .or_else(|| shipped.map(|preset| preset.endpoint))
        .ok_or_else(|| SetupError::NoEndpoint {
            name: name.to_string(),
        })?
        .trim_end_matches('/')
        .to_string();

    Ok(Sources {
        profile,
        shipped,
        configured_endpoint,
        endpoint,
    })
}

/// Whether two endpoints name the same service: same scheme-less authority
/// (host and port), whatever path each carries.
///
/// `https://api.deepseek.com` and `https://api.deepseek.com/v1` are the same
/// place, so a user who writes out the base URL differently does not lose what
/// the preset knows. A different host or port is a different place, and is
/// treated as unknown. Local discovery uses the same comparison for the questions
/// where two spellings *are* one endpoint — which runtime a name means, and
/// whether a discovered endpoint is somewhere the name did not already point —
/// while its candidate list deliberately keeps two spellings of one service
/// apart (tickets/06).
pub fn same_service(one: &str, other: &str) -> bool {
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
