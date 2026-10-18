//! Whether a provider can be used right now: does it resolve from the
//! configuration, does it need a key, is a key there (and from which tier) or is
//! the store unable to answer, and is its endpoint on this machine.
//!
//! The same question used to be answered by each surface on its own, and the
//! answers had already drifted — the panel took the key requirement from the
//! preset's name while the run path took it from the endpoint-filtered setup, and
//! the desktop treated an exported key as usable before asking whether it
//! resolves. This value composes the existing core facts once ([`Readiness::of`])
//! and lets a surface translate them into its own words (ADR 0004).
//!
//! It is deliberately not a probe: capability is a different axis, and a display
//! path must not turn into a request nobody asked for.

use crate::config::Config;
use crate::presets::KeyRequirement;
use crate::secrets::{KeySource, ResolvedKey, SecretError, Secrets};
use crate::setup::{self, ProviderSetup, SetupError, is_loopback};

/// One provider's availability, as the facts a surface words for itself.
///
/// The setup is kept whole rather than flattened: a surface needs its label,
/// model and endpoint for its own sentence, and `current_problem` is the
/// resolution's error. The credential is kept as the three states a store can be
/// in, never as the secret itself, so the value can travel through the
/// interface without carrying anything private.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readiness {
    /// The provider name, as it appears in configuration and env variables.
    pub name: String,
    /// Whether a key has to be found before a request is worth sending.
    ///
    /// The endpoint-filtered setup's requirement when the provider resolves.
    /// When it does not, the preset's requirement after the same endpoint
    /// filter, so a name whose endpoint points elsewhere never inherits a
    /// demand for a key that endpoint does not make.
    pub requirement: KeyRequirement,
    /// The setup the run would use, or why there is none.
    pub resolution: Result<ProviderSetup, SetupError>,
    /// What the secret stores said, once.
    ///
    /// When the setup does not resolve the stores are not asked — nothing could
    /// run, and no surface shows the key for a provider it cannot describe — so
    /// [`KeyPresence::Missing`] here means "not asked" rather than "no key".
    pub key: KeyPresence,
    /// Whether the configured endpoint names this machine.
    ///
    /// Taken from the endpoint, not the provider's name, and independent of
    /// whether a setup resolves: a local preset still waiting for its model is
    /// on this machine even though it cannot be used yet.
    pub local: bool,
}

/// What the credential stores said about one provider.
///
/// Missing and unreadable are kept apart because they send a person to different
/// places: one is "set a key", the other is "the place the key should be is
/// broken". Readiness never folds the second into the first, and neither does the
/// run path (`crate::runner::CredentialState`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyPresence {
    /// A key was found, and from which tier.
    Found(KeySource),
    /// Nothing is stored — or, when the setup did not resolve, nothing was
    /// asked. Never "the store is broken": that is [`KeyPresence::Unreadable`].
    Missing,
    /// A store could not answer; its own reason travels.
    Unreadable(SecretError),
}

impl Readiness {
    /// Answer for `name` from the configuration and the secret stores.
    ///
    /// The provider is resolved even when it cannot be used: a surface asks
    /// about the configured provider whether or not the user has finished
    /// setting it up (a local preset with no model is unusable and still has to
    /// be reported as local). The stores are asked exactly once when the setup
    /// resolves; a name that cannot resolve is not worth a keyring round trip.
    pub fn of(name: &str, config: &Config, secrets: &Secrets) -> Readiness {
        let resolution = ProviderSetup::resolve(name, config);
        let requirement = match &resolution {
            Ok(setup) => setup.key,
            Err(_) => setup::requirement_of(name, config),
        };
        // Only a provider that resolves can run, and only then does a surface
        // display its key. Asking the secret stores while the setup is broken
        // would turn drawing the settings screen into a keyring round trip for a
        // name that cannot be used anyway.
        let key = match &resolution {
            Ok(_) => classify(secrets.resolve(name)).presence(),
            Err(_) => KeyPresence::Missing,
        };
        // The resolved setup already knows its endpoint; re-deriving it would
        // parse the provider a second time on the common path.
        let local = match &resolution {
            Ok(setup) => setup.is_local(),
            Err(_) => local_of(name, config),
        };

        Readiness {
            name: name.to_string(),
            requirement,
            resolution,
            key,
            local,
        }
    }

    /// Whether a run of this provider could proceed.
    ///
    /// The resolution has to succeed, and the credential must not block the
    /// requirement. That blocking rule is [`KeyPresence::blocks`]'s, the same one
    /// the run path applies.
    pub fn usable(&self) -> bool {
        self.resolution.is_ok() && !self.key.blocks(self.requirement)
    }
}

impl KeyPresence {
    /// Whether this presence stops a provider with `requirement` from running.
    ///
    /// The one place the blocking rule lives: only a provider that must
    /// authenticate is stopped by a missing key, and an unreadable store is not a
    /// failure for a provider that may authenticate nothing (it continues
    /// without a key and carries the reason). Both [`Readiness::usable`] and
    /// [`crate::runner::resolve_credential`] consume this answer instead of
    /// re-deriving it.
    pub(crate) fn blocks(&self, requirement: KeyRequirement) -> bool {
        requirement == KeyRequirement::Required && !matches!(self, KeyPresence::Found(_))
    }
}

/// Whether `name`'s configured endpoint is on this machine.
///
/// The pure half of [`Readiness`], for callers that only need the marker
/// (provider options, history and detail rows) and must not touch the keyring to
/// get it. An endpoint that cannot be resolved is not local: there is nowhere a
/// request would go.
pub fn local_of(name: &str, config: &Config) -> bool {
    ProviderSetup::endpoint_of(name, config).is_ok_and(|endpoint| is_loopback(&endpoint))
}

/// The one credential judgement the run path and readiness share.
///
/// A found key is a key; a store that could not answer is not the same fact as
/// no key at all. Whether either blocks a run is not carried here: the
/// requirement decides that once, in [`KeyPresence::blocks`], so the two
/// projections ([`crate::runner::resolve_credential`] and [`Readiness::of`])
/// cannot disagree about what missing or unreadable means.
#[derive(Debug)]
pub(crate) enum Judgement {
    /// A key was found, with the secret for the caller that must send it.
    Found(ResolvedKey),
    /// Nothing is stored.
    Missing,
    /// A store could not answer; its own reason travels.
    Unreadable(SecretError),
}

impl Judgement {
    /// The key state a surface reports. The secret is not part of it.
    pub(crate) fn presence(&self) -> KeyPresence {
        match self {
            Judgement::Found(key) => KeyPresence::Found(key.source),
            Judgement::Missing => KeyPresence::Missing,
            Judgement::Unreadable(source) => KeyPresence::Unreadable(source.clone()),
        }
    }

    /// Whether this judgement stops a provider with `requirement` from running.
    ///
    /// Delegates to [`KeyPresence::blocks`] so the rule stays in one place; the
    /// caller does not have to project the judgement into a `KeyPresence` first.
    pub(crate) fn blocks(&self, requirement: KeyRequirement) -> bool {
        self.presence().blocks(requirement)
    }
}

/// Classify what a provider's stores answered.
///
/// This is a fact about the stores, not about the requirement: callers apply
/// [`KeyPresence::blocks`] to decide whether it stops a run.
pub(crate) fn classify(resolved: Result<Option<ResolvedKey>, SecretError>) -> Judgement {
    match resolved {
        Ok(Some(key)) => Judgement::Found(key),
        Ok(None) => Judgement::Missing,
        Err(source) => Judgement::Unreadable(source),
    }
}
