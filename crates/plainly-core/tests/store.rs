//! The history store: what a Lookup Key is made of, what a hit does, and what a
//! regeneration leaves alone — driven by a real (in-memory) SQLite store.
//!
//! The store is the history *and* the cache (spec §9), so these tests are about
//! identity and about the row: which question this is, and what happens to the
//! row that already answered it.

mod support;

use plainly_core::store::{Lookup, Store, StoreError, normalize};
use plainly_core::{Explanation, Level, Thinking, Timestamp};
use support::TempDir;

fn at(seconds: i64) -> Timestamp {
    Timestamp::from_unix_seconds(seconds).expect("the fixture instant is in range")
}

/// The question every test asks, with one field changed at a time. The prompt
/// version is the fixture's, so the Lookup describes the same question as the
/// Artifact fixture does.
fn lookup(passage: &str) -> Lookup<'_> {
    Lookup {
        passage,
        level: Level::B2,
        source_language: "en",
        native_language: "Chinese",
        prompt_version: support::PROMPT_VERSION,
        provider: "deepseek",
        model: "deepseek-flash",
        thinking: Thinking::Off,
    }
}

fn explanation() -> Explanation {
    Explanation::parse(support::ANSWER).expect("the fixture holds the contract")
}

/// A replacement Explanation with no Glosses at all, so a test can tell whether
/// the old ones were removed rather than added to.
fn replacement() -> Explanation {
    Explanation::parse(
        r#"{"comprehensible": "Rewritten.", "glosses": [], "grammar": "A clause.", "translation": "重写。"}"#,
    )
    .expect("the fixture holds the contract")
}

/// A fresh in-memory store, which is what every test but the WAL one uses.
fn store() -> Store {
    Store::in_memory().expect("an in-memory store opens")
}

/// The ids in the order the history would list them.
fn ids(store: &Store) -> Vec<i64> {
    store
        .list()
        .expect("the history lists")
        .into_iter()
        .map(|record| record.id)
        .collect()
}

#[test]
fn a_question_nobody_has_asked_is_a_miss() {
    let store = store();

    assert!(
        store
            .recall(&lookup(support::PASSAGE).key(), at(1))
            .expect("the store is readable")
            .is_none()
    );
    assert!(store.list().expect("the history lists").is_empty());
}

#[test]
fn a_stored_explanation_comes_back_whole() {
    let mut store = store();
    let artifact = support::artifact(support::PASSAGE, explanation());
    let key = lookup(support::PASSAGE).key();

    let saved = store.remember(&key, &artifact).expect("the store accepts it");
    let found = store
        .recall(&key, at(1_760_000_100))
        .expect("the store is readable")
        .expect("the same question has been asked");

    assert_eq!(found.id, saved.id);
    assert_eq!(
        found.artifact, artifact,
        "the Passage, the Explanation and every piece of provenance survive"
    );
    assert_eq!(
        found.artifact.explanation.glosses.len(),
        3,
        "the Glosses are back, in the order the model gave them"
    );
}

#[test]
fn differ_only_in_surrounding_whitespace_and_line_endings_is_the_same_question() {
    let mut store = store();
    let messy = "  The committee\r\nconducted it.  ";
    let artifact = support::artifact(messy, explanation());

    store
        .remember(&lookup(messy).key(), &artifact)
        .expect("the store accepts it");

    let tidy = "\nThe committee\nconducted it.\n";
    let found = store
        .recall(&lookup(tidy).key(), at(1))
        .expect("the store is readable")
        .expect("the two spellings are one question");

    assert_eq!(
        found.artifact.passage, messy,
        "the Passage is stored verbatim, not as it was normalized"
    );
    assert_eq!(normalize(tidy), "The committee\nconducted it.");
}

#[test]
fn interior_whitespace_is_a_different_question() {
    // Two spaces after a full stop are meaning; only the ends are not.
    assert_ne!(lookup("a  b").key(), lookup("a b").key());
    assert_ne!(lookup("a\nb").key(), lookup("a b").key());
    assert_eq!(lookup(" a\r\n").key(), lookup("\na ").key());
}

#[test]
fn the_whole_provider_profile_and_the_prompt_version_are_in_the_key() {
    let base = lookup(support::PASSAGE);

    for other in [
        Lookup {
            level: Level::A2,
            ..base
        },
        Lookup {
            native_language: "Japanese",
            ..base
        },
        Lookup {
            source_language: "fr",
            ..base
        },
        Lookup {
            prompt_version: "2222",
            ..base
        },
        Lookup {
            provider: "openai",
            ..base
        },
        Lookup {
            model: "gpt-5",
            ..base
        },
        Lookup {
            thinking: Thinking::On,
            ..base
        },
    ] {
        assert_ne!(
            other.key(),
            base.key(),
            "{other:?} must not be answered by the Explanation for {base:?}"
        );
    }
}

#[test]
fn a_hit_reuses_the_row_and_moves_it_to_the_front() {
    let mut store = store();
    let older = store
        .remember(
            &lookup("One.").key(),
            &support::artifact_at("One.", explanation(), 1_760_000_000),
        )
        .expect("the store accepts it");
    let newer = store
        .remember(
            &lookup("Two.").key(),
            &support::artifact_at("Two.", explanation(), 1_760_000_100),
        )
        .expect("the store accepts it");
    assert_eq!(ids(&store), vec![newer.id, older.id], "most recent first");

    let hit = store
        .recall(&lookup("One.").key(), at(1_760_000_200))
        .expect("the store is readable")
        .expect("the question has been asked");

    assert_eq!(hit.id, older.id, "the same row, not a new one");
    assert_eq!(hit.last_seen, at(1_760_000_200));
    assert_eq!(ids(&store), vec![older.id, newer.id]);
    assert_eq!(store.list().expect("the history lists").len(), 2);
}

#[test]
fn a_regeneration_replaces_the_explanation_and_keeps_the_two_dates() {
    let mut store = store();
    let key = lookup(support::PASSAGE).key();
    let saved = store
        .remember(&key, &support::artifact_at(support::PASSAGE, explanation(), 1_000))
        .expect("the store accepts it");

    // The learner looks at it, which moves `last_seen` forward …
    store
        .recall(&key, at(2_000))
        .expect("the store is readable")
        .expect("the question has been asked");

    // … and then asks for it to be done again, at a third instant.
    let again = store
        .remember(
            &key,
            &support::artifact_at(support::PASSAGE, replacement(), 3_000),
        )
        .expect("the store accepts it");

    assert_eq!(again.id, saved.id, "the same record, overwritten");
    assert_eq!(store.list().expect("the history lists").len(), 1);
    assert_eq!(again.artifact.explanation, replacement());
    assert!(
        again.artifact.explanation.glosses.is_empty(),
        "the Glosses of the replaced Explanation are gone, not merged"
    );
    assert_eq!(again.artifact.created_at, at(1_000), "created stays");
    assert_eq!(again.artifact.generated_at, at(3_000), "generated moves");
    assert_eq!(again.last_seen, at(2_000), "last seen is not touched");
}

#[test]
fn a_question_asked_twice_within_the_same_second_is_still_a_hit() {
    // `last_seen` is only accurate to the second, so a second ask at the same
    // instant writes back the value that is already there. That is a hit, and it
    // must not depend on the write reporting itself as a change.
    let mut store = store();
    let key = lookup(support::PASSAGE).key();
    let saved = store
        .remember(
            &key,
            &support::artifact_at(support::PASSAGE, explanation(), 1_000),
        )
        .expect("the store accepts it");

    let hit = store
        .recall(&key, at(1_000))
        .expect("the store is readable")
        .expect("the same instant is still the same question");

    assert_eq!(hit.id, saved.id);
    assert_eq!(hit.last_seen, at(1_000));
    assert_eq!(store.list().expect("the history lists").len(), 1);
}

#[test]
fn a_new_artifact_version_reuses_the_row() {
    // `artifact_version` is not part of a Lookup Key (spec §9): a change of data
    // shape does not make an old answer wrong, so it must not fragment the cache.
    let mut store = store();
    let key = lookup(support::PASSAGE).key();
    let first = support::artifact(support::PASSAGE, explanation());
    store.remember(&key, &first).expect("the store accepts it");

    let mut newer = support::artifact(support::PASSAGE, explanation());
    newer.artifact_version = first.artifact_version + 1;
    let again = store.remember(&key, &newer).expect("the store accepts it");

    assert_eq!(store.list().expect("the history lists").len(), 1);
    assert_eq!(again.artifact.artifact_version, newer.artifact_version);
}

#[test]
fn a_new_level_is_a_new_record_and_leaves_the_old_one_alone() {
    let mut store = store();
    let b2 = store
        .remember(
            &lookup(support::PASSAGE).key(),
            &support::artifact(support::PASSAGE, explanation()),
        )
        .expect("the store accepts it");

    let mut at_a2 = support::artifact(support::PASSAGE, explanation());
    at_a2.level = Level::A2;
    let a2 = store
        .remember(
            &Lookup {
                level: Level::A2,
                ..lookup(support::PASSAGE)
            }
            .key(),
            &at_a2,
        )
        .expect("the store accepts it");

    assert_ne!(a2.id, b2.id, "a new Level is a new query");
    assert_eq!(store.list().expect("the history lists").len(), 2);
    assert_eq!(
        store
            .show(b2.id)
            .expect("the store is readable")
            .expect("the old record is still there")
            .artifact
            .level,
        Level::B2,
        "the old record still says the Level it was pitched at"
    );
}

#[test]
fn a_record_carries_its_provenance() {
    let mut store = store();
    let saved = store
        .remember(
            &lookup(support::PASSAGE).key(),
            &support::artifact(support::PASSAGE, explanation()),
        )
        .expect("the store accepts it");

    let found = store
        .show(saved.id)
        .expect("the store is readable")
        .expect("the record is there");

    assert_eq!(found.artifact.provider, "deepseek");
    assert_eq!(found.artifact.model, "deepseek-flash");
    assert_eq!(found.artifact.thinking, Thinking::Off);
    assert_eq!(found.artifact.prompt_version, support::PROMPT_VERSION);
    assert_eq!(found.artifact.prompt_label, "v7-descriptors");
    assert_eq!(found.artifact.level, Level::B2);
    assert_eq!(found.artifact.source_language, "en");
    assert!(store.show(saved.id + 1).expect("readable").is_none());
}

#[test]
fn the_file_the_store_leaves_behind_is_a_wal_database() {
    // WAL is what lets the panel read while the main window writes (spec §8), and
    // it is a property of the file rather than of the interface — so it is read
    // back from the file with a connection of the test's own.
    let dir = TempDir::new("store-wal");
    let path = dir.join("data/history.db");
    {
        let _store = Store::open(&path).expect("the store opens and creates its directory");
    }

    let conn = rusqlite::Connection::open(&path).expect("the file is a database");
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("the mode is readable");

    assert_eq!(mode, "wal");
}

#[test]
fn a_file_that_is_not_a_database_says_which_file_it_is() {
    // `Connection::open` succeeds on a text file — SQLite only reads it when the
    // first statement runs — so this is the failure a user is most likely to
    // meet, and the one where the path is the whole message.
    let dir = TempDir::new("store-not-a-database");
    let path = dir.join("history.db");
    std::fs::write(&path, "this is not a database").expect("the fixture is writable");

    let error = Store::open(&path).expect_err("a text file is not a store");

    assert!(
        matches!(&error, StoreError::Unusable { .. }),
        "got {error:?}"
    );
    assert!(
        error.to_string().contains(&path.display().to_string()),
        "the message names the file: {error}"
    );
}

#[test]
fn a_row_somebody_edited_is_reported_rather_than_guessed_at() {
    // The history file is on disk and hand-editable. A Level that does not parse
    // is a corrupt record, not a B2 one: inventing a value would change what the
    // stored Explanation claims to be.
    let dir = TempDir::new("store-corrupt");
    let path = dir.join("history.db");
    let key = {
        let mut store = Store::open(&path).expect("the store opens");
        let key = lookup(support::PASSAGE).key();
        store
            .remember(
                &key,
                &support::artifact(support::PASSAGE, explanation()),
            )
            .expect("the store accepts it");
        key.as_str().to_string()
    };

    let conn = rusqlite::Connection::open(&path).expect("the file is a database");
    conn.execute(
        "UPDATE records SET level = 'Z9' WHERE lookup_key = ?1",
        [&key],
    )
    .expect("the test can corrupt the file");

    let store = Store::open(&path).expect("the store opens");
    let error = store.list().expect_err("a corrupt row is not a record");

    assert!(
        matches!(&error, StoreError::Corrupt { value, .. } if value == "Z9"),
        "got {error:?}"
    );
    assert!(error.to_string().contains("where a Level"), "{error}");
}

#[test]
fn a_row_that_does_not_read_back_is_not_a_reason_to_lose_the_rest() {
    // Two records, one of them edited: the good one is still reachable by id, so
    // the failure is about the bad row rather than about the whole history.
    let dir = TempDir::new("store-partly-corrupt");
    let path = dir.join("history.db");
    let (good, bad) = {
        let mut store = Store::open(&path).expect("the store opens");
        let good = store
            .remember(
                &lookup("Good.").key(),
                &support::artifact("Good.", explanation()),
            )
            .expect("the store accepts it");
        let bad = store
            .remember(
                &lookup("Bad.").key(),
                &support::artifact("Bad.", explanation()),
            )
            .expect("the store accepts it");
        (good, bad)
    };

    let conn = rusqlite::Connection::open(&path).expect("the file is a database");
    conn.execute("UPDATE records SET level = 'Z9' WHERE id = ?1", [bad.id])
        .expect("the test can edit the file");

    let store = Store::open(&path).expect("the store opens");
    assert_eq!(
        store
            .show(good.id)
            .expect("the good record is readable")
            .expect("it is there")
            .id,
        good.id
    );
    assert!(
        store.list().is_err(),
        "the bad row is what cannot be read, and it is reported"
    );
}
