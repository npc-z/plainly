//! `plainly history`: what Plainly has already explained.
//!
//! The listing *is* the product of `list`, so it goes to stdout — the same reason
//! `config show` and `providers` print there. `show` prints the Explanation as the
//! five sections `explain` prints, and the record's provenance to stderr, so a
//! pipeline still sees only the product and a person still learns which model
//! answered, at which Level, under which prompt.
//!
//! Reading the history never touches the network and never needs a provider: the
//! store is local, and a record is all there is.

use plainly_core::{Paths, Record, Store, render};

use crate::cli::HistoryCommand;
use crate::commands::{CommandError, history_store, short_hash};
use crate::exit;

/// How much of the Passage a listing row shows: enough to recognise a paragraph,
/// short enough that one record stays one line.
const SNIPPET: usize = 72;

pub fn run(command: Option<HistoryCommand>) -> Result<u8, CommandError> {
    match command.unwrap_or(HistoryCommand::List) {
        HistoryCommand::List => list(),
        HistoryCommand::Show { id } => show(id),
    }
}

fn list() -> Result<u8, CommandError> {
    let store = open()?;

    for record in store.list()? {
        println!("{}", row(&record));
    }

    Ok(exit::SUCCESS)
}

fn show(id: i64) -> Result<u8, CommandError> {
    let store = open()?;
    let record = store.show(id)?.ok_or_else(|| {
        CommandError::Usage(format!(
            "no record {id}: `plainly history` lists what is stored"
        ))
    })?;

    eprintln!("plainly: {}", provenance(&record));
    print!(
        "{}",
        render::markdown(&record.artifact.passage, &record.artifact.explanation)
    );

    Ok(exit::SUCCESS)
}

fn open() -> Result<Store, CommandError> {
    let paths = Paths::discover().map_err(|error| CommandError::Failed(error.to_string()))?;
    history_store(&paths)
}

/// One listing row: the id `show` takes, then the provenance the spec asks every
/// list to carry, then enough of the Passage to recognise it.
///
/// Fields are separated by two spaces so the row is readable and still cuttable,
/// and the Passage is last so the columns before it keep their positions whatever
/// the Passage contains.
fn row(record: &Record) -> String {
    let artifact = &record.artifact;

    format!(
        "{}  {}  {}  {}/{}  thinking {}  {}@{}  {}",
        record.id,
        record.last_seen.to_rfc3339(),
        artifact.level,
        artifact.provider,
        artifact.model,
        artifact.thinking.as_str(),
        artifact.prompt_label,
        short_hash(&artifact.prompt_version),
        snippet(&artifact.passage),
    )
}

/// What `show` says about a record: everything the row carries, plus the two
/// other moments in its life.
///
/// The listing abbreviates the prompt hash because a row is one line; here the
/// whole hash is printed, because this is the one place a person can read the
/// version of the prompt that produced the Explanation.
fn provenance(record: &Record) -> String {
    let artifact = &record.artifact;

    format!(
        "record {} — {}/{} (thinking {}), {}, {}@{}, created {}, generated {}, last seen {}",
        record.id,
        artifact.provider,
        artifact.model,
        artifact.thinking.as_str(),
        artifact.level,
        artifact.prompt_label,
        artifact.prompt_version,
        artifact.created_at.to_rfc3339(),
        artifact.generated_at.to_rfc3339(),
        record.last_seen.to_rfc3339(),
    )
}

/// The Passage on one line, cut to [`SNIPPET`] characters.
fn snippet(passage: &str) -> String {
    let line = passage.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut snippet: String = line.chars().take(SNIPPET).collect();
    if line.chars().count() > SNIPPET {
        snippet.push('…');
    }
    snippet
}
