//! Turning parsed arguments into behaviour, and behaviour into exit codes.

mod config;
mod explain;
mod history;
mod providers;

use clap::Parser;

use plainly_core::{
    Cache, CredentialError, CredentialState, Paths, ProviderSetup, SchemaSupport, Secrets,
    Store, resolve_credential,
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
    /// Plainly refused the input on purpose — the clipboard is marked sensitive,
    /// or its marker could not be read — so nothing was sent and nothing was
    /// stored. Distinct from a failure because a script branching on the exit
    /// code has to be able to tell "we chose not to" from "it went wrong"
    /// (spec §10, §11).
    Refused(String),
}

impl CommandError {
    fn exit_code(&self) -> u8 {
        match self {
            CommandError::Usage(_) => exit::USAGE,
            CommandError::NotConfigured(_) => exit::NOT_CONFIGURED,
            CommandError::Failed(_) => exit::FAILURE,
            CommandError::Refused(_) => exit::REFUSED,
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
///
/// The exception is the store saying that what was asked for is not there: an id
/// that names no Record, or a Tag with nothing in it. Those are mistakes in the
/// asking, which is what code 2 is for.
impl From<plainly_core::StoreError> for CommandError {
    fn from(error: plainly_core::StoreError) -> Self {
        use plainly_core::StoreError;
        match error {
            StoreError::NoRecord { id } => no_record(id),
            StoreError::EmptyTag { .. } => CommandError::Usage(
                "a tag cannot be blank: it needs at least one character that is not a space"
                    .to_string(),
            ),
            StoreError::Open { .. }
            | StoreError::Directory { .. }
            | StoreError::Unusable { .. }
            | StoreError::Sqlite { .. }
            | StoreError::Corrupt { .. } => CommandError::Failed(error.to_string()),
        }
    }
}

/// The wording for an id that names nothing, shared by every subcommand that
/// takes one: the store says a Record is missing, the command line says where the
/// ones that are there are listed.
pub(crate) fn no_record(id: i64) -> CommandError {
    CommandError::Usage(format!(
        "no record {id}: `plainly history` lists what is stored"
    ))
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

/// How a credential failure becomes an exit code and a sentence.
///
/// One mapping for every CLI caller, so the no-key instruction and the broken
/// store failure cannot drift between `explain` and `providers`. Core owns the
/// three states; this owns the wording (ADR 0003).
pub(crate) fn credential_error(error: CredentialError) -> CommandError {
    match error {
        CredentialError::Missing { provider, variable } => CommandError::NotConfigured(format!(
            "no API key for {provider}: export {variable}, or store one with \
             `plainly providers key set {provider}`"
        )),
        CredentialError::Unreadable { .. } => CommandError::Failed(error.to_string()),
    }
}

/// The key to send, if there is one.
///
/// The policy is core's ([`resolve_credential`]): a provider that may
/// authenticate nothing continues without a key when the store cannot answer,
/// and a provider that needs one keeps the failure. This adapts that answer to
/// the CLI's sentences and exit codes.
pub(crate) fn resolve_key(
    setup: &ProviderSetup,
    secrets: &Secrets,
) -> Result<Option<String>, CommandError> {
    match resolve_credential(setup, secrets) {
        Ok(CredentialState::Key(key)) => Ok(Some(key)),
        Ok(CredentialState::NoKey) => Ok(None),
        Ok(CredentialState::Unreadable(source)) => {
            eprintln!(
                "plainly: {source}; sending no key, since {} may not need one",
                setup.label
            );
            Ok(None)
        }
        Err(error) => Err(credential_error(error)),
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::Usage(message)
            | CommandError::NotConfigured(message)
            | CommandError::Failed(message)
            | CommandError::Refused(message) => f.write_str(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plainly_core::{Config, EnvSecrets, SecretError, SecretStore};

    /// A keyring that is present but cannot answer: a headless box with a
    /// session bus and no Secret Service behind it.
    struct BrokenKeyring;

    impl SecretStore for BrokenKeyring {
        fn get(&self, _provider: &str) -> Result<Option<String>, SecretError> {
            Err(SecretError::Backend("no secret service".to_string()))
        }

        fn set(&self, _provider: &str, _secret: &str) -> Result<(), SecretError> {
            Err(SecretError::Backend("no secret service".to_string()))
        }

        fn delete(&self, _provider: &str) -> Result<(), SecretError> {
            Err(SecretError::Backend("no secret service".to_string()))
        }
    }

    fn broken_keyring() -> Secrets {
        Secrets::with_stores(EnvSecrets::empty(), Some(Box::new(BrokenKeyring)))
    }

    fn setup(name: &str, config: &str) -> ProviderSetup {
        let config = Config::parse(config).expect("the fixture is valid TOML");
        ProviderSetup::resolve(name, &config).expect("the fixture provider resolves")
    }

    /// The whole point of `KeyRequirement::Optional`: a local runtime is what a
    /// headless box points at, and a keyring that cannot answer must not be the
    /// thing that stops it.
    #[test]
    fn a_provider_that_may_authenticate_nothing_survives_a_broken_keyring() {
        let setup = setup("ollama", "[providers.ollama]\nmodel = \"qwen3.5:4b\"\n");

        assert_eq!(resolve_key(&setup, &broken_keyring()).unwrap(), None);
    }

    /// A provider that needs a key keeps the failure: there the keyring is the
    /// thing to fix, and sending an unauthenticated request would hide that.
    #[test]
    fn a_provider_that_needs_a_key_still_reports_the_keyring() {
        let setup = setup("deepseek", "");

        let error = resolve_key(&setup, &broken_keyring()).unwrap_err();

        assert!(matches!(error, CommandError::Failed(_)), "got {error:?}");
        assert!(error.to_string().contains("no secret service"), "{error}");
    }
}
