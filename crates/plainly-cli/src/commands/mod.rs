//! Turning parsed arguments into behaviour, and behaviour into exit codes.

mod config;
mod explain;
mod history;
mod providers;

use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

use plainly_core::{
    Cache, KeyRequirement, Paths, ProviderSetup, SchemaSupport, Secrets, Store, Timestamp,
};

use crate::cli::{Cli, Command};
use crate::exit;

/// One thing that went wrong, and how it should be reported.
#[derive(Debug)]
pub enum CommandError {
    /// The caller asked for something that is not a valid thing to ask.
    Usage(String),
    /// There is nothing to talk to: no key where one is needed, or no provider
    /// configured. Distinct from a failure because the fix is different: this
    /// one is answered by configuration, not by trying again.
    NotConfigured(String),
    /// Plainly tried and could not.
    Failed(String),
}

impl CommandError {
    fn exit_code(&self) -> u8 {
        match self {
            CommandError::Usage(_) => exit::USAGE,
            CommandError::NotConfigured(_) => exit::NOT_CONFIGURED,
            CommandError::Failed(_) => exit::FAILURE,
        }
    }
}

/// Configuration failures, mapped onto the documented codes.
///
/// A configuration file that cannot be read as TOML is a *configuration*
/// problem, so it takes `NOT_CONFIGURED`: a script asking "is this machine set
/// up?" gets the same answer from a corrupt file as from a missing key. The
/// remaining gap is `Io` and `ExternallyModified` — "the file could not be
/// written" — which still land on `FAILURE`, because the five documented codes
/// have no slot for local state that would not save. Recorded in the ticket
/// rather than settled by inventing a sixth code here.
impl From<plainly_core::ConfigError> for CommandError {
    fn from(error: plainly_core::ConfigError) -> Self {
        use plainly_core::ConfigError;
        match error {
            ConfigError::UnknownKey { .. }
            | ConfigError::Value { .. }
            | ConfigError::Invalid { .. } => CommandError::Usage(error.to_string()),
            ConfigError::Parse { .. } => CommandError::NotConfigured(error.to_string()),
            ConfigError::Io { .. } | ConfigError::ExternallyModified { .. } => {
                CommandError::Failed(error.to_string())
            }
        }
    }
}

/// The history store failing is local state rather than anything the provider
/// did, and the five documented codes have no slot for "the file on disk will not
/// work": it lands on the failure code, with the store's own message — which
/// names the path wherever the path is known.
impl From<plainly_core::StoreError> for CommandError {
    fn from(error: plainly_core::StoreError) -> Self {
        CommandError::Failed(error.to_string())
    }
}

/// Run the command line and return the process exit code.
pub fn dispatch() -> u8 {
    let cli = Cli::parse();

    let outcome = match cli.command {
        // No subcommand is `explain`, so `echo … | plainly` is the shortest path
        // from a Passage to an Explanation and no one has to learn a verb first.
        None => explain::run(cli.explain),
        // `explain`'s arguments are accepted on both sides of the verb; the
        // subcommand's word wins, and the root's fills anything it left unsaid.
        Some(Command::Explain(args)) => explain::run(args.with_root(cli.explain)),
        Some(Command::History { command }) => history::run(command),
        Some(Command::Config { command }) => config::run(command),
        Some(Command::Providers { command }) => providers::run(command),
    };
    match outcome {
        Ok(code) => code,
        Err(error) => {
            eprintln!("plainly: {error}");
            error.exit_code()
        }
    }
}

/// How an endpoint is held to the contract, in the words a person reads.
///
/// One source for both surfaces that say it — `explain` while a run is in
/// flight and `providers` when it is not — because the two tiers are what the
/// user is being asked to tell apart, and two copies of the wording would
/// eventually disagree.
pub(crate) fn tier(schema: SchemaSupport) -> &'static str {
    match schema {
        SchemaSupport::Enforced => "enforced by the endpoint (response_format json_schema)",
        SchemaSupport::BestEffort => "best effort (json_object, then our own validation and retry)",
    }
}

/// How much of a prompt hash fits in a one-line note. The hash is what tells two
/// prompts with the same label apart — an appendix changes only the hash — and
/// eight hex characters are more than a person needs to see that two differ.
pub(crate) const SHORT_HASH: usize = 8;

/// The first [`SHORT_HASH`] characters of a prompt hash, taken by character
/// rather than by byte so a hand-edited file cannot split one.
///
/// Shared by the two places that abbreviate a version for a one-line report, so
/// they abbreviate it the same way.
pub(crate) fn short_hash(hash: &str) -> String {
    hash.chars().take(SHORT_HASH).collect()
}

/// Where the capability conclusions live: under `appCacheDir`, beside — never
/// inside — the user's configuration (spec §8).
pub(crate) fn cache(paths: &Paths) -> Cache {
    Cache::new(paths.cache_dir().join("providers"))
}

/// The history store: under `appDataDir`, where durable records live (spec §8).
///
/// Opening it creates the file and its directory when they are not there yet, so
/// a store that cannot be opened is local state the user has to fix rather than a
/// provider problem.
pub(crate) fn history_store(paths: &Paths) -> Result<Store, CommandError> {
    Ok(Store::open(paths.history_file())?)
}

/// The current instant, for stamping an Artifact or a probe conclusion.
pub(crate) fn now() -> Result<Timestamp, CommandError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CommandError::Failed(format!("the system clock is before 1970: {error}")))?
        .as_secs();

    i64::try_from(seconds)
        .ok()
        .and_then(Timestamp::from_unix_seconds)
        .ok_or_else(|| {
            CommandError::Failed(
                "the system clock is outside the range Plainly can stamp".to_string(),
            )
        })
}

/// The key to send, if there is one.
///
/// A tier that fails to answer is normally an error rather than a skip, because
/// quietly using a different credential is worse than saying so ([`Secrets`]).
/// An endpoint that may authenticate nothing is the exception: a local runtime
/// is exactly what a headless box points at, there is no key to protect, and the
/// endpoint still gets to answer 401 if it wanted one. A provider that needs a
/// key keeps the failure, because there the fix is the keyring.
pub(crate) fn resolve_key(
    setup: &ProviderSetup,
    secrets: &Secrets,
) -> Result<Option<String>, CommandError> {
    match secrets.resolve(&setup.name) {
        Ok(key) => Ok(key.map(|key| key.secret)),
        Err(error) if setup.key == KeyRequirement::Optional => {
            eprintln!(
                "plainly: {error}; sending no key, since {} may not need one",
                setup.label
            );
            Ok(None)
        }
        Err(error) => Err(CommandError::Failed(error.to_string())),
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::Usage(message)
            | CommandError::NotConfigured(message)
            | CommandError::Failed(message) => f.write_str(message),
        }
    }
}
