//! The command line surface.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Explain hard English in English.
#[derive(Debug, Parser)]
#[command(
    name = "plainly",
    version,
    about = "Explain hard English in English",
    long_about = "Plainly takes a Passage of hard English and returns an Explanation: \
                  a leveled paraphrase, plain-English glosses of what blocks it, an \
                  optional grammar note, and a translation.\n\n\
                  With no subcommand it reads a Passage from stdin:\n  \
                  echo \"hard sentence\" | plainly\n\n\
                  Configuring a provider and its key:\n  \
                  plainly config set app.provider deepseek\n  \
                  printf %s \"$KEY\" | plainly providers key set deepseek",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// `explain`'s own arguments, so that the default command can be spelled
    /// without naming it: `plainly --format json` and `plainly passage.md` mean
    /// what `plainly explain …` means. The subcommand form still exists, and the
    /// two are folded together rather than one silently winning.
    #[command(flatten)]
    pub explain: ExplainArgs,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Explain one Passage, from a file or from stdin (the default)
    Explain(ExplainArgs),
    /// Read and write the configuration file
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Providers: what Plainly is talking to, what that endpoint takes, and its
    /// API keys
    Providers {
        #[command(subcommand)]
        command: Option<ProviderCommand>,
    },
}

#[derive(Debug, Args)]
pub struct ExplainArgs {
    /// A file to read the Passage from; stdin when omitted
    pub file: Option<PathBuf>,
    /// What to write to stdout; markdown when omitted
    #[arg(long, value_enum)]
    pub format: Option<OutputFormat>,
}

impl ExplainArgs {
    /// Fold the root command's copy of these arguments in behind this one, so
    /// `plainly --format json explain passage.md` is not a silently dropped
    /// flag. An `Option` rather than a defaulted value is what makes "the
    /// subcommand stated it" knowable; anything it stated wins.
    pub fn with_root(self, root: ExplainArgs) -> Self {
        Self {
            file: self.file.or(root.file),
            format: self.format.or(root.format),
        }
    }
}

/// The product's shape on stdout. Human information goes to stderr either way,
/// so choosing json never changes what a pipeline sees but the product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum OutputFormat {
    /// The five sections, as the prototype renders them
    #[default]
    Markdown,
    /// The Artifact as one JSON document: the Explanation and its metadata
    Json,
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
    /// Ask the endpoint what it takes, ignoring the cached conclusion
    Probe {
        /// Provider name, matching [providers.<name>]; the configured provider
        /// when omitted
        name: Option<String>,
    },
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
