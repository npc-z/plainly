//! Capability probing: what an endpoint's call surface actually is, and what a
//! run does when it turns out not to be what was assumed.
//!
//! There is deliberately no static capability table. The same vendor changes what
//! it accepts — DeepSeek answers `json_object` and rejects `json_schema` — so a
//! preset is a starting point and a probe is the answer (spec §7). On the first
//! run against an endpoint, Plainly sends a few tiny requests: is a JSON Schema
//! enforced, does the canonical thinking switch work, and what models does the
//! endpoint list. The conclusion is cached under `appCacheDir`, never in the
//! user's configuration file: the file is the user's, the cache is recomputable,
//! and clearing one must not disturb the other.
//!
//! Two things a probe can never do, and the code says so rather than pretending:
//!
//! - it cannot *prove* a thinking switch works, because an endpoint is free to
//!   ignore a field silently (`{"enable_thinking":false}` is exactly that), so
//!   only a preset can claim the canonical spelling and only a 400 can take the
//!   claim away;
//! - it cannot conclude anything from an endpoint that did not answer, so a
//!   transport failure is never written to the cache — the next run asks again —
//!   and the run in hand takes the cautious shape instead.
//!
//! What is done with a conclusion — the run that probes, retries and downgrades
//! — is [`crate::explain::run`]: deciding what an endpoint *is* and deciding what
//! to do about it are different jobs, and only the second one needs the retry
//! policy.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::artifact::Timestamp;
use crate::presets::{SchemaSupport, Surface, ThinkingSwitch};
use crate::provider::{Provider, ProviderError, ProviderErrorKind};
use crate::setup::ProviderSetup;

/// The user message a probe sends.
///
/// It asks for the smallest answer that can still be a JSON object, so a probe
/// costs a handful of tokens. It is a constant rather than a literal at the call
/// site because a test double has to be able to tell a probe from a real request
/// ([`crate::chat`] sends it verbatim).
pub const PROBE_PASSAGE: &str =
    "Reply with exactly this JSON object and nothing else: {\"ok\": true}";

/// The system message a probe sends. Ordinary explanations are not asked for, so
/// an endpoint that rates its own behaviour by the prompt sees a request that is
/// not one.
pub const PROBE_SYSTEM: &str =
    "You are being probed for one JSON object. Answer with the requested JSON and nothing else.";

/// One model an endpoint says it serves.
///
/// The two optional fields are what local runtimes volunteer and hosted services
/// do not: whether the model is loaded right now, and how much context it has.
/// tickets/06 builds the discovery UI on this; a probe only records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointModel {
    /// The model id, exactly as the endpoint spells it.
    pub id: String,
    /// Whether the runtime says the model is loaded. `None` when it does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loaded: Option<bool>,
    /// The context length the runtime reports, if it reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
}

/// One probe request, as the endpoint sees it.
///
/// The fields are the two assumptions worth testing, and nothing else: a probe
/// that carried the whole Explanation contract would be measuring the model
/// rather than the endpoint's call surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeRequest {
    /// The response shape to send: a JSON Schema, or a bare request for JSON.
    pub schema: SchemaSupport,
    /// Whether to send the canonical thinking switch alongside it.
    pub disable_thinking: bool,
}

/// The transport half of a probe.
///
/// `Ok` means the endpoint accepted the shape — not that the answer was good.
/// A probe is an experiment about the call surface, so the answer's content is
/// not read at all.
pub trait ProbeEndpoint {
    /// Send one tiny request shaped as `request`.
    fn probe(&self, request: &ProbeRequest) -> Result<(), ProviderError>;
    /// The models the endpoint lists, in the endpoint's own order.
    fn models(&self) -> Result<Vec<EndpointModel>, ProviderError>;
}

/// Everything a run needs from a transport: one answer and the probe's view of
/// the same endpoint.
///
/// Blanket-implemented, so [`crate::chat::ChatCompletions`] and a test double
/// both qualify without either naming the other.
pub trait Endpoint: Provider + ProbeEndpoint {}

impl<T: Provider + ProbeEndpoint> Endpoint for T {}

/// What a probe concluded about an endpoint's call surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    /// Whether the endpoint enforces a JSON Schema or can only be asked for
    /// JSON and checked here.
    pub schema: SchemaSupport,
    /// Whether the canonical thinking switch is understood.
    pub thinking: ThinkingSwitch,
    /// The models the endpoint listed when it was probed, or `None` when the
    /// list could not be read at all.
    ///
    /// An `Option` rather than an empty vector because the two answers are
    /// different facts: "this endpoint serves nothing" and "nobody could find
    /// out" call for different words, and tickets/06 tells them apart in front
    /// of the user.
    pub models: Option<Vec<EndpointModel>>,
    /// When the probe ran, for a surface that wants to show it.
    pub probed_at: Timestamp,
}

impl Capability {
    /// What to run with when the endpoint cannot be asked at all.
    ///
    /// The best-effort tier, not the schema one: spec §7 says a failed probe
    /// downgrades to `json_object` plus our own validation and retry, and the
    /// spec's asymmetry is deliberate — a schema request is the one that gets
    /// rejected outright, so it is the wrong thing to guess at when nothing is
    /// known.
    pub fn cautious(probed_at: Timestamp) -> Self {
        Self {
            schema: SchemaSupport::BestEffort,
            thinking: ThinkingSwitch::Unsupported,
            models: None,
            probed_at,
        }
    }
}

/// Where the capability a run is using came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// A conclusion written by an earlier probe.
    Cached,
    /// A probe made for this run.
    Probed,
    /// Nothing could be learned, so the cautious shape is in use.
    Assumed,
}

/// The capability to run with, and how it was arrived at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub capability: Capability,
    pub origin: Origin,
    /// Why nothing could be learned: the endpoint did not answer the probe.
    pub unanswered: Option<String>,
    /// The conclusion was not written to the cache, and this is why — either the
    /// write failed, or there was no conclusion worth keeping (an unverified
    /// thinking claim). The run is unaffected; the next one will probe again.
    pub unwritten: Option<String>,
}

/// The capability cache: one JSON file per provider under `appCacheDir`.
///
/// A file rather than a database, and per *provider name* rather than per
/// endpoint: the entry records the endpoint and model it was made for, so
/// changing either in configuration makes it a miss, and a provider the user
/// never probed leaves no file at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cache {
    dir: PathBuf,
}

/// What one cache file holds. Private: the format is an implementation detail,
/// and a file from a future version reads as a miss rather than as an error.
#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    endpoint: String,
    model: String,
    schema: SchemaSupport,
    thinking: ThinkingSwitch,
    #[serde(default)]
    models: Option<Vec<EndpointModel>>,
    probed_at: Timestamp,
}

/// Why a capability could not be cached. Never fatal: the conclusion is
/// recomputable, and a run must not fail because a cache directory is read-only.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cannot write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl Cache {
    /// A cache rooted at `dir` — `appCacheDir`, or a temporary directory in a
    /// test.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The directory the entries live in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The file one provider's conclusion lives in.
    ///
    /// The name is escaped rather than trusted. It comes from configuration or
    /// from a command-line argument, and a `/` or a `..` in it must not be able
    /// to put the cache somewhere else on the disk.
    pub fn file(&self, provider: &str) -> PathBuf {
        let safe: String = provider
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();

        self.dir.join(format!("{safe}.json"))
    }

    /// The cached conclusion for `setup`, if there is one that was made for this
    /// endpoint and model.
    ///
    /// A file that cannot be read, does not parse, or was made for a different
    /// endpoint or model is a miss: the conclusion is recomputable, so anything
    /// unexpected is answered by probing again rather than by failing.
    pub fn load(&self, setup: &ProviderSetup) -> Option<Capability> {
        let text = fs::read_to_string(self.file(&setup.name)).ok()?;
        let entry: Entry = serde_json::from_str(&text).ok()?;

        (entry.endpoint == setup.endpoint && entry.model == setup.model).then_some(Capability {
            schema: entry.schema,
            thinking: entry.thinking,
            models: entry.models,
            probed_at: entry.probed_at,
        })
    }

    /// Write a conclusion for `setup`.
    pub fn store(&self, setup: &ProviderSetup, capability: &Capability) -> Result<(), CacheError> {
        let path = self.file(&setup.name);
        let entry = Entry {
            endpoint: setup.endpoint.clone(),
            model: setup.model.clone(),
            schema: capability.schema,
            thinking: capability.thinking,
            models: capability.models.clone(),
            probed_at: capability.probed_at,
        };
        let document =
            serde_json::to_string_pretty(&entry).map_err(|source| CacheError::Write {
                path: path.clone(),
                source: std::io::Error::other(source),
            })?;

        // The directory is created on the way in; a half-written file would be a
        // corrupt entry, so the write goes through a temporary name in the same
        // directory — per process, because the CLI and the panel can probe the
        // same provider at the same time (`load` treats a partial file as a miss
        // either way).
        let dir = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(dir).map_err(|source| CacheError::Write {
            path: path.clone(),
            source,
        })?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "capability.json".to_string());
        let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));

        // A failure leaves no half-written temporary behind, so a directory that
        // could not be written to does not slowly fill with them.
        let written = fs::write(&tmp, document).and_then(|()| fs::rename(&tmp, &path));
        if let Err(source) = written {
            let _ = fs::remove_file(&tmp);
            return Err(CacheError::Write { path, source });
        }
        Ok(())
    }

    /// Forget one provider's conclusion, so the next run probes again.
    pub fn forget(&self, provider: &str) {
        let _ = fs::remove_file(self.file(provider));
    }
}

/// The capability to run with: a cached conclusion, or a fresh probe.
pub fn capability_for(
    endpoint: &dyn Endpoint,
    setup: &ProviderSetup,
    cache: &Cache,
    now: Timestamp,
) -> Resolution {
    match cache.load(setup) {
        Some(capability) => Resolution {
            capability,
            origin: Origin::Cached,
            unanswered: None,
            unwritten: None,
        },
        None => reprobe(endpoint, setup, cache, now),
    }
}

/// Probe `setup`'s endpoint regardless of what the cache says, and cache the
/// conclusion when there is one worth keeping.
pub fn reprobe(
    endpoint: &dyn Endpoint,
    setup: &ProviderSetup,
    cache: &Cache,
    now: Timestamp,
) -> Resolution {
    // The strongest shape first, whoever the vendor is: the point of probing is
    // that a preset's guess is not evidence.
    let schema = match endpoint.probe(&ProbeRequest {
        schema: SchemaSupport::Enforced,
        disable_thinking: false,
    }) {
        Ok(()) => SchemaSupport::Enforced,
        Err(error) if error.kind() == ProviderErrorKind::UnsupportedParameter => {
            // The documented signal: `json_schema` is a 400 and `json_object` is
            // a 200 on the same endpoint (DeepSeek). The second request is what
            // tells "this endpoint cannot enforce a schema" apart from "this
            // endpoint rejected *something* about the request".
            match endpoint.probe(&ProbeRequest {
                schema: SchemaSupport::BestEffort,
                disable_thinking: false,
            }) {
                Ok(()) => SchemaSupport::BestEffort,
                Err(error) => return assumed(error.to_string(), now),
            }
        }
        Err(error) => return assumed(error.to_string(), now),
    };

    // Only a preset's own claim is worth testing, and only on the route that can
    // carry the field: an endpoint that takes it and ignores it answers 200
    // exactly like one that honours it, so a 200 is not evidence and this request
    // exists to collect a 400.
    //
    // The second half of each arm is whether the switch question was *settled*.
    // An unsettled one still leaves the vendor's claim in the capability — the
    // run in hand uses it, because dropping the switch on a hiccup would turn a
    // dropped connection into "this provider cannot be told to stop thinking" —
    // but it is not something to write into a cache with no TTL (see below).
    let (thinking, settled) = match setup.thinking_switch {
        // A vendor surface whose route does not carry the canonical spelling is
        // not evidence either way, so its claim stands unverified rather than
        // being "confirmed" by a request that never asked.
        ThinkingSwitch::Canonical if setup.surface == Surface::OpenAi => {
            match endpoint.probe(&ProbeRequest {
                schema,
                disable_thinking: true,
            }) {
                Ok(()) => (ThinkingSwitch::Canonical, true),
                Err(error) if error.kind() == ProviderErrorKind::UnsupportedParameter => {
                    (ThinkingSwitch::Unsupported, true)
                }
                Err(_) => (ThinkingSwitch::Canonical, false),
            }
        }
        ThinkingSwitch::Canonical => (ThinkingSwitch::Canonical, false),
        ThinkingSwitch::Unsupported => (ThinkingSwitch::Unsupported, true),
    };

    // Models are discovery rather than capability: an endpoint that does not
    // list them still answers, so a failure here costs a list, not a run. It is
    // kept as `None` rather than swallowed into an empty list, so a surface can
    // say "could not be read" instead of "none".
    let models = endpoint.models().ok();

    let capability = Capability {
        schema,
        thinking,
        models,
        probed_at: now,
    };
    // An unsettled switch question keeps the whole conclusion out of the cache:
    // writing the vendor's unverified claim into it would turn "could not check
    // today" into a permanent "probed" fact. The next run pays one more probe
    // instead, which is the cheaper of the two mistakes.
    let unwritten = if settled {
        cache
            .store(setup, &capability)
            .err()
            .map(|error| error.to_string())
    } else {
        Some("the thinking switch could not be verified, so nothing was cached".to_string())
    };

    Resolution {
        capability,
        origin: Origin::Probed,
        unanswered: None,
        unwritten,
    }
}

/// The cautious shape, with the reason the endpoint could not be asked.
fn assumed(reason: String, now: Timestamp) -> Resolution {
    Resolution {
        capability: Capability::cautious(now),
        origin: Origin::Assumed,
        unanswered: Some(reason),
        unwritten: None,
    }
}
