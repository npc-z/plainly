//! `plainly history`: what Plainly has already explained, and how to tidy it up.
//!
//! The listing *is* the product of `list`, so it goes to stdout — the same reason
//! `config show` and `providers` print there. `show` prints the Explanation as the
//! five sections `explain` prints, and the record's provenance to stderr, so a
//! pipeline still sees only the product and a person still learns which model
//! answered, at which Level, under which prompt.
//!
//! Everything that changes the history says what it did on stderr and prints
//! nothing on stdout: a deletion is not a product, and a pipeline that asked for
//! one should not have to filter a sentence out of its input. Deleting is final
//! (spec §9), so `clear` asks twice — the command is the first act, `--yes` the
//! second — while `delete` takes the id it was pointed at.
//!
//! Reading the history never touches the network and never needs a provider: the
//! store is local, and a record is all there is.

use plainly_core::{Paths, Record, Store, render};

use crate::cli::HistoryCommand;
use crate::commands::{CommandError, history_store, no_record, short_hash};
use crate::exit;

/// How much of the Passage a listing row shows: enough to recognise a paragraph,
/// short enough that one record stays one line.
const SNIPPET: usize = 72;

/// What every search says out loud, the way the search box's own line does in the
/// main window (spec §9, §13).
///
/// A limit stated is a fact about v0; the same limit discovered is a bug report.
/// The Translation is not indexed — it is not English, and FTS5 tokenizes per
/// table — so a search that found nothing because the word was Chinese and a
/// search that is broken have to be told apart by the person reading.
const NOT_SEARCHABLE: &str =
    "search covers the Passage, the Comprehensible English and the Glosses; \
     the Translation is not searchable in v0";

pub fn run(command: Option<HistoryCommand>) -> Result<u8, CommandError> {
    match command.unwrap_or(HistoryCommand::List { tag: None }) {
        HistoryCommand::List { tag } => list(tag),
        HistoryCommand::Show { id } => show(id),
        HistoryCommand::Search { query, tag } => search(&query, tag),
        HistoryCommand::Tag { id, tag } => add_tag(id, &tag),
        HistoryCommand::Untag { id, tag } => remove_tag(id, &tag),
        HistoryCommand::Delete { id } => delete(id),
        HistoryCommand::Clear { yes } => clear(yes),
    }
}

fn list(tag: Option<String>) -> Result<u8, CommandError> {
    let store = open()?;
    print(&store.search("", tag.as_deref())?);

    Ok(exit::SUCCESS)
}

fn search(query: &str, tag: Option<String>) -> Result<u8, CommandError> {
    // A search for nothing is a mistake in the asking rather than a query that
    // matches everything: `history` already prints everything, and it does not
    // pretend to have searched.
    if query.trim().is_empty() {
        return Err(CommandError::Usage(
            "a search needs something to look for: `plainly history search <query>`".to_string(),
        ));
    }

    let store = open()?;
    eprintln!("plainly: {NOT_SEARCHABLE}");
    print(&store.search(query, tag.as_deref())?);

    Ok(exit::SUCCESS)
}

fn show(id: i64) -> Result<u8, CommandError> {
    let store = open()?;
    let record = store.show(id)?.ok_or_else(|| no_record(id))?;

    eprintln!("plainly: {}", provenance(&record));
    print!(
        "{}",
        render::markdown(&record.artifact.passage, &record.artifact.explanation)
    );

    Ok(exit::SUCCESS)
}

fn add_tag(id: i64, tag: &str) -> Result<u8, CommandError> {
    let mut store = open()?;
    // The store returns the Tag as it stored it, so the confirmation says what
    // the history now holds rather than what was typed at it.
    let stored = store.tag(id, tag)?;

    eprintln!("plainly: record {id} is tagged {stored:?}");
    Ok(exit::SUCCESS)
}

fn remove_tag(id: i64, tag: &str) -> Result<u8, CommandError> {
    let mut store = open()?;
    let removed = store.untag(id, tag)?;

    eprintln!("plainly: record {id} no longer carries the tag {removed:?}");
    Ok(exit::SUCCESS)
}

fn delete(id: i64) -> Result<u8, CommandError> {
    let mut store = open()?;
    store.delete(id)?;

    eprintln!("plainly: deleted record {id}, and everything that belonged to it");
    Ok(exit::SUCCESS)
}

fn clear(yes: bool) -> Result<u8, CommandError> {
    // Asked before the store is opened: the answer is about the request, not
    // about the history, and a refusal should not depend on the file being
    // readable.
    if !yes {
        return Err(CommandError::Usage(
            "clearing deletes every record in the history and nothing brings them back; \
             run `plainly history clear --yes` to confirm"
                .to_string(),
        ));
    }

    let mut store = open()?;
    let removed = store.clear()?;

    eprintln!("plainly: cleared {removed} records");
    Ok(exit::SUCCESS)
}

fn open() -> Result<Store, CommandError> {
    let paths = Paths::discover().map_err(|error| CommandError::Failed(error.to_string()))?;
    history_store(&paths)
}

fn print(records: &[Record]) {
    for record in records {
        println!("{}", row(record));
    }
}

/// One listing row: the id `show` takes, then the provenance the spec asks every
/// list to carry, then the tags, then enough of the Passage to recognise it.
///
/// Fields are separated by two spaces so the row is readable and still cuttable,
/// and the Passage is last so the columns before it keep their positions whatever
/// the Passage contains. Tags are printed even though they are also a filter:
/// a row that hid them would make `--tag` look like it matched nothing.
fn row(record: &Record) -> String {
    let artifact = &record.artifact;

    format!(
        "{}  {}  {}  {}/{}  thinking {}  {}@{}{}  {}",
        record.id,
        record.last_seen.to_rfc3339(),
        artifact.level,
        artifact.provider,
        artifact.model,
        artifact.thinking.as_str(),
        artifact.prompt_label,
        short_hash(&artifact.prompt_version),
        tag_column(record),
        snippet(&artifact.passage),
    )
}

/// The Tags as the listing's own column, or nothing at all when there are none.
fn tag_column(record: &Record) -> String {
    match tags_text(record, " ") {
        Some(text) => format!("  [{text}]"),
        None => String::new(),
    }
}

/// The Tags as a phrase for a provenance line, or nothing when there are none.
fn tag_phrase(record: &Record) -> String {
    match tags_text(record, ", ") {
        Some(text) => format!(", tagged {text}"),
        None => String::new(),
    }
}

/// The Tags as one piece of text, or `None` when the Record carries none.
///
/// A Tag with a space in it is quoted, because the command line accepts such a
/// Tag and its lines have to be able to say it: `["to read"]` is one Tag, while
/// `[to read]` would read as two.
fn tags_text(record: &Record, separator: &str) -> Option<String> {
    (!record.tags.is_empty()).then(|| {
        record
            .tags
            .iter()
            .map(|tag| match tag.contains(char::is_whitespace) {
                true => format!("{tag:?}"),
                false => tag.clone(),
            })
            .collect::<Vec<_>>()
            .join(separator)
    })
}

/// What `show` says about a record: everything the row carries, plus its tags and
/// the two other moments in its life.
///
/// The listing abbreviates the prompt hash because a row is one line; here the
/// whole hash is printed, because this is the one place a person can read the
/// version of the prompt that produced the Explanation.
fn provenance(record: &Record) -> String {
    let artifact = &record.artifact;

    format!(
        "record {} — {}/{} (thinking {}), {}, {}@{}, created {}, generated {}, last seen {}{}",
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
        tag_phrase(record),
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
