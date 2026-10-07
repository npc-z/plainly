//! Turning parsed arguments into behaviour, and behaviour into exit codes.

mod config;
mod explain;
mod providers;

use clap::Parser;

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

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::Usage(message)
            | CommandError::NotConfigured(message)
            | CommandError::Failed(message) => f.write_str(message),
        }
    }
}
