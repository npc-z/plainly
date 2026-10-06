//! The command line surface.

use clap::{Parser, Subcommand};

/// Explain hard English in English.
#[derive(Debug, Parser)]
#[command(
    name = "plainly",
    version,
    about = "Explain hard English in English",
    long_about = "Plainly takes a Passage of hard English and returns an Explanation: \
                  a leveled paraphrase, plain-English glosses of what blocks it, an \
                  optional grammar note, and a translation.\n\n\
                  Configuring a provider and its key:\n  \
                  plainly config set app.provider deepseek\n  \
                  printf %s \"$KEY\" | plainly providers key set deepseek",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Read and write the configuration file
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Providers: API keys today, discovery and capability probes next
    Providers {
        #[command(subcommand)]
        command: ProviderCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the configuration file's path
    Path,
    /// Print the effective configuration, defaults included
    Show,
    /// Set one key, for example: plainly config set app.level A2
    Set {
        /// A dotted key, as listed by `plainly config show`
        key: String,
        /// The new value; quote it if your shell needs you to
        value: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProviderCommand {
    /// Manage the API key of one provider
    Key {
        #[command(subcommand)]
        command: KeyCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum KeyCommand {
    /// Read a key from stdin and store it in the OS keyring
    Set {
        /// Provider name, matching [providers.<name>]
        name: String,
    },
    /// Forget a key that was stored in the OS keyring
    Clear {
        /// Provider name, matching [providers.<name>]
        name: String,
    },
}
