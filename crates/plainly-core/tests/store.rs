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

/// A fixture whose four English fields share no word, so a search result says
/// *which* field was matched rather than which field happens to contain "the".
///
/// One exception is deliberate: `ledgers` is in both the Passage and a Gloss
/// expression, which is what makes the two search tables overlap.
const LEDGERS: &str = "The auditors examined the ledgers twice.";
const LEDGERS_ANSWER: &str = r#"{
  "comprehensible": "The accountants checked the books again.",
  "glosses": [
    { "expression": "pored over", "gloss": "studied something very carefully" },
    { "expression": "the ledgers", "gloss": "the company's account books" }
  ],
  "grammar": null,
  "translation": "审计人员把账目检查了两遍。"
}"#;

/// A fresh in-memory store, which is what every test but the WAL one uses.
fn store() -> Store {
    Store::in_memory().expect("an in-memory store opens")
}

fn ledgers() -> Explanation {
    Explanation::parse(LEDGERS_ANSWER).expect("the fixture holds the contract")
}

/// Store one Passage and return the Record it made.
fn keep(store: &mut Store, passage: &str, explanation: Explanation) -> plainly_core::Record {
    store
        .remember(&lookup(passage).key(), &support::artifact(passage, explanation))
        .expect("the store accepts it")
}

/// The ids a search returns, in the order the search returned them.
fn found(store: &Store, query: &str) -> Vec<i64> {
    store
        .search(query, None)
        .expect("the search runs")
        .into_iter()
        .map(|record| record.id)
        .collect()
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

// ---------------------------------------------------------------------------
// Search: the English fields, two tables, one list
// ---------------------------------------------------------------------------

#[test]
fn search_finds_a_record_by_its_passage_and_by_its_comprehensible_english() {
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());

    assert_eq!(found(&store, "auditors"), vec![record.id], "the Passage");
    assert_eq!(
        found(&store, "accountants"),
        vec![record.id],
        "the Comprehensible English"
    );
    assert_eq!(
        found(&store, "audit"),
        vec![record.id],
        "a search is matched by prefix: the box is typed into, not submitted"
    );
}

#[test]
fn search_finds_a_record_by_a_gloss_expression_and_by_a_gloss_gloss() {
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());

    assert_eq!(
        found(&store, "pored"),
        vec![record.id],
        "the expression a Gloss explains"
    );
    assert_eq!(
        found(&store, "studied"),
        vec![record.id],
        "the Gloss's own words"
    );
}

#[test]
fn a_record_whose_passage_and_gloss_both_match_is_listed_once() {
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());

    // `ledgers` is in the Passage and in a Gloss expression, so the two search
    // tables both answer — and the history is still one row.
    assert_eq!(found(&store, "ledgers"), vec![record.id]);
}

#[test]
fn all_terms_have_to_match() {
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());

    assert_eq!(
        found(&store, "auditors accountants"),
        vec![record.id],
        "one record can satisfy both terms between its fields"
    );
    assert!(
        found(&store, "auditors bicycles").is_empty(),
        "a term nothing holds rules the record out"
    );
}

#[test]
fn the_terms_of_one_query_may_be_spread_across_the_fields() {
    // The Passage and a Gloss are two tables, but one search: `MATCH` is
    // evaluated per table, so asking both words of either table would lose a
    // query whose words are one in each.
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());

    assert_eq!(
        found(&store, "auditors studied"),
        vec![record.id],
        "a word from the Passage and a word from a Gloss"
    );
    assert_eq!(
        found(&store, "pored ledgers"),
        vec![record.id],
        "a word from a Gloss expression and a word from the Passage"
    );
    assert!(
        found(&store, "auditors studied bicycles").is_empty(),
        "every term still has to be held somewhere"
    );
}

#[test]
fn search_lists_the_most_recently_seen_first() {
    let mut store = store();
    let older = keep(&mut store, LEDGERS, ledgers());
    let newer = store
        .remember(
            &lookup(support::PASSAGE).key(),
            &support::artifact_at(support::PASSAGE, explanation(), 1_760_000_100),
        )
        .expect("the store accepts it");

    assert_eq!(found(&store, "the"), vec![newer.id, older.id]);

    store
        .recall(&lookup(LEDGERS).key(), at(1_760_000_200))
        .expect("the store is readable")
        .expect("the question has been asked");

    assert_eq!(
        found(&store, "the"),
        vec![older.id, newer.id],
        "a hit moves the record to the front of a search too"
    );
}

#[test]
fn the_translation_is_not_searchable() {
    // Spec §9: `translation` is not indexed, and the tokenizer is per-table, so
    // Chinese would need a table of its own. This is the known fact the surfaces
    // state, asserted as a fact rather than left to be discovered.
    let mut store = store();
    keep(&mut store, LEDGERS, ledgers());

    for chinese in ["审计", "账目", "检查"] {
        assert!(
            found(&store, chinese).is_empty(),
            "{chinese:?} is in the Translation and must not be found"
        );
    }
    assert_eq!(
        found(&store, "auditors").len(),
        1,
        "and the same record is found through its English, so the assertion above is about the field"
    );
}

#[test]
fn the_grammar_note_is_not_searchable() {
    let mut store = store();
    let record = keep(
        &mut store,
        support::PASSAGE_WITH_GRAMMAR,
        Explanation::parse(support::ANSWER_WITH_GRAMMAR).expect("the fixture holds the contract"),
    );

    assert!(
        found(&store, "auxiliary").is_empty(),
        "`auxiliary` is in the Grammar note alone"
    );
    assert_eq!(
        found(&store, "auditors"),
        vec![record.id],
        "the Passage of the same record is still searchable"
    );
}

#[test]
fn a_query_of_punctuation_finds_nothing_rather_than_failing() {
    // FTS5 has a query language of its own, and a search box is not the place to
    // teach it: every term is quoted, so punctuation is literal and a query that
    // means nothing to the tokenizer quietly matches nothing.
    let mut store = store();
    keep(&mut store, LEDGERS, ledgers());

    for junk in ["!!!", "\"", "(NEAR", "-", "*"] {
        assert!(
            found(&store, junk).is_empty(),
            "{junk:?} must not be an error and must not match"
        );
    }
}

#[test]
fn an_empty_query_matches_everything() {
    // What a search box holds before anything is typed into it, and after it is
    // cleared. The history is still ordered by when it was last seen.
    let mut store = store();
    let first = keep(&mut store, LEDGERS, ledgers());
    let second = keep(&mut store, support::PASSAGE, explanation());

    assert_eq!(found(&store, ""), vec![second.id, first.id]);
    assert_eq!(found(&store, "   "), vec![second.id, first.id]);
}

// ---------------------------------------------------------------------------
// Tags: the learner's own words, added by hand
// ---------------------------------------------------------------------------

/// The ids a search returns when it is narrowed to one tag.
fn tagged(store: &Store, query: &str, tag: &str) -> Vec<i64> {
    store
        .search(query, Some(tag))
        .expect("the search runs")
        .into_iter()
        .map(|record| record.id)
        .collect()
}

#[test]
fn a_search_narrowed_by_a_tag_uses_the_same_rule_as_storing_one() {
    // The Tag a filter is given goes through the same rule as the Tag that was
    // stored: `--tag "  work  "` finds the record, and `--tag " "` is a
    // mistake in the asking rather than a filter that quietly lists nothing.
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());
    store.tag(record.id, "work").expect("the record exists");

    assert_eq!(tagged(&store, "", "  work  "), vec![record.id]);
    assert!(matches!(
        store.search("", Some("   ")),
        Err(StoreError::EmptyTag { .. })
    ));
}

#[test]
fn a_tag_can_be_added_and_removed_again() {
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());

    store.tag(record.id, "work").expect("the record exists");
    store.tag(record.id, "money").expect("the record exists");

    let shown = store
        .show(record.id)
        .expect("the store is readable")
        .expect("the record is there");
    assert_eq!(
        shown.tags,
        vec!["money".to_string(), "work".to_string()],
        "a tag belongs to the Record and comes back with it"
    );

    store.untag(record.id, "money").expect("the tag was there");

    let shown = store
        .show(record.id)
        .expect("the store is readable")
        .expect("the record is there");
    assert_eq!(shown.tags, vec!["work".to_string()]);
}

#[test]
fn a_tag_is_trimmed_and_a_blank_one_is_refused() {
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());

    let stored = store
        .tag(record.id, "  work  ")
        .expect("the record exists");
    assert_eq!(
        stored, "work",
        "the Tag comes back as it was stored, so a caller cannot report one thing \
         while the history holds another"
    );
    let shown = store
        .show(record.id)
        .expect("the store is readable")
        .expect("the record is there");
    assert_eq!(
        shown.tags,
        vec!["work".to_string()],
        "the ends of what was typed are not part of the Tag"
    );

    for blank in ["", "   ", "\t"] {
        assert!(
            matches!(
                store.tag(record.id, blank),
                Err(StoreError::EmptyTag { .. })
            ),
            "{blank:?} is not a Tag"
        );
    }
    assert_eq!(
        store
            .show(record.id)
            .expect("the store is readable")
            .expect("the record is there")
            .tags,
        vec!["work".to_string()],
        "a refused tag is not stored"
    );
}

#[test]
fn the_same_tag_twice_is_still_one_tag() {
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());

    store.tag(record.id, "work").expect("the record exists");
    store.tag(record.id, "work").expect("the record exists");

    assert_eq!(
        store
            .show(record.id)
            .expect("the store is readable")
            .expect("the record is there")
            .tags,
        vec!["work".to_string()]
    );
}

#[test]
fn removing_a_tag_that_was_never_there_is_not_an_error() {
    // Clicking a chip that is already gone, or removing a tag twice, is not a
    // mistake worth a failure: what the caller asked for is true afterwards.
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());

    store.untag(record.id, "work").expect("nothing to undo");

    assert!(
        store
            .show(record.id)
            .expect("the store is readable")
            .expect("the record is there")
            .tags
            .is_empty()
    );
}

#[test]
fn tagging_a_record_that_is_not_there_is_reported() {
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());

    assert!(matches!(
        store.tag(record.id + 1, "work"),
        Err(StoreError::NoRecord { id }) if id == record.id + 1
    ));
    assert!(matches!(
        store.untag(record.id + 1, "work"),
        Err(StoreError::NoRecord { .. })
    ));
}

#[test]
fn tags_survive_a_regeneration() {
    // A regeneration replaces the Explanation, not the Record: the learner's own
    // Tags are about the Passage, and they are still true of it.
    let mut store = store();
    let key = lookup(LEDGERS).key();
    let record = keep(&mut store, LEDGERS, ledgers());
    store.tag(record.id, "work").expect("the record exists");

    let again = store
        .remember(
            &key,
            &support::artifact_at(LEDGERS, replacement(), 1_760_000_100),
        )
        .expect("the store accepts it");

    assert_eq!(again.id, record.id);
    assert_eq!(again.tags, vec!["work".to_string()]);
}

#[test]
fn a_search_can_be_narrowed_to_one_tag() {
    let mut store = store();
    let work = keep(&mut store, LEDGERS, ledgers());
    let other = keep(&mut store, support::PASSAGE, explanation());
    store.tag(work.id, "work").expect("the record exists");
    store.tag(other.id, "reading").expect("the record exists");

    assert_eq!(tagged(&store, "", "work"), vec![work.id]);
    assert_eq!(tagged(&store, "", "reading"), vec![other.id]);
    assert_eq!(
        tagged(&store, "", "nothing-is-tagged-this"),
        Vec::<i64>::new()
    );
    assert_eq!(
        tagged(&store, "ledgers", "work"),
        vec![work.id],
        "a tag narrows a text search rather than replacing it"
    );
    assert!(
        tagged(&store, "ledgers", "reading").is_empty(),
        "both have to hold"
    );
    assert_eq!(
        tagged(&store, "committee", "reading"),
        vec![other.id]
    );
}

// ---------------------------------------------------------------------------
// Deletion and clearing: a hard delete, and no graveyard behind it
// ---------------------------------------------------------------------------

#[test]
fn deleting_a_record_takes_the_glosses_the_tags_and_the_search_rows_with_it() {
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());
    store.tag(record.id, "work").expect("the record exists");

    store.delete(record.id).expect("it is there");

    assert!(store.list().expect("the history lists").is_empty());
    assert!(store.show(record.id).expect("readable").is_none());
    for gone in ["auditors", "accountants", "pored", "studied", "ledgers"] {
        assert!(
            found(&store, gone).is_empty(),
            "{gone:?} was in the deleted record"
        );
    }
    assert!(
        store
            .search("", Some("work"))
            .expect("the search runs")
            .is_empty(),
        "and the tag of the deleted Record is not left behind as a filter"
    );
}

#[test]
fn a_deleted_record_leaves_nothing_for_the_next_record_to_inherit() {
    // SQLite hands the id of a deleted Record to the next insert, so search rows
    // or tags that outlived their Record would make the next Passage answer to
    // the old one's words. This is the visible form of a graveyard.
    let mut store = store();
    let first = keep(&mut store, LEDGERS, ledgers());
    store.tag(first.id, "work").expect("the record exists");
    store.delete(first.id).expect("it is there");

    let second = keep(&mut store, support::PASSAGE, explanation());
    assert_eq!(second.id, first.id, "the id is reused, which is the point");
    assert!(second.tags.is_empty(), "and the old Tags are not");
    assert!(
        found(&store, "auditors").is_empty(),
        "the deleted Passage is not findable through the new Record"
    );
    assert_eq!(found(&store, "committee"), vec![second.id]);
}

#[test]
fn deleting_a_record_that_is_not_there_is_reported() {
    let mut store = store();
    let record = keep(&mut store, LEDGERS, ledgers());

    assert!(matches!(
        store.delete(record.id + 1),
        Err(StoreError::NoRecord { id }) if id == record.id + 1
    ));
    assert_eq!(store.list().expect("the history lists").len(), 1);
}

#[test]
fn clearing_removes_every_record_and_reports_how_many() {
    let mut store = store();
    keep(&mut store, LEDGERS, ledgers());
    let second = keep(&mut store, support::PASSAGE, explanation());
    store.tag(second.id, "work").expect("the record exists");

    assert_eq!(store.clear().expect("the history clears"), 2);

    assert!(store.list().expect("the history lists").is_empty());
    assert!(store.show(second.id).expect("readable").is_none());
    for gone in ["auditors", "pored", "committee", "ground"] {
        assert!(found(&store, gone).is_empty(), "{gone:?} was cleared");
    }
    assert!(
        store.search("", Some("work")).expect("the search runs").is_empty(),
        "a tag of a cleared Record is gone too"
    );
}

#[test]
fn clearing_an_empty_history_removes_nothing() {
    let mut store = store();

    assert_eq!(store.clear().expect("there is nothing to clear"), 0);
}

#[test]
fn the_history_file_is_not_encrypted() {
    // Spec §9: the store is not encrypted. The trade is deliberate — a forgotten
    // password losing the whole history is worse than somebody who already has
    // read access reading it — and a property of the file is read back from the
    // file, since the interface would look the same either way.
    let dir = TempDir::new("store-not-encrypted");
    let path = dir.join("history.db");
    {
        let mut store = Store::open(&path).expect("the store opens");
        keep(&mut store, LEDGERS, ledgers());
    }

    let conn = rusqlite::Connection::open(&path).expect("the file opens with no key");
    let passage: String = conn
        .query_row("SELECT passage FROM records", [], |row| row.get(0))
        .expect("the Passage is legible as it stands");

    assert_eq!(passage, LEDGERS);
}

#[test]
fn a_history_written_before_the_search_tables_existed_is_searchable() {
    // The search tables are derived data, so the build that predates them is
    // simulated by dropping them: the next open has to notice they are empty and
    // fill them from the history rather than quietly answering nothing.
    let dir = TempDir::new("store-reindex");
    let path = dir.join("history.db");
    {
        let mut store = Store::open(&path).expect("the store opens");
        keep(&mut store, LEDGERS, ledgers());
    }
    {
        let conn = rusqlite::Connection::open(&path).expect("the file is a database");
        conn.execute_batch("DROP TABLE record_fts; DROP TABLE gloss_fts;")
            .expect("the test can drop the search tables");
    }

    let store = Store::open(&path).expect("the store opens again");

    assert_eq!(found(&store, "auditors").len(), 1, "the Passage is indexed");
    assert_eq!(found(&store, "pored").len(), 1, "and so is the Gloss");
}
