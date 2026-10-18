//! One run's machine state, and one Passage's turn through it.
//!
//! A surface used to assemble the paths, the configuration, the provider, the
//! credential, the prompt, the history store, the capability cache and the
//! instant itself. Two surfaces did it twice, and the assembly had already
//! drifted — a store write failure ended the CLI's batch but only the panel's
//! chunk, and an unreadable keyring was a warning on one side and silence on the
//! other. This module is the one place that state is opened ([`Runner::open`]),
//! and the one place a Passage's turn is decided ([`Runner::explain`]): recall
//! the stored answer, or generate it and remember it, and come back with a value.
//!
//! What stays with the surface is what is genuinely a surface's: where a Passage
//! comes from, how a long input is split, which Passages are duplicates, how
//! progress and failures are worded, and what is drawn. The line is the ADR's
//! ([ADR 0003]): one run's machine state has one owner, because the run policy
//! that had two had already diverged.
//!
//! The clock is still a value. [`Runner::open`] takes the instant from the
//! surface, which produces it with [`Timestamp::now`] — the crate's single
//! system-clock conversion — so nothing below the surface reads the clock.
//!
//! [ADR 0003]: ../../../docs/adr/0003-the-runner-owns-one-runs-machine-state.md

use std::time::Duration;

use crate::artifact::{Artifact, Timestamp};
use crate::chat::ChatCompletions;
use crate::config::{Config, ConfigError, ConfigFile, Level};
use crate::explain::{self, Downgrade, RunFailure};
use crate::paths::{PathError, Paths};
use crate::probe::{Cache, Endpoint, Resolution};
use crate::prompt::Prompt;
use crate::readiness::{Judgement, classify};
use crate::secrets::{SecretError, Secrets, env_var_name};
use crate::setup::{ProviderSetup, SetupError};
use crate::store::{Lookup, LookupKey, Record, Store, StoreError};

/// The connector [`explain::run`] takes, held for the life of one run.
///
/// A named alias rather than the bare boxed closure: the same shape is the
/// transport field, the injection seam and the argument `explain::run` already
/// documents, and spelling it three times would invite the three from drifting.
///
/// The credential is an argument rather than something the closure captured:
/// a `Bypass` turn may talk to a provider other than the configured one, and
/// its key is resolved for that turn (spec §3). `None` means "send no key", not
/// "use the configured key".
type Connect = dyn Fn(&ProviderSetup, Option<&str>) -> Box<dyn Endpoint>;

/// What a run sends as its prompt.
///
/// A choice rather than a [`Prompt`] because the configuration is opened by the
/// runner: only the runner knows the user's appendix and level descriptors by
/// the time the prompt is built. `--factory-prompt` is the one caller that asks
/// for the shipped text instead, for this run only (spec §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptChoice {
    /// The factory text plus the user's appendix and descriptor overrides.
    #[default]
    Configured,
    /// The shipped factory text, with the user's prompt data left out.
    Factory,
}

impl PromptChoice {
    /// The prompt this choice means for `config`.
    fn resolve(self, config: &Config) -> Prompt {
        match self {
            PromptChoice::Configured => Prompt::from_config(&config.prompts),
            PromptChoice::Factory => Prompt::factory(),
        }
    }
}

/// What a turn may reuse, and what it must ask for.
///
/// A turn-level option because it is a property of this Passage's turn rather
/// than of the run: the same opened run answers one Passage from the store and
/// another by asking. `Reuse` is the path both surfaces take today; `Bypass` is
/// the ⟳ path — ignore the store, and optionally ask a different provider.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Recall {
    /// Answer from the history store when the Lookup Key is already there.
    #[default]
    Reuse,
    /// Ignore the store and ask again, overwriting the row in place.
    ///
    /// `provider: Some(name)` resolves that provider's setup and credential for
    /// this turn instead of using the configured one; `None` keeps the
    /// configured provider.
    Bypass { provider: Option<String> },
}

/// One Passage explained, and what is known about how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    /// The Explanation this turn produced, with its provenance.
    pub artifact: Artifact,
    /// The history row the answer is in. `None` means this turn's answer is
    /// *not* stored: a miss whose write failed, or a regeneration whose write
    /// failed while an older Record may still be in place. Deliberately not a
    /// failure: the answer was already paid for (ADR 0003).
    pub record: Option<Record>,
    /// Whether the answer came from the store rather than from the provider.
    pub from_cache: bool,
    /// Whether the setup this turn resolved is on this machine.
    ///
    /// The setup that actually answered: the configured one for `Reuse` and
    /// `Bypass { provider: None }`, the override's for
    /// `Bypass { provider: Some(_) }`. Kept on the turn rather than derived by
    /// the surface from the `Recall` it asked with, because only the runner
    /// knows which setup a turn resolved — a `Bypass` provider whose configured
    /// endpoint is loopback is local, whatever its name suggests.
    ///
    /// On a cache hit nothing actually answers. The value is then the locality
    /// of the setup this turn resolved, and the Lookup Key does not carry the
    /// endpoint, so an endpoint override since the Record was written can make
    /// this describe the current configuration rather than the machine that
    /// produced the stored answer.
    pub local: bool,
    /// The capability the successful attempt ran under, when one was asked for.
    /// `None` on a cache hit: a Record does not carry a tier, and guessing the
    /// tier of a request that was never made would describe the wrong run.
    pub resolution: Option<Resolution>,
    /// The correction the endpoint forced before it answered, if any.
    pub downgrade: Option<Downgrade>,
    /// Facts about this turn that are not failures. `NotStored` is the one
    /// today: the answer exists but the history could not keep it.
    pub notices: Vec<TurnNotice>,
}

/// A structured fact about a run that is not a failure.
///
/// A notice is a value, not a sentence: the surface wording differs — the CLI
/// prints a warning line, the panel paints its status row — while the fact is
/// one. This is the run-level half: a fact about the machine, reported once
/// before the turns. The turn-level half is [`TurnNotice`]; the two are separate
/// types so a surface never has to handle a variant that cannot occur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunNotice {
    /// A provider's credential store could not answer, and that provider is
    /// allowed to send no key. The turn goes ahead keylessly rather than
    /// pretending the key is simply absent.
    KeyringUnavailable { provider: String, reason: String },
}

/// A structured fact about one turn that is not a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnNotice {
    /// The answer was produced but the history could not keep it. The artifact
    /// is still the turn's answer (ADR 0003).
    NotStored { reason: String },
}

/// Why a turn produced no Explanation.
// The failure stays a value rather than a box the surface has to unwrap:
// its fields are what the surface branches on, and a failed turn is a
// once-per-Passage event, not a hot loop — the same trade `explain::run`
// records at its own return site. Boxing only to shrink the enum would put
// an unwrap back in front of every surface.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum TurnFailure {
    /// The probe / retry / downgrade path gave up. It keeps that path's own
    /// account of the failure, and the capability the last attempt was sent
    /// under, so a surface can word the same answer it always did.
    Run(RunFailure),
    /// A `Bypass` turn named a provider that could not be used. The provider is
    /// the one the turn asked for, and the error says whether its setup or its
    /// credential was the problem — two things a person fixes in different
    /// places.
    Unavailable {
        provider: String,
        error: UnavailableError,
    },
    /// The history could not be read, so whether there is a stored answer is
    /// unknown. Not a miss: treating a damaged or locked store as "nothing
    /// stored" would quietly pay for a fresh generation (spec §3). `StoreError`
    /// is not `Clone` or `PartialEq` — it carries `rusqlite::Error` — so the
    /// failure is a value the surface matches rather than one it compares.
    Store(StoreError),
}

/// Why a `Bypass` turn's provider could not be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UnavailableError {
    #[error(transparent)]
    Setup(#[from] SetupError),
    #[error(transparent)]
    Credential(#[from] CredentialError),
}

/// Why a run could not be opened.
///
/// One variant per thing a surface has to say something different about: a
/// machine that cannot name its own directories, a configuration that will not
/// parse, a store that will not open, a provider that does not resolve, and a
/// credential that is missing or unreadable. The last two are the ones a person
/// fixes in different places.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error(transparent)]
    Paths(#[from] PathError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Setup(#[from] SetupError),
    #[error(transparent)]
    Credential(#[from] CredentialError),
}

/// Why the configured provider's key could not be used.
///
/// Missing and unreadable are kept apart because they send the user to different
/// places: one is "set a key", the other is "the place the key should be is
/// broken". Treating the second as the first is how a surface ends up telling
/// someone to set a key they already set.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    // The fact only: which provider, which variable. How to set a key is the
    // surface's wording (ADR 0003) — the panel must not show a CLI command, and
    // the CLI keeps the sentence it always printed.
    #[error("no API key for {provider}: {variable} is not set")]
    Missing { provider: String, variable: String },
    #[error("cannot read the credential for {provider}: {source}")]
    Unreadable {
        provider: String,
        #[source]
        source: SecretError,
    },
}

/// One opened run: the machine state a surface explains a Passage with.
///
/// Opening it is the only way a surface reaches the configuration, the provider,
/// the credential, the store, the capability cache or the prompt on the explain
/// path. The process environment is read once, by [`Runner::open`]; after that
/// nothing here touches it, and the clock always arrives as a value.
pub struct Runner {
    config: Config,
    setup: ProviderSetup,
    /// The configured provider's credential, resolved at open. A `Bypass` turn
    /// that keeps the configured provider reuses it; one that names another
    /// resolves that provider's credential per turn.
    key: Option<String>,
    /// Held rather than consumed at open: a `Bypass` turn resolves the provider
    /// it names against these stores (spec §3).
    secrets: Secrets,
    prompt: Prompt,
    store: Store,
    cache: Cache,
    now: Timestamp,
    notices: Vec<RunNotice>,
    connect: Box<Connect>,
    pause: Box<dyn Fn(Duration)>,
}

impl Runner {
    /// Open the run this machine is configured for.
    ///
    /// The directories and the secret stores come from the process; everything
    /// below takes them as values so a test can open the same run without a home
    /// directory, a keyring or a network.
    pub fn open(prompt: PromptChoice, now: Timestamp) -> Result<Self, RunnerError> {
        let paths = Paths::discover()?;
        let secrets = Secrets::from_process_env();
        Self::open_at(paths, secrets, prompt, now)
    }

    /// Open a run against explicit paths, secret stores and instant.
    ///
    /// This is the test seam the ADR records: the substitution a test actually
    /// needs is the environment, so `paths` and `secrets` are values rather than
    /// a trait to implement.
    pub fn open_at(
        paths: Paths,
        secrets: Secrets,
        prompt: PromptChoice,
        now: Timestamp,
    ) -> Result<Self, RunnerError> {
        // The order is the one a person fixes the failures in: configuration,
        // then the provider it names, then that provider's credential, then the
        // local files, and only then the prompt built from the configuration.
        let config = ConfigFile::open(paths.config_file())?.config()?;
        let setup = ProviderSetup::resolve(&config.app.provider, &config)?;
        let resolved = resolve_credential(&setup, &secrets)?;
        let (key, notices) = match resolved {
            CredentialState::Key(key) => (Some(key), Vec::new()),
            CredentialState::NoKey => (None, Vec::new()),
            CredentialState::Unreadable(source) => (
                None,
                vec![RunNotice::KeyringUnavailable {
                    provider: setup.name.clone(),
                    reason: source.to_string(),
                }],
            ),
        };
        let store = Store::open(paths.history_file())?;
        let cache = Cache::new(paths.cache_dir().join("providers"));
        let prompt = prompt.resolve(&config);

        Ok(Self {
            config,
            setup,
            key,
            secrets,
            prompt,
            store,
            cache,
            now,
            notices,
            // The credential is a parameter of the connector rather than
            // captured: a turn may ask another provider, whose own key was
            // resolved for that turn.
            connect: Box::new(|setup, key| {
                Box::new(ChatCompletions::new(setup.clone(), key.unwrap_or_default()))
            }),
            pause: Box::new(std::thread::sleep),
        })
    }

    /// The run-level notices a surface reports once, before the turns begin.
    ///
    /// A credential store that cannot answer is the only one today: it is a
    /// fact about the machine rather than about a Passage, so it belongs to the
    /// run and is never folded into a worded sentence here.
    pub fn notices(&self) -> &[RunNotice] {
        &self.notices
    }

    /// The provider this run resolved.
    ///
    /// A surface reads the facts it has to print — label, model, thinking,
    /// whether the endpoint is local — from the setup rather than rebuilding a
    /// second answer from the configuration.
    pub fn setup(&self) -> &ProviderSetup {
        &self.setup
    }

    /// The prompt this run sends.
    ///
    /// A surface needs it to word a contract failure and to offer the factory
    /// fallback; the decision of what to send has already been made here.
    pub fn prompt(&self) -> &Prompt {
        &self.prompt
    }

    /// The Lookup Key a [`Recall::Reuse`] turn of `passage` would use.
    ///
    /// Built from the configured provider, the configuration snapshot and the
    /// prompt this run sends, through the same [`explain::request`] →
    /// [`Lookup::from`] path [`Runner::explain`] takes, so the two cannot
    /// disagree. It exists so a surface can plan or deduplicate a batch before it
    /// calls [`Runner::explain`], rather than building a second answer of its own.
    pub fn lookup(&self, passage: &str) -> LookupKey {
        let request = explain::request(passage, &self.config, &self.setup, &self.prompt);
        Lookup::from(&request).key()
    }

    /// The Record a [`Recall::Reuse`] turn of `passage` would recall, if any.
    ///
    /// The same [`Runner::lookup`] key and the same store read
    /// [`Runner::explain`] takes, so the two cannot disagree: a surface plans a
    /// batch — counts the requests it will make and says what it is reusing —
    /// without opening the store itself. A store that cannot answer is an
    /// `Err`, not a miss: a surface that planned on "nothing stored" would pay
    /// for a generation the store never said was needed.
    pub fn cached(&self, passage: &str) -> Result<Option<Record>, StoreError> {
        self.recalled(&self.lookup(passage))
    }

    /// The Level in effect from the configuration snapshot.
    ///
    /// A surface words a contract failure with it; opening the configuration
    /// again would let the message describe a different run than the one that
    /// sent the request.
    pub fn level(&self) -> Level {
        self.config.app.level
    }

    /// Replace the HTTP transport with a scripted endpoint factory.
    ///
    /// [`Runner::open`] and [`Runner::open_at`] install [`ChatCompletions`]. A
    /// test wants to assert the turn rather than the wire, so it hands in the
    /// same connector [`explain::run`] already takes; the connector returns the
    /// existing [`Endpoint`] seam, so this adds no new trait and no server. The
    /// second argument is the credential the turn resolved — `None` when there
    /// is none to send.
    #[doc(hidden)]
    pub fn with_transport(mut self, connect: Box<Connect>) -> Self {
        self.connect = connect;
        self
    }

    /// Replace what happens while waiting before a retry.
    ///
    /// [`Runner::open`] sleeps, which is right for a surface with no progress
    /// line. The CLI reports the backoff as it waits ("asking again in 1s"), and
    /// that wording is the surface's, so it hands in the same closure
    /// [`explain::run`] already takes. The seam is the one [`Runner::with_transport`]
    /// uses for the same reason: the policy is core's, the wording is not.
    #[doc(hidden)]
    pub fn with_pause(mut self, pause: Box<dyn Fn(Duration)>) -> Self {
        self.pause = pause;
        self
    }

    /// Explain one Passage: recall, or generate and remember.
    ///
    /// The request is built by [`explain::request`] from the setup this turn
    /// chose, so the Lookup Key a turn recalls with is the same key every
    /// surface would have made. What a surface decides — which Passage, whether
    /// it is a repeat of an earlier one, what to print — is still the surface's.
    // The failure stays a value rather than a box the surface has to unwrap:
    // its fields are what the surface branches on, and a failed turn is a
    // once-per-Passage event, not a hot loop (the same trade `explain::run`
    // records at its own return site).
    #[allow(clippy::result_large_err)]
    pub fn explain(&mut self, passage: &str, recall: Recall) -> Result<Turn, TurnFailure> {
        let effective = match &recall {
            Recall::Reuse | Recall::Bypass { provider: None } => self.configured(),
            Recall::Bypass {
                provider: Some(provider),
            } => self
                .overridden(provider)
                .map_err(|error| TurnFailure::Unavailable {
                    provider: provider.clone(),
                    error,
                })?,
        };
        let request = explain::request(passage, &self.config, &effective.setup, &self.prompt);
        let key = Lookup::from(&request).key();

        // A recall that fails is the turn's failure, not a miss: a damaged or
        // locked store cannot say whether an answer is already paid for, and
        // asking the provider anyway would hide that behind a billable request.
        // The provider has not been called at this point, so a failure here
        // costs nothing.
        if recall == Recall::Reuse {
            let stored = self.recalled(&key).map_err(TurnFailure::Store)?;
            if let Some(record) = stored {
                return Ok(Turn {
                    artifact: record.artifact.clone(),
                    record: Some(record),
                    from_cache: true,
                    local: effective.setup.is_local(),
                    resolution: None,
                    downgrade: None,
                    notices: Vec::new(),
                });
            }
        }

        let connect = |setup: &ProviderSetup| (self.connect)(setup, effective.key.as_deref());
        let run = match explain::run(
            &effective.setup,
            &request,
            &connect,
            &self.cache,
            self.now,
            &*self.pause,
        ) {
            Ok(run) => run,
            Err(failure) => return Err(TurnFailure::Run(*failure)),
        };

        // A write that fails keeps the answer. The artifact is what the person
        // paid for, and losing it because the history could not keep it is the
        // one outcome ADR 0003 refuses; `NotStored` is a turn notice, not a
        // turn failure.
        match self.store.remember(&key, &run.artifact) {
            Ok(record) => Ok(Turn {
                artifact: record.artifact.clone(),
                record: Some(record),
                from_cache: false,
                local: effective.setup.is_local(),
                resolution: Some(run.resolution),
                downgrade: run.downgrade,
                notices: Vec::new(),
            }),
            Err(error) => Ok(Turn {
                artifact: run.artifact,
                record: None,
                from_cache: false,
                local: effective.setup.is_local(),
                resolution: Some(run.resolution),
                downgrade: run.downgrade,
                notices: vec![TurnNotice::NotStored {
                    reason: error.to_string(),
                }],
            }),
        }
    }

    /// The provider this run resolved, with its eagerly resolved key.
    fn configured(&self) -> Effective {
        Effective {
            setup: self.setup.clone(),
            key: self.key.clone(),
        }
    }

    /// The provider a `Bypass` turn named, resolved for this turn only.
    ///
    /// The setup and the credential are resolved here rather than at open
    /// because the turn is what names the provider (spec §3); a failure is the
    /// turn's, not the run's.
    fn overridden(&mut self, provider: &str) -> Result<Effective, UnavailableError> {
        let setup = ProviderSetup::resolve(provider, &self.config)?;
        let resolved = resolve_credential(&setup, &self.secrets)?;
        let key = match &resolved {
            CredentialState::Key(key) => Some(key.clone()),
            CredentialState::NoKey | CredentialState::Unreadable(_) => None,
        };
        // An unreadable store for a key-optional provider is the same value as
        // at open: not a failure, but not silence either.
        if let CredentialState::Unreadable(source) = &resolved {
            let notice = RunNotice::KeyringUnavailable {
                provider: setup.name.clone(),
                reason: source.to_string(),
            };
            if !self.notices.contains(&notice) {
                self.notices.push(notice);
            }
        }
        Ok(Effective { setup, key })
    }

    /// The stored answer for `key`, if there is one.
    ///
    /// A read error is returned rather than folded into a miss. A store that is
    /// already open but cannot answer is a damaged or locked file, and turning
    /// that into "nothing stored" would make the next step pay for a fresh
    /// generation (spec §3). Only a *write* failure is non-fatal; a surface
    /// decides whether a read failure aborts a batch or fails one chunk.
    fn recalled(&self, key: &LookupKey) -> Result<Option<Record>, StoreError> {
        self.store.recall(key, self.now)
    }
}

/// What one turn asks: a provider and its credential.
///
/// A `Bypass` turn may name a provider other than the configured one, so the
/// pair is chosen per turn rather than read off the `Runner` every time; the
/// configured pair is cloned from the run.
struct Effective {
    setup: ProviderSetup,
    key: Option<String>,
}

/// A provider's credential, as the three states a store can be in.
///
/// `NoKey` and `Unreadable` are different facts: the first is "nothing is
/// stored", the second is "the place it should be stored could not answer". A
/// required provider treats the second as an error rather than as a missing key;
/// a provider that may authenticate nothing continues without a key and carries
/// the reason for a notice.
#[derive(Debug)]
pub enum CredentialState {
    /// A credential was found.
    Key(String),
    /// Nothing is stored, and that is allowed.
    NoKey,
    /// The store could not answer. Only a provider that may authenticate nothing
    /// reaches this state instead of a [`CredentialError`].
    Unreadable(SecretError),
}

/// The credential for `setup`'s provider, as the state a surface has to report.
///
/// One policy for every caller — the runner at open and per `Bypass`, and the
/// CLI's `providers` path — so a keyring that will not answer cannot be read as
/// "no key" in one place and as an error in another. The wording stays with the
/// surfaces: this returns the facts (provider, variable, store error), not the
/// sentence.
///
/// The judgement itself is `crate::readiness::classify`'s, and the blocking rule
/// is `crate::readiness::KeyPresence::blocks`', both shared with
/// [`crate::readiness::Readiness`]: this function only projects them into the run
/// path's state and errors, so the run path and the display path cannot give two
/// rules about a missing or unreadable key.
pub fn resolve_credential(
    setup: &ProviderSetup,
    secrets: &Secrets,
) -> Result<CredentialState, CredentialError> {
    let judgement = classify(secrets.resolve(&setup.name));

    // A found key never blocks, so the tuple's first element is ignored for it;
    // the other two states read the one blocking rule to decide error vs state.
    match (judgement.blocks(setup.key), judgement) {
        (_, Judgement::Found(key)) => Ok(CredentialState::Key(key.secret)),
        (true, Judgement::Missing) => Err(CredentialError::Missing {
            provider: setup.name.clone(),
            variable: env_var_name(&setup.name),
        }),
        (false, Judgement::Missing) => Ok(CredentialState::NoKey),
        (true, Judgement::Unreadable(source)) => Err(CredentialError::Unreadable {
            provider: setup.name.clone(),
            source,
        }),
        (false, Judgement::Unreadable(source)) => Ok(CredentialState::Unreadable(source)),
    }
}
