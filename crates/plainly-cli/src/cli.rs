//! The command line surface.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use plainly_core::ExportFormat;

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
    /// The stored Explanations: everything Plainly has already explained
    History {
        #[command(subcommand)]
        command: Option<HistoryCommand>,
    },
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
    /// Explain what the clipboard holds instead of stdin or a file
    ///
    /// The clipboard is read through `wl-paste` (wl-clipboard) and handed to
    /// core as data. Content a password manager has marked secret — the MIME
    /// type `x-kde-passwordManagerHint` with the value `secret` — is refused:
    /// nothing is sent, nothing is stored, and the exit code is 4. A manager
    /// that does not use that convention is not covered by it.
    #[arg(long)]
    pub clipboard: bool,
    /// What to write to stdout; markdown when omitted
    #[arg(long, value_enum)]
    pub format: Option<OutputFormat>,
    /// Ignore the appendix and the descriptor overrides in [prompts] for this
    /// run, and ask with the shipped factory prompt only
    ///
    /// This is the command line's form of the panel's "retry with the factory
    /// prompt": when a contract failure names the prompt, this is the one-step
    /// way to find out whether the user's own rules are what the model cannot
    /// follow. The Explanation is still stored — under the factory prompt's
    /// version, which is what produced it — and the configuration is left alone.
    #[arg(long)]
    pub factory_prompt: bool,
    /// Ask the provider again even though this exact question is already stored
    ///
    /// The stored Explanation is overwritten in place, and keeps the moment it
    /// was created and the moment it was last seen: a regeneration is the same
    /// record with a new answer, not a new record.
    #[arg(long)]
    pub regenerate: bool,
}

impl ExplainArgs {
    /// Fold the root command's copy of these arguments in behind this one, so
    /// `plainly --format json explain passage.md` is not a silently dropped
    /// flag. An `Option` rather than a defaulted value is what makes "the
    /// subcommand stated it" knowable; anything it stated wins.
    pub fn with_root(self, root: ExplainArgs) -> Self {
        Self {
            file: self.file.or(root.file),
            clipboard: self.clipboard || root.clipboard,
            format: self.format.or(root.format),
            factory_prompt: self.factory_prompt || root.factory_prompt,
            regenerate: self.regenerate || root.regenerate,
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

/// The `history` subcommands. A bare `plainly history` lists, because that is
/// what looking at the history almost always means.
#[derive(Debug, Subcommand)]
pub enum HistoryCommand {
    /// List the stored Explanations, most recently seen first (the default)
    List {
        /// Only the Records carrying this tag
        #[arg(long)]
        tag: Option<String>,
    },
    /// Show one stored Explanation as the five sections `explain` prints
    Show {
        /// The id `plainly history list` prints
        id: i64,
    },
    /// Find stored Explanations by a word in the Passage, the Comprehensible
    /// English or a Gloss
    ///
    /// The Translation and the Grammar note are not searched: the Translation is
    /// Chinese and v0 searches English only. Every word of the query has to
    /// appear somewhere — the Passage, the Comprehensible English, a Gloss
    /// expression, a Gloss — and each is matched by prefix.
    Search {
        /// What to look for
        query: String,
        /// Only the Records carrying this tag
        #[arg(long)]
        tag: Option<String>,
    },
    /// Tag one stored Explanation with a word of your own
    ///
    /// Tags are the learner's: nothing infers one, and nothing is tagged unless
    /// you say so.
    Tag {
        /// The id `plainly history list` prints
        id: i64,
        /// The Tag; quote it if it has spaces
        tag: String,
    },
    /// Take a Tag off one stored Explanation
    Untag {
        /// The id `plainly history list` prints
        id: i64,
        /// The Tag to remove
        tag: String,
    },
    /// Delete one stored Explanation, for good
    ///
    /// Nothing is left behind: no tombstone, and no way back.
    Delete {
        /// The id `plainly history list` prints
        id: i64,
    },
    /// Delete every stored Explanation
    ///
    /// There is no way back, so it takes a second act: without `--yes` it says
    /// what it would remove and stops.
    Clear {
        /// Confirm that every Record should go
        #[arg(long)]
        yes: bool,
    },
    /// Write the stored Explanations to stdout as one document
    ///
    /// The product goes to stdout and nothing else does, so it can be redirected
    /// or piped: `plainly history export anki > cards.csv`.
    Export {
        /// What to write: `markdown` to read, `anki` for one card per Gloss,
        /// `raw` for one row per Record
        // Parsed by core's own `FromStr` rather than a second enum with the same
        // three variants: `app.export_format` and this argument name one closed
        // set of shapes, and two spellings of it is how they come to disagree.
        // There is no `.apkg` (spec §9) — writing one means implementing half of
        // Anki's importer, and Anki already maps CSV fields onto its own. A plain
        // comment, so a maintainer reads it and the help text does not.
        #[arg(value_parser = |text: &str| text.parse::<ExportFormat>())]
        format: ExportFormat,
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
    /// Ask the endpoint what it takes, ignoring the cached conclusion
    Probe {
        /// Provider name, matching [providers.<name>]; the configured provider
        /// when omitted
        name: Option<String>,
    },
    /// Look for local runtimes on the common ports and list what they serve
    Discover,
    /// Select one model of a local runtime: it is written into
    /// [providers.<name>] and becomes the provider a run uses
    Use {
        /// Provider name, matching [providers.<name>]
        provider: String,
        /// The model id to use, as the runtime lists it
        model: String,
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
