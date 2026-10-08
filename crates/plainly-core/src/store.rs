//! The history store: what Plainly has already explained, and the identity that
//! makes a question the same question.
//!
//! The store is both the learner's history and the cache (spec §9), which is why
//! the identity matters so much: a **Lookup Key** that is too loose hands back an
//! explanation another model produced, and one that is too tight pays for the same
//! Passage twice. The key is the SHA-256 of the normalized Passage plus the whole
//! provider profile and the prompt version — and *not* the artifact version,
//! because a change of data shape does not make an old answer wrong.
//!
//! Two facts about the Passage are deliberately different from each other:
//!
//! - The Passage is **stored verbatim**, because it is the authority a learner
//!   checks the Explanation against.
//! - The Passage is **normalized for identity only** — leading and trailing
//!   whitespace, and CRLF against LF. Interior whitespace is meaning, so
//!   changing it is a different question.
//!
//! A hit reuses the same row and moves it to the top of the history by
//! `last_seen`; there is no `lookup_count`, because Plainly is not a spaced
//! repetition system. A regeneration overwrites the Explanation in place and
//! keeps `created_at` and `last_seen`: the record is the same record, and only
//! what it holds is new.
//!
//! The store is relational rather than a JSON blob because the natural unit of an
//! Anki export is one Gloss, and one SQL query should produce the cards
//! (tickets/10). `ord` keeps the model's order, which is the order of how much
//! each expression blocks comprehension.

use std::fmt;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, Row, params};

use crate::artifact::{Artifact, Timestamp};
use crate::explanation::{Explanation, Gloss};
use crate::hashing::{push_field, sha256_hex};
use crate::provider::ExplainRequest;
use crate::{Level, Thinking};

/// How long a read or write waits for a concurrent writer before giving up. The
/// panel and the main window each hold their own connection (spec §8), so a
/// writer must wait for a writer rather than fail.
const BUSY_TIMEOUT_MS: u32 = 5_000;

/// The schema, created on open. One version of Plainly ships one shape; the
/// `artifact_version` on every row is what tells a later reader how to read an
/// old row, so this file needs no version of its own yet.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS records (
    id               INTEGER PRIMARY KEY,
    lookup_key       TEXT NOT NULL UNIQUE,
    passage          TEXT NOT NULL,
    level            TEXT NOT NULL,
    source_language  TEXT NOT NULL,
    native_language  TEXT NOT NULL,
    provider         TEXT NOT NULL,
    model            TEXT NOT NULL,
    thinking         TEXT NOT NULL,
    artifact_version INTEGER NOT NULL,
    prompt_version   TEXT NOT NULL,
    prompt_label     TEXT NOT NULL,
    comprehensible   TEXT NOT NULL,
    grammar          TEXT,
    translation      TEXT NOT NULL,
    created_at       TEXT NOT NULL,
    generated_at     TEXT NOT NULL,
    last_seen        TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS glosses (
    record_id  INTEGER NOT NULL REFERENCES records(id) ON DELETE CASCADE,
    ord        INTEGER NOT NULL,
    expression TEXT NOT NULL,
    gloss      TEXT NOT NULL,
    PRIMARY KEY (record_id, ord)
);
CREATE TABLE IF NOT EXISTS tags (
    record_id INTEGER NOT NULL REFERENCES records(id) ON DELETE CASCADE,
    tag       TEXT NOT NULL,
    PRIMARY KEY (record_id, tag)
);
-- Two search tables rather than one, because the two things a learner searches
-- for are different sizes: a Passage and its Comprehensible English are one row
-- per Record, while a Gloss is one row per expression. Plain (not external
-- content) tables: they hold their own copy of the text, which is what lets a
-- regeneration delete and re-insert a row without also feeding it the old one.
CREATE VIRTUAL TABLE IF NOT EXISTS record_fts USING fts5(
    passage,
    comprehensible
);
CREATE VIRTUAL TABLE IF NOT EXISTS gloss_fts USING fts5(
    expression,
    gloss,
    record_id UNINDEXED
);
";

/// One Record, joined to its Glosses: one row per Gloss, or a single row with
/// NULL Gloss columns when there are none.
///
/// Reading a Record this way is one statement, so its Glosses belong to the same
/// snapshot as its columns — a `recall` racing a regeneration can come back with
/// the old Explanation or the new one, never half of each.
const READ: &str = "SELECT r.id, r.lookup_key, r.passage, r.level, r.source_language, \
                    r.native_language, r.provider, r.model, r.thinking, \
                    r.artifact_version, r.prompt_version, r.prompt_label, \
                    r.comprehensible, r.grammar, r.translation, r.created_at, \
                    r.generated_at, r.last_seen, \
                    g.expression AS gloss_expression, g.gloss AS gloss_gloss \
                    FROM records r LEFT JOIN glosses g ON g.record_id = r.id";

/// Everything that would make two lookups the same question (spec §9).
///
/// The Passage is the one field that has to be normalized before it counts; every
/// other field is already a choice, and a change to any of them is a different
/// question. `artifact_version` is deliberately absent: bumping the data model
/// must not throw the cache away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lookup<'a> {
    pub passage: &'a str,
    pub level: Level,
    pub source_language: &'a str,
    pub native_language: &'a str,
    pub prompt_version: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub thinking: Thinking,
}

impl<'a> From<&'a ExplainRequest> for Lookup<'a> {
    fn from(request: &'a ExplainRequest) -> Self {
        Self {
            passage: &request.passage,
            level: request.level,
            source_language: &request.source_language,
            native_language: &request.native_language,
            prompt_version: &request.prompt_version,
            provider: &request.provider,
            model: &request.model,
            thinking: request.thinking,
        }
    }
}

impl Lookup<'_> {
    /// The Lookup Key: SHA-256 over the normalized Passage and the profile.
    pub fn key(&self) -> LookupKey {
        let mut material = String::new();
        push_field(&mut material, &normalize(self.passage));
        push_field(&mut material, self.level.as_str());
        push_field(&mut material, self.source_language);
        push_field(&mut material, self.native_language);
        push_field(&mut material, self.prompt_version);
        push_field(&mut material, self.provider);
        push_field(&mut material, self.model);
        push_field(&mut material, self.thinking.as_str());

        LookupKey(sha256_hex(&material))
    }
}

/// The identity of one question, as the store indexes it.
///
/// A type of its own rather than a `String`, so that a Passage or a bare provider
/// name cannot be passed where a key belongs. It is hashable so a caller holding
/// several questions — the CLI's batch — can ask whether two of them are the same
/// question without hashing the Passage again.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LookupKey(String);

impl LookupKey {
    /// The key as it is stored, for a caller that has to print or compare it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LookupKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One row of the history: an Explanation, and what identifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub id: i64,
    pub lookup_key: LookupKey,
    /// When this question was last asked. The history is ordered by it.
    pub last_seen: Timestamp,
    /// The learner's own Tags for this Record, in tag order.
    ///
    /// Tags are added by hand and are the learner's vocabulary, not Plainly's:
    /// nothing infers one from the Passage, because a wrong Tag is worse than no
    /// Tag when the point of a Tag is that the learner chose it.
    pub tags: Vec<String>,
    /// The Passage, the Explanation, and every piece of provenance — including
    /// the Level and Native Language the Explanation was pitched at, which is
    /// what makes changing a setting leave old records alone.
    pub artifact: Artifact,
}

/// Why the history store could not be opened or used.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("cannot open the history store at {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("cannot create {path}: {source}")]
    Directory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The file opened but is not a store this build can use: not SQLite at all,
    /// read-only where WAL has to be written, or a schema that will not create.
    /// It is the likeliest way opening fails, so it says *which* file — the
    /// failures that can only happen later are [`StoreError::Sqlite`].
    #[error("the history store at {path} is not usable: {source}")]
    Unusable {
        path: PathBuf,
        #[source]
        source: rusqlite::Error,
    },
    #[error("the history store could not be read or written: {source}")]
    Sqlite {
        #[source]
        source: rusqlite::Error,
    },
    #[error("record {record} of the history store holds {value} where {expected} was expected")]
    Corrupt {
        /// The row to look at: nothing else in the store identifies it to a
        /// person, and this is the id `history delete` takes.
        record: i64,
        value: String,
        expected: &'static str,
    },
    /// The id names no Record: a deletion or a Tag was asked of something that is
    /// not in the history, which is a mistake in the asking rather than a failure
    /// of the store.
    #[error("there is no record {id} in the history")]
    NoRecord { id: i64 },
    /// A Tag with nothing in it would be a chip that filters nothing.
    #[error("a tag cannot be blank: {tag:?} has nothing in it")]
    EmptyTag { tag: String },
}

impl From<rusqlite::Error> for StoreError {
    fn from(source: rusqlite::Error) -> Self {
        Self::Sqlite { source }
    }
}

/// The history store, open on one SQLite connection.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open (or create) the store at `path`, creating its directory.
    ///
    /// WAL is what lets the panel read while the main window writes; the busy
    /// timeout is what makes a concurrent writer wait instead of failing.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|source| StoreError::Directory {
                path: dir.to_path_buf(),
                source,
            })?;
        }

        let conn = Connection::open(path).map_err(|source| StoreError::Open {
            path: path.to_path_buf(),
            source,
        })?;

        Self::prepare(conn, Some(path.to_path_buf()))
    }

    /// A store that lives in memory and disappears with the value. This is how a
    /// test gets a real store without a real file.
    pub fn in_memory() -> Result<Self, StoreError> {
        Self::prepare(Connection::open_in_memory()?, None)
    }

    fn prepare(conn: Connection, path: Option<PathBuf>) -> Result<Self, StoreError> {
        // Everything that can go wrong here is about the file rather than about
        // a statement, so it names the file; a failure later in the session is a
        // [`StoreError::Sqlite`], which has no path left to add.
        let unusable = |source: rusqlite::Error| match &path {
            Some(path) => StoreError::Unusable {
                path: path.clone(),
                source,
            },
            None => StoreError::Sqlite { source },
        };

        conn.pragma_update(None, "busy_timeout", BUSY_TIMEOUT_MS)
            .map_err(unusable)?;
        // WAL after the timeout, not before: switching the journal mode writes to
        // the file, so it is the statement most likely to meet another process's
        // lock, and it should be the one that waits rather than failing.
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(unusable)?;
        // Deleting a Record has to take its Glosses with it (tickets/09), and a
        // store that only enforces that from Rust is one forgotten call away
        // from orphans.
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(unusable)?;
        conn.execute_batch(SCHEMA).map_err(unusable)?;
        // A store written before the search tables existed has none of their
        // rows. They are derived data — every row in them is a copy of text that
        // lives in `records` or `glosses` — so "empty while the history is not"
        // means exactly that, and rebuilding is a no-op in every other case.
        reindex(&conn).map_err(unusable)?;

        Ok(Self { conn })
    }

    /// The stored Explanation for this question, if it has been asked before.
    ///
    /// A hit is the same record: the row is reused and its `last_seen` moves to
    /// `now`, so the history keeps its most recently asked questions first. The
    /// Passage comes back exactly as it was stored, not as it was normalized.
    ///
    /// The bump and the read are two statements, and whichever of them a
    /// concurrent writer lands between, the answer is a Record the store really
    /// holds — the one from before the write or the one after it. Whether there
    /// *was* a hit is the read's decision, not the write's: an `UPDATE` that sets
    /// a row to the value it already has still counts as a change to SQLite, and
    /// a recall in the same second as the insert is an ordinary thing to do.
    pub fn recall(&self, key: &LookupKey, now: Timestamp) -> Result<Option<Record>, StoreError> {
        self.conn.execute(
            "UPDATE records SET last_seen = ?1 WHERE lookup_key = ?2",
            params![now.to_rfc3339(), key.as_str()],
        )?;

        self.by_key(key)
    }

    /// Store a generated Artifact under its key.
    ///
    /// A key already in the store is overwritten in place — that is what
    /// `--regenerate` means — and the row keeps its `created_at` and
    /// `last_seen`: the record was made when it was made, and the learner has
    /// seen it for as long as they have seen it. Only the Explanation and
    /// `generated_at` are new.
    pub fn remember(&mut self, key: &LookupKey, artifact: &Artifact) -> Result<Record, StoreError> {
        let tx = self.conn.transaction()?;

        tx.execute(
            "INSERT INTO records (
                 lookup_key, passage, level, source_language, native_language,
                 provider, model, thinking, artifact_version, prompt_version,
                 prompt_label, comprehensible, grammar, translation,
                 created_at, generated_at, last_seen
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
             ON CONFLICT(lookup_key) DO UPDATE SET
                 passage          = excluded.passage,
                 level            = excluded.level,
                 source_language  = excluded.source_language,
                 native_language  = excluded.native_language,
                 provider         = excluded.provider,
                 model            = excluded.model,
                 thinking         = excluded.thinking,
                 artifact_version = excluded.artifact_version,
                 prompt_version   = excluded.prompt_version,
                 prompt_label     = excluded.prompt_label,
                 comprehensible   = excluded.comprehensible,
                 grammar          = excluded.grammar,
                 translation      = excluded.translation,
                 generated_at     = excluded.generated_at",
            params![
                key.as_str(),
                &artifact.passage,
                artifact.level.as_str(),
                &artifact.source_language,
                &artifact.native_language,
                &artifact.provider,
                &artifact.model,
                artifact.thinking.as_str(),
                i64::from(artifact.artifact_version),
                &artifact.prompt_version,
                &artifact.prompt_label,
                &artifact.explanation.comprehensible,
                artifact.explanation.grammar.as_deref(),
                &artifact.explanation.translation,
                artifact.created_at.to_rfc3339(),
                artifact.generated_at.to_rfc3339(),
                artifact.created_at.to_rfc3339(),
            ],
        )?;

        let id: i64 = tx.query_row(
            "SELECT id FROM records WHERE lookup_key = ?1",
            [key.as_str()],
            |row| row.get(0),
        )?;

        // The Explanation is replaced whole, so its Glosses are too: a
        // regeneration that changed the number of Glosses must not leave the old
        // ones behind. The search rows are rewritten from the same values in the
        // same loop — a copy kept in step by being made in one place.
        tx.execute("DELETE FROM glosses WHERE record_id = ?1", [id])?;
        tx.execute("DELETE FROM gloss_fts WHERE record_id = ?1", [id])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO glosses (record_id, ord, expression, gloss) VALUES (?1, ?2, ?3, ?4)",
            )?;
            let mut index = tx.prepare(
                "INSERT INTO gloss_fts (expression, gloss, record_id) VALUES (?1, ?2, ?3)",
            )?;
            for (ord, gloss) in artifact.explanation.glosses.iter().enumerate() {
                insert.execute(params![id, ord as i64, &gloss.expression, &gloss.gloss])?;
                index.execute(params![&gloss.expression, &gloss.gloss, id])?;
            }
        }

        // Same for the Record's own row: a regeneration whose Passage changed
        // spelling must not leave the old words findable.
        tx.execute("DELETE FROM record_fts WHERE rowid = ?1", [id])?;
        tx.execute(
            "INSERT INTO record_fts (rowid, passage, comprehensible) VALUES (?1, ?2, ?3)",
            params![
                id,
                &artifact.passage,
                &artifact.explanation.comprehensible
            ],
        )?;

        let record = read_by_id(&tx, id)?.expect("the row was just written");
        tx.commit()?;

        Ok(record)
    }

    /// Every stored Explanation, most recently seen first: the search with
    /// nothing asked of it.
    pub fn list(&self) -> Result<Vec<Record>, StoreError> {
        self.search("", None)
    }

    /// One stored Explanation, if the id is in the store.
    pub fn show(&self, id: i64) -> Result<Option<Record>, StoreError> {
        read_by_id(&self.conn, id)
    }

    /// Tag a Record with the learner's own word for it.
    ///
    /// The Tag is trimmed, because what it was typed into is a text field and its
    /// ends are not part of it; a Tag with nothing left after trimming is refused
    /// rather than stored. Applying the same tag twice is not a mistake —
    /// the second one has nothing left to do.
    ///
    /// The Tag comes back as it was stored, so a caller that reports what it did
    /// reports what the history now holds rather than what it was handed.
    pub fn tag<'a>(&mut self, id: i64, tag: &'a str) -> Result<&'a str, StoreError> {
        let tag = trim_tag(tag)?;
        let tx = self.conn.transaction()?;
        if !exists(&tx, id)? {
            return Err(StoreError::NoRecord { id });
        }

        tx.execute(
            "INSERT OR IGNORE INTO tags (record_id, tag) VALUES (?1, ?2)",
            params![id, tag],
        )?;
        tx.commit()?;

        Ok(tag)
    }

    /// Take one of a Record's Tags off again, returning the Tag that was asked
    /// for, trimmed, the way [`Store::tag`] does.
    ///
    /// Removing a Tag the Record does not carry is not an error: the caller asked
    /// for a Record without that Tag and that is what it now has. Removing a Tag
    /// from a Record that does not exist is an error, because then there is no
    /// Record at all to have an opinion about.
    pub fn untag<'a>(&mut self, id: i64, tag: &'a str) -> Result<&'a str, StoreError> {
        let tag = trim_tag(tag)?;
        let tx = self.conn.transaction()?;
        if !exists(&tx, id)? {
            return Err(StoreError::NoRecord { id });
        }

        tx.execute(
            "DELETE FROM tags WHERE record_id = ?1 AND tag = ?2",
            params![id, tag],
        )?;
        tx.commit()?;

        Ok(tag)
    }

    /// Remove one Record from the history, for good.
    ///
    /// A hard delete (spec §9): no tombstone, no `deleted_at`, nothing left to
    /// find. The Passage, the Explanation, the Glosses, the Tags and the search
    /// rows go together, because a Record that keeps any one of them is not gone:
    /// it still answers a search, or it hands the next Record the words of a
    /// Passage its learner threw away.
    pub fn delete(&mut self, id: i64) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        if !exists(&tx, id)? {
            return Err(StoreError::NoRecord { id });
        }

        // The search tables first: once the Record is gone there is no id left to
        // aim at its Gloss rows.
        tx.execute("DELETE FROM record_fts WHERE rowid = ?1", [id])?;
        tx.execute("DELETE FROM gloss_fts WHERE record_id = ?1", [id])?;
        // The Glosses and the Tags go with the Record: both reference it with
        // ON DELETE CASCADE, and `foreign_keys` is on.
        tx.execute("DELETE FROM records WHERE id = ?1", [id])?;
        tx.commit()?;

        Ok(())
    }

    /// Remove every Record, and say how many there were.
    ///
    /// The count is what a caller can repeat back: clearing is the one act here
    /// that cannot be aimed at anything, so it reports what it did rather than
    /// trusting that it was understood.
    pub fn clear(&mut self) -> Result<usize, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM record_fts", [])?;
        tx.execute("DELETE FROM gloss_fts", [])?;
        let removed = tx.execute("DELETE FROM records", [])?;
        tx.commit()?;

        Ok(removed)
    }

    /// The stored Explanations whose English matches `query`, most recently seen
    /// first, narrowed to one Tag when one is given.
    ///
    /// Two tables answer: one holds the Passage and the Comprehensible English,
    /// the other holds each Gloss's expression and its words. A record both
    /// answer for appears once — the history is a list of Records, not of
    /// matches. Every term of the query has to be held somewhere, though not
    /// necessarily in the same field. The Translation and the Grammar note are
    /// deliberately not indexed (spec §9): `translation` is Chinese, and the
    /// tokenizer is a property of the table, so searching it would need a third
    /// table with a tokenizer of its own. That is a stated fact of v0 rather than
    /// a defect to find later, and the surfaces that offer a search box say so.
    ///
    /// An empty query is not an error and not a miss: it matches everything,
    /// which is what a search box holds before anything has been typed into it.
    /// Terms are matched by prefix, because the box is typed into. The order is
    /// `last_seen`, most recent first, with `id` breaking ties — `last_seen` is
    /// only accurate to the second, and two questions asked in the same second
    /// still have an order.
    pub fn search(&self, query: &str, tag: Option<&str>) -> Result<Vec<Record>, StoreError> {
        let terms = match_terms(query);
        let tag = tag.map(trim_tag).transpose()?;
        let mut clauses: Vec<String> = Vec::new();
        let mut values: Vec<&dyn rusqlite::ToSql> = Vec::new();

        // A clause per term, and the clauses are ANDed. One `a AND b` expression
        // would not do: `MATCH` is evaluated per table, so it would ask the
        // Passage and the Glosses each for *both* words, and a query with one word
        // in each would find nothing. Asked this way, every term has to be held
        // somewhere, which is what the surfaces promise.
        for term in &terms {
            // One number, used by both sub-selects: they are the same term asked
            // of two tables.
            let at = values.len() + 1;
            clauses.push(format!(
                "r.id IN (SELECT rowid FROM record_fts WHERE record_fts MATCH ?{at} \
                 UNION SELECT record_id FROM gloss_fts WHERE gloss_fts MATCH ?{at})"
            ));
            values.push(term);
        }

        // `as_ref`, not `if let Some(tag) = tag`: the value bound has to be the
        // reference itself, since `ToSql` is implemented for `&str` rather than
        // for the unsized `str` behind it.
        if let Some(tag) = tag.as_ref() {
            let at = values.len() + 1;
            clauses.push(format!(
                "r.id IN (SELECT record_id FROM tags WHERE tag = ?{at})"
            ));
            values.push(tag);
        }

        let filter = match clauses.is_empty() {
            true => String::new(),
            false => format!(" WHERE {}", clauses.join(" AND ")),
        };

        self.query(
            &format!("{READ}{filter} ORDER BY r.last_seen DESC, r.id DESC, g.ord"),
            rusqlite::params_from_iter(values),
        )
    }

    fn by_key(&self, key: &LookupKey) -> Result<Option<Record>, StoreError> {
        Ok(self
            .query(
                &format!("{READ} WHERE r.lookup_key = ?1 ORDER BY g.ord"),
                [key.as_str()],
            )?
            .into_iter()
            .next())
    }

    fn query(&self, sql: &str, params: impl rusqlite::Params) -> Result<Vec<Record>, StoreError> {
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement
            .query_map(params, Raw::read)?
            .collect::<Result<Vec<_>, _>>()?;
        let mut records = group(rows)?;
        read_tags(&self.conn, &mut records)?;

        Ok(records)
    }
}

/// Whether the history holds this Record.
///
/// A tag or a deletion asks before it writes, so that a missing Record is
/// reported as a missing Record rather than as a foreign key that could not be
/// satisfied.
fn exists(conn: &Connection, id: i64) -> Result<bool, StoreError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM records WHERE id = ?1)",
        [id],
        |row| row.get(0),
    )?)
}

/// The Tag as it will be stored or searched for: trimmed, and refused when
/// nothing is left.
fn trim_tag(tag: &str) -> Result<&str, StoreError> {
    let trimmed = tag.trim();
    match trimmed.is_empty() {
        true => Err(StoreError::EmptyTag {
            tag: tag.to_string(),
        }),
        false => Ok(trimmed),
    }
}

/// Fill in every Record's tags, in tag order.
///
/// A statement of its own rather than a third table in the join: two one-to-many
/// tables joined at once multiply their rows, and a Record with three Glosses and
/// two tags would come back as six.
fn read_tags(conn: &Connection, records: &mut [Record]) -> Result<(), StoreError> {
    let mut statement = conn.prepare("SELECT tag FROM tags WHERE record_id = ?1 ORDER BY tag")?;

    for record in records {
        record.tags = statement
            .query_map([record.id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
    }

    Ok(())
}

/// Read one Record by id, Glosses and tags and all.
fn read_by_id(conn: &Connection, id: i64) -> Result<Option<Record>, StoreError> {
    let mut statement = conn.prepare(&format!("{READ} WHERE r.id = ?1 ORDER BY g.ord"))?;
    let rows = statement
        .query_map([id], Raw::read)?
        .collect::<Result<Vec<_>, _>>()?;
    let mut records = group(rows)?;
    read_tags(conn, &mut records)?;

    Ok(records.into_iter().next())
}

/// Gather the joined rows back into Records: consecutive rows with one id are one
/// Record, and each of them contributes a Gloss when it has one.
///
/// The queries put `g.ord` last in their `ORDER BY`, so a Record's rows are
/// adjacent and its Glosses stay in the model's order.
fn group(rows: Vec<Raw>) -> Result<Vec<Record>, StoreError> {
    let mut records: Vec<Record> = Vec::new();

    for row in rows {
        let gloss = row.gloss();
        if records.last().map(|record| record.id) != Some(row.id) {
            records.push(row.decode()?);
        }
        if let Some(gloss) = gloss {
            records
                .last_mut()
                .expect("a row without a Record was just pushed")
                .artifact
                .explanation
                .glosses
                .push(gloss);
        }
    }

    Ok(records)
}

/// One row as SQLite hands it over, before the enum and timestamp columns are
/// read back into their types.
struct Raw {
    id: i64,
    lookup_key: String,
    passage: String,
    level: String,
    source_language: String,
    native_language: String,
    provider: String,
    model: String,
    thinking: String,
    artifact_version: u32,
    prompt_version: String,
    prompt_label: String,
    comprehensible: String,
    grammar: Option<String>,
    translation: String,
    created_at: String,
    generated_at: String,
    last_seen: String,
    /// The joined Gloss, when this row has one.
    gloss_expression: Option<String>,
    gloss_gloss: Option<String>,
}

impl Raw {
    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            lookup_key: row.get("lookup_key")?,
            passage: row.get("passage")?,
            level: row.get("level")?,
            source_language: row.get("source_language")?,
            native_language: row.get("native_language")?,
            provider: row.get("provider")?,
            model: row.get("model")?,
            thinking: row.get("thinking")?,
            artifact_version: row.get("artifact_version")?,
            prompt_version: row.get("prompt_version")?,
            prompt_label: row.get("prompt_label")?,
            comprehensible: row.get("comprehensible")?,
            grammar: row.get("grammar")?,
            translation: row.get("translation")?,
            created_at: row.get("created_at")?,
            generated_at: row.get("generated_at")?,
            last_seen: row.get("last_seen")?,
            gloss_expression: row.get("gloss_expression")?,
            gloss_gloss: row.get("gloss_gloss")?,
        })
    }

    /// The Gloss this row carries, if it carries one: the join's NULL columns mean
    /// the Record has no Glosses at all.
    fn gloss(&self) -> Option<Gloss> {
        Some(Gloss {
            expression: self.gloss_expression.clone()?,
            gloss: self.gloss_gloss.clone()?,
        })
    }

    /// Turn one stored row back into a Record, with no Glosses yet: the caller
    /// adds those as it walks the joined rows.
    ///
    /// Every column here was written from a typed value, so a column that does
    /// not read back is a file somebody else has been editing. Reported rather
    /// than defaulted: a Level of `B2` invented for a row that says `Z9` would
    /// silently change what the stored Explanation claims to be. The record's id
    /// is in the error because it is the only handle a person has on the row.
    fn decode(self) -> Result<Record, StoreError> {
        let corrupt = |value: String, expected: &'static str| StoreError::Corrupt {
            record: self.id,
            value,
            expected,
        };

        let level = self
            .level
            .parse::<Level>()
            .map_err(|_| corrupt(self.level.clone(), "a Level: A1, A2, B2 or C1"))?;
        let thinking = self
            .thinking
            .parse::<Thinking>()
            .map_err(|_| corrupt(self.thinking.clone(), "a thinking setting: on or off"))?;

        let artifact = Artifact {
            passage: self.passage,
            level,
            source_language: self.source_language,
            native_language: self.native_language,
            provider: self.provider,
            model: self.model,
            thinking,
            artifact_version: self.artifact_version,
            prompt_version: self.prompt_version,
            prompt_label: self.prompt_label,
            created_at: timestamp(self.id, "created_at", &self.created_at)?,
            generated_at: timestamp(self.id, "generated_at", &self.generated_at)?,
            explanation: Explanation {
                comprehensible: self.comprehensible,
                glosses: Vec::new(),
                grammar: self.grammar,
                translation: self.translation,
            },
        };

        Ok(Record {
            id: self.id,
            lookup_key: LookupKey(self.lookup_key),
            last_seen: timestamp(self.id, "last_seen", &self.last_seen)?,
            tags: Vec::new(),
            artifact,
        })
    }
}

/// A stored timestamp, read back strictly.
fn timestamp(record: i64, column: &'static str, value: &str) -> Result<Timestamp, StoreError> {
    Timestamp::from_rfc3339(value).ok_or_else(|| StoreError::Corrupt {
        record,
        value: format!("{column} = {value:?}"),
        expected: "an RFC 3339 UTC timestamp",
    })
}

/// Normalize a Passage for identity: trim its ends and read CRLF as LF.
///
/// Interior whitespace is meaning — two spaces after a full stop are not the same
/// question as one — so it is left alone. The normalized form is never stored;
/// the Passage itself is.
pub fn normalize(passage: &str) -> String {
    passage.replace("\r\n", "\n").trim().to_string()
}

/// What a person typed, as one FTS5 `MATCH` expression per term — and nothing at
/// all when there is nothing to look for.
///
/// Every term becomes a quoted phrase and every phrase is a prefix. Quoting is
/// what keeps FTS5's query language out of a search box: `-`, `*`, `NEAR(` and a
/// stray `"` are then literal text rather than an expression the person never
/// meant to write, and a query of nothing but punctuation quietly matches
/// nothing instead of failing. A double quote inside a term is escaped by
/// doubling it, which is how FTS5 ends a string.
///
/// The terms come back separately rather than joined into one expression, because
/// the caller has to ask each of them of both search tables and AND the answers;
/// see [`Store::search`].
fn match_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect()
}

/// Rebuild the search tables from the history when they are empty.
///
/// The guard is what makes this the upgrade path from a store written before the
/// search tables existed rather than work on every open: in steady state they
/// hold one row per Passage and one per Gloss, and the only way to be empty while
/// the history is not is to have been written by an earlier build. The two halves
/// are decided separately because a history in which nothing was ever glossed is
/// ordinary. Each guard asks whether *any* row is there rather than counting
/// them, so opening a long history stays cheap.
fn reindex(conn: &Connection) -> rusqlite::Result<()> {
    if !indexed(conn, "record_fts")? {
        conn.execute_batch(
            "INSERT INTO record_fts (rowid, passage, comprehensible)
             SELECT id, passage, comprehensible FROM records",
        )?;
    }

    if !indexed(conn, "gloss_fts")? {
        conn.execute_batch(
            "INSERT INTO gloss_fts (expression, gloss, record_id)
             SELECT expression, gloss, record_id FROM glosses",
        )?;
    }

    Ok(())
}

/// Whether a search table holds anything at all.
///
/// The table name is interpolated rather than bound because SQLite has no
/// placeholder for an identifier; it is one of two constants in this file, never
/// anything a caller supplies.
fn indexed(conn: &Connection, table: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        &format!("SELECT EXISTS (SELECT 1 FROM {table})"),
        [],
        |row| row.get(0),
    )
}
