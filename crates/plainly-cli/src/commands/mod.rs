//! Turning parsed arguments into behaviour, and behaviour into exit codes.

mod config;
mod providers;

use clap::{CommandFactory, Parser};

use crate::cli::{Cli, Command};
use crate::exit;

/// One thing that went wrong, and how it should be reported.
#[derive(Debug)]
pub enum CommandError {
    /// The caller asked for something that is not a valid thing to ask.
    Usage(String),
    /// Plainly tried and could not.
    Failed(String),
}

impl CommandError {
    fn exit_code(&self) -> u8 {
        match self {
            CommandError::Usage(_) => exit::USAGE,
            CommandError::Failed(_) => exit::FAILURE,
        }
    }
}

impl From<plainly_core::ConfigError> for CommandError {
    fn from(error: plainly_core::ConfigError) -> Self {
        use plainly_core::ConfigError;
        match error {
            ConfigError::UnknownKey { .. }
            | ConfigError::Value { .. }
            | ConfigError::Invalid { .. } => CommandError::Usage(error.to_string()),
            ConfigError::Io { .. }
            | ConfigError::Parse { .. }
            | ConfigError::ExternallyModified { .. } => CommandError::Failed(error.to_string()),
        }
    }
}

/// Run the command line and return the process exit code.
pub fn dispatch() -> u8 {
    let cli = Cli::parse();

    let outcome = match cli.command {
        // `explain` becomes the default command once it exists; until then,
        // saying nothing is a usage error rather than an invented behaviour.
        // Help goes to stderr: a usage error must leave stdout clean.
        None => {
            eprint!("{}", Cli::command().render_help());
            return exit::USAGE;
        }
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
            CommandError::Usage(message) | CommandError::Failed(message) => f.write_str(message),
        }
    }
}
