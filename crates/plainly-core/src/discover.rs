//! Local runtime discovery: which ports Plainly asks, and what answered.
//!
//! v0 does not guess a port (spec §7). It asks a short, shipped list of the
//! ports a local runtime commonly listens on, says what it found, and lets the
//! user pick one — a runtime on none of those ports is reached by configuring
//! its endpoint, which is the same act as configuring any other provider.
//!
//! Three things are deliberately separate here:
//!
//! - [`candidates`] decides *where to look*. The common ports are a constant;
//!   an endpoint the user already configured is added to it, because that is not
//!   a guess either — it is the user's own answer — and a runtime on a port of
//!   their choosing should be listed too.
//! - [`scan`] asks each candidate and keeps the ones that answered with a model
//!   list. A port that refused, timed out, or answered with something that is
//!   not a model list is not a runtime: the answer is what makes it one.
//! - [`select`] turns the result into a selection: which of the runtimes that
//!   answered is the one a provider name means, and which of them is the one a
//!   model makes selectable.
//!
//! Whether a model's reported context is big enough is deliberately *not* here.
//! That is a question about the request's budget, and it lives next to the budget
//! it is derived from ([`crate::chat::context_fits`]); this module only carries
//! what the runtime said.
//!
//! What discovery does *not* decide is which credential a candidate gets. That
//! belongs to the name a candidate would be configured under: a loopback runtime
//! started with `--api-key` — which spec §7 asks of the llama.cpp sidecar —
//! answers 401 without one, and reporting the project's own recommended setup as
//! missing would be a lie. [`ModelLister`] is therefore handed the provider name
//! as well as the endpoint, and the implementation decides; this module never
//! looks a key up itself, and carries none.

use crate::config::Config;
use crate::probe::EndpointModel;
use crate::provider::ProviderError;
use crate::setup::{is_loopback, same_service};

/// A local runtime that answered, and the models it lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalRuntime {
    /// The provider name to configure it under: the shipped preset whose port
    /// this is, or the name the user already configured for this endpoint.
    pub provider: String,
    /// The base URL that answered.
    pub endpoint: String,
    /// The models it lists, in its own order. An empty list is an answer too:
    /// the runtime is up and serves nothing.
    pub models: Vec<EndpointModel>,
}

impl LocalRuntime {
    /// Whether this runtime lists `model`.
    ///
    /// A model id nobody serves is a configuration that fails at the first
    /// Passage, so a selection asks this before it writes anything.
    pub fn serves(&self, model: &str) -> bool {
        self.model(model).is_some()
    }

    /// The entry the runtime lists for `model`, if it lists one.
    pub fn model(&self, model: &str) -> Option<&EndpointModel> {
        self.models.iter().find(|entry| entry.id == model)
    }
}

/// One port a local runtime commonly listens on, and the shipped provider that
/// port usually belongs to.
///
/// The port is evidence, not proof: what makes a runtime a runtime is its
/// answer, and the endpoint is written out explicitly in configuration either
/// way, so a runtime on somebody else's port is still usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommonPort {
    pub port: u16,
    /// The preset name that port usually belongs to, which is also the
    /// `[providers.<name>]` table a selection would be written into.
    pub provider: &'static str,
}

impl CommonPort {
    /// The OpenAI-compatible base URL for this port.
    pub fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }
}

/// The ports Plainly asks, in the order it asks them.
///
/// 3060 is the loopback sidecar port this project runs its own llama.cpp on,
/// 11434 is Ollama's, 8080 is llama.cpp's own default, and 1234 is LM Studio's
/// (spec §7). The order is also the order results are reported in, and the order
/// in which a name that matches more than one of them is resolved.
pub const COMMON_PORTS: [CommonPort; 4] = [
    CommonPort {
        port: 3060,
        provider: "llamacpp",
    },
    CommonPort {
        port: 11434,
        provider: "ollama",
    },
    CommonPort {
        port: 8080,
        provider: "llamacpp",
    },
    CommonPort {
        port: 1234,
        provider: "lmstudio",
    },
];

/// One place worth asking what it serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The provider name this endpoint would be selected under.
    pub provider: String,
    /// The OpenAI-compatible base URL.
    pub endpoint: String,
}

/// The transport half of discovery: one endpoint's model list.
///
/// `Ok` is an endpoint that answered *as a model server* — the list may be
/// empty, which is a runtime that serves nothing — while `Err` is one that did
/// not answer that question at all.
///
/// The provider name travels with the endpoint because a runtime may
/// authenticate: it is the name a credential for this candidate would belong to,
/// and resolving that (or deciding not to) is the implementation's business
/// rather than this module's.
pub trait ModelLister {
    /// The models at `endpoint`, a base URL such as `http://127.0.0.1:11434/v1`,
    /// for the provider name this candidate would be configured under.
    fn models_at(
        &self,
        endpoint: &str,
        provider: &str,
    ) -> Result<Vec<EndpointModel>, ProviderError>;
}

/// Where to look: the common ports, plus every loopback endpoint the user
/// configured.
///
/// One candidate per *name and endpoint*. A port the user pointed a provider at
/// is therefore asked — and named — as they configured it, while a runtime on a
/// port of their choosing is found too. Two names at one endpoint are both kept,
/// because merging them by service would make one of the user's names
/// unselectable; the cost is that an endpoint spelled two ways is asked twice.
pub fn candidates(config: &Config) -> Vec<Candidate> {
    let mut candidates: Vec<Candidate> = COMMON_PORTS
        .iter()
        .map(|port| Candidate {
            provider: port.provider.to_string(),
            endpoint: port.endpoint(),
        })
        .collect();

    for (name, endpoint) in config.providers.iter().filter_map(|(name, profile)| {
        let endpoint = profile.endpoint.as_deref()?.trim().trim_end_matches('/');
        is_loopback(endpoint).then(|| (name.clone(), endpoint.to_string()))
    }) {
        let already = candidates
            .iter()
            .any(|candidate| candidate.provider == name && candidate.endpoint == endpoint);
        if !already {
            candidates.push(Candidate {
                provider: name,
                endpoint,
            });
        }
    }

    candidates
}

/// One candidate that did not answer with a model list, and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unanswered {
    /// The provider name the candidate would be configured under.
    pub provider: String,
    /// The endpoint that was asked.
    pub endpoint: String,
    /// What came back instead. A refused connection says "nothing is listening";
    /// an HTTP status says something *is*, and which answer it did not give.
    pub error: ProviderError,
}

/// What one scan found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan {
    /// The candidates that answered with a model list.
    pub runtimes: Vec<LocalRuntime>,
    /// The candidates that did not, in the order they were asked. Kept because
    /// "nothing is listening" and "something refused us" are different answers:
    /// a surface that reports the first when the second is true offers a fix that
    /// cannot work (tickets/06).
    pub unanswered: Vec<Unanswered>,
}

/// Ask every candidate, in order, and keep both what answered and what did not.
pub fn scan(lister: &dyn ModelLister, candidates: &[Candidate]) -> Scan {
    let mut runtimes = Vec::new();
    let mut unanswered = Vec::new();

    for candidate in candidates {
        match lister.models_at(&candidate.endpoint, &candidate.provider) {
            Ok(models) => runtimes.push(LocalRuntime {
                provider: candidate.provider.clone(),
                endpoint: candidate.endpoint.clone(),
                models,
            }),
            Err(error) => unanswered.push(Unanswered {
                provider: candidate.provider.clone(),
                endpoint: candidate.endpoint.clone(),
                error,
            }),
        }
    }

    Scan {
        runtimes,
        unanswered,
    }
}

/// What a selection should do with what answered under a provider name.
///
/// One name can be answered by more than one runtime — llama.cpp's own port and
/// this project's sidecar port are both `llamacpp` — and a name can point at an
/// endpoint that did not answer. Both cases are decided here rather than at the
/// surface, because the surface's job is the wording and this is the rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection<'a> {
    /// Take this runtime: it is the one the name means and it lists the model.
    Use(&'a LocalRuntime),
    /// The endpoint the user configured did not answer, and this runtime under
    /// the same name did — the one that lists the model when one does, so the
    /// report sends the user to a runtime that can actually serve it. Reported
    /// rather than used: a provider the user pointed somewhere is not moved by a
    /// selection.
    Silent(&'a LocalRuntime),
    /// Another runtime under the same name lists the model, and the name was
    /// pointed somewhere by the user. Reported, for the same reason.
    Elsewhere(&'a LocalRuntime),
    /// The runtime the name means does not list the model, and no other runtime
    /// under that name does either. Reported from its own list.
    Unserved(&'a LocalRuntime),
    /// Nothing answered under the name at all.
    Unanswered,
}

/// Decide what a selection of `model` under `provider` should do.
///
/// `endpoint` is where the name points today, and `configured` says whether that
/// is the user's own answer or the preset's guess. The distinction is what the
/// whole rule turns on:
///
/// - the name's own runtime serves the model → that is the one, whether the name
///   was configured or guessed;
/// - the name merely guessed a port, and another runtime under the same name
///   serves the model → take that one. A preset's port is a guess, the model is
///   what tells same-name runtimes apart, and finding the right port without
///   being told is the point (tickets/06);
/// - the user pointed the name somewhere → never move it. The runtime that has
///   the model is reported instead, and so is one that answered where the user's
///   endpoint did not ([`Selection::Silent`]).
pub fn select<'a>(
    runtimes: &'a [LocalRuntime],
    provider: &str,
    model: &str,
    endpoint: &str,
    configured: bool,
) -> Selection<'a> {
    let named = runtime_for(runtimes, provider, endpoint);
    let serving = || {
        runtimes
            .iter()
            .find(|runtime| runtime.provider == provider && runtime.serves(model))
    };

    match named {
        Some(other) if !selects(other, endpoint, configured) => {
            Selection::Silent(serving().unwrap_or(other))
        }
        Some(named) if named.serves(model) => Selection::Use(named),
        Some(named) if !configured => match serving() {
            Some(elsewhere) => Selection::Use(elsewhere),
            None => Selection::Unserved(named),
        },
        Some(named) => match serving() {
            Some(elsewhere) => Selection::Elsewhere(elsewhere),
            None => Selection::Unserved(named),
        },
        None => Selection::Unanswered,
    }
}

/// The runtime a provider name means, among the ones that answered.
///
/// The endpoint the name points at wins, whether it is spelled the same way or is
/// the same service spelled differently. When that endpoint did not answer, the
/// first runtime that did is the only thing left to consider; whether it may be
/// *selected* is [`selects`]'s question.
fn runtime_for<'a>(
    runtimes: &'a [LocalRuntime],
    provider: &str,
    endpoint: &str,
) -> Option<&'a LocalRuntime> {
    let named = || {
        runtimes
            .iter()
            .filter(|runtime| runtime.provider == provider)
    };

    named()
        .find(|runtime| runtime.endpoint == endpoint)
        .or_else(|| named().find(|runtime| same_service(&runtime.endpoint, endpoint)))
        .or_else(|| named().next())
}

/// Whether a runtime that answered under a name is the one that name meant.
///
/// `configured` says whether the name's endpoint is the user's own answer or the
/// preset's guess, and the user's answer decides: a runtime at another endpoint
/// is not what a name the user pointed somewhere means, however it is spelled.
fn selects(runtime: &LocalRuntime, endpoint: &str, configured: bool) -> bool {
    !configured || same_service(&runtime.endpoint, endpoint)
}
