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
/// name cannot be passed where a key belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        /// person, and until `history delete` exists (tickets/09) there is no
        /// other handle on it.
        record: i64,
        value: String,
        expected: &'static str,
    },
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
        // ones behind.
        tx.execute("DELETE FROM glosses WHERE record_id = ?1", [id])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO glosses (record_id, ord, expression, gloss) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (ord, gloss) in artifact.explanation.glosses.iter().enumerate() {
                insert.execute(params![id, ord as i64, &gloss.expression, &gloss.gloss])?;
            }
        }

        let record = read_by_id(&tx, id)?.expect("the row was just written");
        tx.commit()?;

        Ok(record)
    }

    /// Every stored Explanation, most recently seen first.
    pub fn list(&self) -> Result<Vec<Record>, StoreError> {
        // `id` breaks ties, because `last_seen` is only accurate to the second
        // and two questions asked in the same second still have an order.
        self.query(
            &format!("{READ} ORDER BY r.last_seen DESC, r.id DESC, g.ord"),
            params![],
        )
    }

    /// One stored Explanation, if the id is in the store.
    pub fn show(&self, id: i64) -> Result<Option<Record>, StoreError> {
        read_by_id(&self.conn, id)
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

        group(rows)
    }
}

/// Read one Record by id, Glosses and all.
fn read_by_id(conn: &Connection, id: i64) -> Result<Option<Record>, StoreError> {
    let mut statement = conn.prepare(&format!("{READ} WHERE r.id = ?1 ORDER BY g.ord"))?;
    let rows = statement
        .query_map([id], Raw::read)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(group(rows)?.into_iter().next())
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
