//! The history store at the CLI seam: the second identical run costs nothing, a
//! regeneration lands on the same record, and the listing carries the provenance
//! the spec asks for.
//!
//! The real binary, temporary XDG directories, and the same fake provider the
//! explain tests use — so "no request was sent" is an assertion about a server
//! rather than about a log line.

mod support;

use support::TempDir;
use support::provider::{FakeProvider, Reply};
use support::{code, stderr, stdout};

const SUCCESS: i32 = 0;
const USAGE: i32 = 2;

const PASSAGE: &str = "The committee conducted a thorough investigation into the matter, \
                       but the manager had already gone to ground.";

const ANSWER: &str = r#"{
  "comprehensible": "The committee tried hard to find out what had happened, but the manager had already gone into hiding.",
  "glosses": [
    { "expression": "go to ground", "gloss": "hide so that nobody can find you" }
  ],
  "grammar": null,
  "translation": "委员会对此事进行了彻底调查，但那位经理已经躲了起来。"
}"#;

/// A second answer, so a regeneration can be told apart from a reuse.
const REGENERATED: &str = r#"{
  "comprehensible": "A second attempt at the same sentence.",
  "glosses": [],
  "grammar": "The `but` clause carries the contrast.",
  "translation": "对同一句话的第二次尝试。"
}"#;

fn custom_provider(endpoint: &str) -> String {
    format!(
        "[app]\n\
         provider = \"stub\"\n\n\
         [providers.stub]\n\
         endpoint = \"{endpoint}\"\n\
         model = \"stub-model\"\n"
    )
}

/// The id the listing prints for its one row.
fn only_id(listing: &str) -> i64 {
    listing
        .split_whitespace()
        .next()
        .expect("the listing has a row")
        .parse()
        .expect("the first column is the id")
}

#[test]
fn a_second_identical_run_is_answered_from_the_store_and_sends_nothing() {
    let dir = TempDir::new("history-reuse");
    // One reply for two runs: a second request would be answered with a 500, so
    // "the run succeeded" is itself the proof that nothing was sent.
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let first = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&first), SUCCESS, "{}", stderr(&first));

    let second = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&second), SUCCESS, "{}", stderr(&second));
    assert_eq!(
        stdout(&second),
        stdout(&first),
        "the same question gets the same product"
    );
    let message = stderr(&second);
    assert!(
        message.contains("reusing"),
        "a hit says where the answer came from: {message}"
    );
    for provenance in ["stub/stub-model", "thinking off", "B2", "v7-descriptors@"] {
        assert!(
            message.contains(provenance),
            "a hit carries the provenance of what it reuses ({provenance}): {message}"
        );
    }
    assert_eq!(
        server.chat_requests().len(),
        1,
        "the store is the cache: {:?}",
        server.requests()
    );
}

#[test]
fn regenerate_asks_again_and_overwrites_the_same_record() {
    let dir = TempDir::new("history-regenerate");
    let server = FakeProvider::start([Reply::content(ANSWER), Reply::content(REGENERATED)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let first = dir.plainly_with(
        &["--format", "json"],
        PASSAGE,
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );
    assert_eq!(code(&first), SUCCESS, "{}", stderr(&first));
    let before: serde_json::Value =
        serde_json::from_str(&stdout(&first)).expect("the product is one JSON document");

    let listing = dir.plainly_with(&["history"], "", &[]);
    assert_eq!(code(&listing), SUCCESS, "{}", stderr(&listing));
    let id = only_id(&stdout(&listing));

    // Past the next second, so the two runs cannot share a `created_at` by
    // accident — the point of the next assertion is that they do not.
    std::thread::sleep(std::time::Duration::from_millis(1_100));

    let again = dir.plainly_with(
        &["--regenerate", "--format", "json"],
        PASSAGE,
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );
    assert_eq!(code(&again), SUCCESS, "{}", stderr(&again));
    let after: serde_json::Value =
        serde_json::from_str(&stdout(&again)).expect("the product is one JSON document");

    assert!(
        after["comprehensible"]
            .as_str()
            .expect("a string")
            .contains("A second attempt at the same sentence."),
        "the new Explanation is the product: {after}"
    );
    assert_eq!(
        after["created_at"], before["created_at"],
        "the record was made when it was made: a regeneration does not change that"
    );
    assert_ne!(
        after["generated_at"], before["generated_at"],
        "but the Explanation is new"
    );
    assert_eq!(
        server.chat_requests().len(),
        2,
        "--regenerate sends the question again"
    );

    let later = dir.plainly_with(&["history"], "", &[]);
    assert_eq!(
        only_id(&stdout(&later)),
        id,
        "a regeneration is the same record, not a new one"
    );
    assert_eq!(
        stdout(&later).lines().count(),
        1,
        "and the history did not grow: {}",
        stdout(&later)
    );

    let shown = dir.plainly_with(&["history", "show", &id.to_string()], "", &[]);
    assert_eq!(code(&shown), SUCCESS, "{}", stderr(&shown));
    assert!(stdout(&shown).contains("A second attempt at the same sentence."));
    assert!(
        stderr(&shown).contains(&after["created_at"].as_str().expect("a string").to_string()),
        "what `show` says about the record agrees with what the run printed: {}",
        stderr(&shown)
    );
}

#[test]
fn a_new_level_is_a_new_record() {
    // Changing a setting does not rewrite a stored Explanation: the same Passage
    // at another Level is another question (spec §9).
    let dir = TempDir::new("history-level");
    let server = FakeProvider::start([Reply::content(ANSWER), Reply::content(ANSWER)]);
    let config = |level: &str| {
        format!(
            "[app]\nprovider = \"stub\"\nlevel = \"{level}\"\n\n\
             [providers.stub]\nendpoint = \"{}\"\nmodel = \"stub-model\"\n",
            server.base_url()
        )
    };

    dir.write_config(&config("B2"));
    let at_b2 = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&at_b2), SUCCESS, "{}", stderr(&at_b2));

    dir.write_config(&config("A2"));
    let at_a2 = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&at_a2), SUCCESS, "{}", stderr(&at_a2));

    assert_eq!(server.chat_requests().len(), 2, "a new Level is a new query");

    let listing = dir.plainly_with(&["history"], "", &[]);
    assert_eq!(
        stdout(&listing).lines().count(),
        2,
        "two records, one per Level: {}",
        stdout(&listing)
    );
    assert!(stdout(&listing).contains("B2") && stdout(&listing).contains("A2"));
}

#[test]
fn the_listing_carries_the_provenance_of_every_record() {
    let dir = TempDir::new("history-provenance");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let run = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&run), SUCCESS, "{}", stderr(&run));

    let listing = dir.plainly_with(&["history", "list"], "", &[]);
    let row = stdout(&listing);

    for expected in [
        "stub/stub-model",
        "thinking off",
        "B2",
        "v7-descriptors@",
        "The committee conducted a thorough investigation",
    ] {
        assert!(row.contains(expected), "missing {expected} in: {row}");
    }
    assert_eq!(row.lines().count(), 1, "{row}");
}

#[test]
fn show_prints_the_five_sections_and_the_provenance() {
    let dir = TempDir::new("history-show");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));
    let run = dir.plainly_with(
        &["--format", "json"],
        PASSAGE,
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );
    let artifact: serde_json::Value =
        serde_json::from_str(&stdout(&run)).expect("the product is one JSON document");
    let prompt_version = artifact["prompt_version"]
        .as_str()
        .expect("the prompt version is a string")
        .to_string();

    let listing = dir.plainly_with(&["history"], "", &[]);
    let id = only_id(&stdout(&listing));
    let shown = dir.plainly_with(&["history", "show", &id.to_string()], "", &[]);

    assert_eq!(code(&shown), SUCCESS, "{}", stderr(&shown));
    let product = stdout(&shown);
    for heading in [
        "### Original",
        "### Comprehensible English",
        "### Key Help",
        "### Translation",
    ] {
        assert!(product.contains(heading), "missing {heading} in:\n{product}");
    }
    // Provenance is human information, so it stays on stderr even here: stdout
    // is the product, exactly as it is for `explain`. The show line carries the
    // *whole* prompt hash — the listing abbreviates it, this is where a person
    // reads it.
    let message = stderr(&shown);
    assert!(message.contains("stub/stub-model"), "{message}");
    assert!(message.contains(&prompt_version), "{message}");
    assert_eq!(prompt_version.len(), 64);
    assert!(!product.contains("stub-model"), "{product}");
}

#[test]
fn history_works_without_a_provider_or_a_configuration_file() {
    // The history is local and needs nothing configured: a machine whose config
    // was deleted can still read what it stored, search it, and clear it.
    let dir = TempDir::new("history-empty");

    let listing = dir.plainly_with(&["history"], "", &[]);

    assert_eq!(code(&listing), SUCCESS, "{}", stderr(&listing));
    assert_eq!(stdout(&listing), "", "nothing stored, nothing printed");

    let search = dir.plainly_with(&["history", "search", "ground"], "", &[]);
    assert_eq!(code(&search), SUCCESS, "{}", stderr(&search));
    assert_eq!(stdout(&search), "");

    let cleared = dir.plainly_with(&["history", "clear", "--yes"], "", &[]);
    assert_eq!(code(&cleared), SUCCESS, "{}", stderr(&cleared));
    assert!(
        stderr(&cleared).contains('0'),
        "clearing an empty history says so: {}",
        stderr(&cleared)
    );
}

#[test]
fn showing_a_record_that_is_not_there_is_a_usage_error() {
    let dir = TempDir::new("history-missing");

    let shown = dir.plainly_with(&["history", "show", "7"], "", &[]);

    assert_eq!(code(&shown), USAGE);
    assert_eq!(stdout(&shown), "");
    assert!(stderr(&shown).contains('7'), "{}", stderr(&shown));
}

// ---------------------------------------------------------------------------
// Search, tags, deletion and clearing
// ---------------------------------------------------------------------------

/// A second Passage, so a search or a tag filter has something to leave out.
const SECOND: &str = "She was told to keep her cards close to her chest.";

const SECOND_ANSWER: &str = r#"{
  "comprehensible": "She was told not to say what she was planning.",
  "glosses": [
    { "expression": "keep your cards close to your chest", "gloss": "not tell anyone your plans" }
  ],
  "grammar": null,
  "translation": "有人让她不要把计划说出去。"
}"#;

/// A directory holding one explained Passage, and the id it was stored under.
fn one_explained(name: &str) -> (TempDir, i64) {
    let dir = TempDir::new(name);
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let run = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&run), SUCCESS, "{}", stderr(&run));

    let listing = dir.plainly_with(&["history"], "", &[]);
    let id = only_id(&stdout(&listing));
    (dir, id)
}

#[test]
fn search_finds_a_stored_explanation_through_any_of_its_english() {
    let (dir, id) = one_explained("history-search");

    // One word from each of the three indexed fields: the Passage, the
    // Comprehensible English, and a Gloss.
    for query in ["committee", "hiding", "nobody"] {
        let hit = dir.plainly_with(&["history", "search", query], "", &[]);
        assert_eq!(code(&hit), SUCCESS, "{}", stderr(&hit));
        assert_eq!(
            only_id(&stdout(&hit)),
            id,
            "{query:?} should find the record: {}",
            stdout(&hit)
        );
    }

    let miss = dir.plainly_with(&["history", "search", "bicycles"], "", &[]);
    assert_eq!(code(&miss), SUCCESS, "{}", stderr(&miss));
    assert_eq!(stdout(&miss), "");
}

#[test]
fn search_says_that_the_translation_is_not_searchable() {
    let (dir, _) = one_explained("history-search-fact");

    // A word that is in the Translation and nowhere else: the miss is the stated
    // fact, not a defect, and the search location says which it is.
    let search = dir.plainly_with(&["history", "search", "委员会"], "", &[]);

    assert_eq!(code(&search), SUCCESS, "{}", stderr(&search));
    assert_eq!(stdout(&search), "");
    let message = stderr(&search);
    assert!(
        message.contains("not searchable in v0"),
        "the search location states the limit: {message}"
    );
    assert!(message.contains("Translation"), "{message}");
}

#[test]
fn a_search_for_nothing_is_a_usage_error() {
    let (dir, _) = one_explained("history-search-empty");

    for nothing in ["", "   "] {
        let search = dir.plainly_with(&["history", "search", nothing], "", &[]);
        assert_eq!(code(&search), USAGE);
        assert_eq!(stdout(&search), "");
        assert!(
            stderr(&search).contains("look for"),
            "{}",
            stderr(&search)
        );
    }
}

#[test]
fn a_tag_is_added_shown_filtered_and_taken_off() {
    let dir = TempDir::new("history-tags");
    let server = FakeProvider::start([Reply::content(ANSWER), Reply::content(SECOND_ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let first = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&first), SUCCESS, "{}", stderr(&first));
    let second = dir.plainly_with(&[], SECOND, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&second), SUCCESS, "{}", stderr(&second));

    let listing = dir.plainly_with(&["history"], "", &[]);
    assert_eq!(stdout(&listing).lines().count(), 2);
    let newest = only_id(&stdout(&listing));

    let tagged = dir.plainly_with(&["history", "tag", &newest.to_string(), "reading"], "", &[]);
    assert_eq!(code(&tagged), SUCCESS, "{}", stderr(&tagged));
    assert_eq!(stdout(&tagged), "", "tagging is not a product");

    let filtered = dir.plainly_with(&["history", "list", "--tag", "reading"], "", &[]);
    assert_eq!(code(&filtered), SUCCESS, "{}", stderr(&filtered));
    assert_eq!(
        stdout(&filtered).lines().count(),
        1,
        "only the tagged record: {}",
        stdout(&filtered)
    );
    assert_eq!(only_id(&stdout(&filtered)), newest);
    assert!(
        stdout(&filtered).contains("[reading]"),
        "a row shows the tag it was filtered by: {}",
        stdout(&filtered)
    );

    let shown = dir.plainly_with(&["history", "show", &newest.to_string()], "", &[]);
    assert!(
        stderr(&shown).contains("tagged reading"),
        "the detail says it too: {}",
        stderr(&shown)
    );

    // The same tag narrows a search rather than replacing it.
    let searched = dir.plainly_with(&["history", "search", "cards", "--tag", "reading"], "", &[]);
    assert_eq!(only_id(&stdout(&searched)), newest);
    let elsewhere = dir.plainly_with(&["history", "search", "committee", "--tag", "reading"], "", &[]);
    assert_eq!(stdout(&elsewhere), "");

    let untagged = dir.plainly_with(&["history", "untag", &newest.to_string(), "reading"], "", &[]);
    assert_eq!(code(&untagged), SUCCESS, "{}", stderr(&untagged));
    assert_eq!(stdout(&untagged), "", "and neither is taking one off");
    assert_eq!(
        stdout(&dir.plainly_with(&["history", "list", "--tag", "reading"], "", &[])),
        ""
    );
    assert_eq!(
        stdout(&dir.plainly_with(&["history"], "", &[])).lines().count(),
        2,
        "taking a tag off does not take the record with it"
    );
}

#[test]
fn tagging_a_record_that_is_not_there_is_a_usage_error() {
    let dir = TempDir::new("history-tags-missing");

    for command in [["history", "tag", "7", "work"], ["history", "untag", "7", "work"]] {
        let run = dir.plainly_with(&command, "", &[]);
        assert_eq!(code(&run), USAGE, "{}", stderr(&run));
        assert!(stderr(&run).contains('7'), "{}", stderr(&run));
    }
}

#[test]
fn a_blank_tag_is_a_usage_error() {
    let (dir, id) = one_explained("history-tags-blank");

    let run = dir.plainly_with(&["history", "tag", &id.to_string(), "   "], "", &[]);

    assert_eq!(code(&run), USAGE);
    assert!(stderr(&run).contains("cannot be blank"), "{}", stderr(&run));
    assert!(
        !stdout(&dir.plainly_with(&["history"], "", &[])).contains('['),
        "nothing was tagged"
    );
}

#[test]
fn deleting_a_record_forgets_it() {
    let (dir, id) = one_explained("history-delete");

    let deleted = dir.plainly_with(&["history", "delete", &id.to_string()], "", &[]);
    assert_eq!(code(&deleted), SUCCESS, "{}", stderr(&deleted));
    assert_eq!(stdout(&deleted), "", "a deletion is not a product");
    assert!(stderr(&deleted).contains(&id.to_string()));

    assert_eq!(stdout(&dir.plainly_with(&["history"], "", &[])), "");
    assert_eq!(
        stdout(&dir.plainly_with(&["history", "search", "committee"], "", &[])),
        ""
    );
    assert_eq!(
        code(&dir.plainly_with(&["history", "show", &id.to_string()], "", &[])),
        USAGE
    );

    let again = dir.plainly_with(&["history", "delete", &id.to_string()], "", &[]);
    assert_eq!(code(&again), USAGE);
}

#[test]
fn clearing_takes_a_second_act() {
    let (dir, id) = one_explained("history-clear");

    let refused = dir.plainly_with(&["history", "clear"], "", &[]);
    assert_eq!(code(&refused), USAGE);
    assert_eq!(stdout(&refused), "");
    assert!(
        stderr(&refused).contains("--yes"),
        "the refusal says how to confirm: {}",
        stderr(&refused)
    );
    assert_eq!(
        stdout(&dir.plainly_with(&["history"], "", &[])).lines().count(),
        1,
        "a refusal clears nothing"
    );

    let cleared = dir.plainly_with(&["history", "clear", "--yes"], "", &[]);
    assert_eq!(code(&cleared), SUCCESS, "{}", stderr(&cleared));
    assert_eq!(stdout(&cleared), "");
    assert!(
        stderr(&cleared).contains('1'),
        "it says how many went: {}",
        stderr(&cleared)
    );
    assert_eq!(stdout(&dir.plainly_with(&["history"], "", &[])), "");
    assert_eq!(
        code(&dir.plainly_with(&["history", "show", &id.to_string()], "", &[])),
        USAGE
    );
}

#[test]
fn a_tag_with_a_space_in_it_is_still_one_tag() {
    // The help says to quote such a Tag, so the lines have to be able to say it
    // back: `["to read"]` is one Tag, where `[to read]` would read as two.
    let (dir, id) = one_explained("history-tags-spaces");

    // Padded on purpose: the confirmation has to say what the history holds, not
    // what was typed at it.
    let tagged = dir.plainly_with(&["history", "tag", &id.to_string(), "  to read  "], "", &[]);
    assert_eq!(code(&tagged), SUCCESS, "{}", stderr(&tagged));
    assert!(
        stderr(&tagged).contains("\"to read\""),
        "the confirmation echoes the stored Tag: {}",
        stderr(&tagged)
    );

    let listing = dir.plainly_with(&["history"], "", &[]);
    assert!(
        stdout(&listing).contains("[\"to read\"]"),
        "{:?}",
        stdout(&listing)
    );

    let filtered = dir.plainly_with(&["history", "list", "--tag", "to read"], "", &[]);
    assert_eq!(only_id(&stdout(&filtered)), id);
    assert_eq!(
        stdout(&dir.plainly_with(&["history", "list", "--tag", "to"], "", &[])),
        "",
        "the spaces inside a Tag are not separators"
    );
}

// ---------------------------------------------------------------------------
// The three exports
// ---------------------------------------------------------------------------

#[test]
fn export_writes_the_product_to_stdout_and_nothing_else() {
    let (dir, _) = one_explained("history-export");

    let markdown = dir.plainly_with(&["history", "export", "markdown"], "", &[]);
    assert_eq!(code(&markdown), SUCCESS, "{}", stderr(&markdown));
    let product = stdout(&markdown);
    assert!(
        product.contains(" · B2 · stub/stub-model · v7-descriptors"),
        "the section is headed with what tells two Records apart:\n{product}"
    );
    for heading in [
        "### Original",
        "### Comprehensible English",
        "### Key Help",
        "### Translation",
    ] {
        assert!(product.contains(heading), "missing {heading} in:\n{product}");
    }
    assert_eq!(stderr(&markdown), "", "an export says nothing else");

    let anki = dir.plainly_with(&["history", "export", "anki"], "", &[]);
    assert_eq!(code(&anki), SUCCESS, "{}", stderr(&anki));
    let product = stdout(&anki);
    let cards: Vec<&str> = product.lines().collect();
    assert_eq!(
        cards.len(),
        2,
        "the header and the one card this Gloss makes:\n{product}"
    );
    assert_eq!(
        cards[0],
        "expression,gloss,passage,level,native_language,provider,model,prompt_label"
    );
    assert!(
        cards[1].starts_with("go to ground,hide so that nobody can find you,\""),
        "front and back are the Gloss, and the Passage follows as context: {}",
        cards[1]
    );
    assert_eq!(stderr(&anki), "");

    let raw = dir.plainly_with(&["history", "export", "raw"], "", &[]);
    assert_eq!(code(&raw), SUCCESS, "{}", stderr(&raw));
    let table = stdout(&raw);
    assert!(
        table.starts_with("id,created_at,generated_at,last_seen,level,"),
        "{table}"
    );
    assert!(
        table.contains("go to ground → hide so that nobody can find you"),
        "the Glosses are in one cell:\n{table}"
    );
    assert!(table.contains("委员会"), "and so is the Translation:\n{table}");
    assert_eq!(stderr(&raw), "");
}

#[test]
fn export_of_an_empty_history_is_a_product_with_no_records() {
    // Nothing is configured and nothing has been explained: an empty history is a
    // legal thing to export, not an error.
    let dir = TempDir::new("history-export-empty");

    let markdown = dir.plainly_with(&["history", "export", "markdown"], "", &[]);
    assert_eq!(code(&markdown), SUCCESS, "{}", stderr(&markdown));
    assert_eq!(stdout(&markdown), "");

    for (format, header) in [
        ("anki", "expression,gloss,passage"),
        ("raw", "id,created_at,generated_at"),
    ] {
        let export = dir.plainly_with(&["history", "export", format], "", &[]);
        assert_eq!(code(&export), SUCCESS, "{}", stderr(&export));
        assert!(
            stdout(&export).lines().count() == 1 && stdout(&export).starts_with(header),
            "the columns alone, so an import still knows what it is reading: {:?}",
            stdout(&export)
        );
    }
}

#[test]
fn an_export_without_a_known_format_is_a_usage_error() {
    let dir = TempDir::new("history-export-usage");

    for args in [
        vec!["history", "export"],
        vec!["history", "export", "pdf"],
    ] {
        let run = dir.plainly_with(&args, "", &[]);
        assert_eq!(code(&run), USAGE, "{}", stderr(&run));
        assert_eq!(stdout(&run), "");
    }
}

#[test]
fn the_export_and_the_single_record_view_share_one_rendering() {
    // The detail view and the export must not drift: the same Record, read one
    // way, is inside the same Record read the other way.
    let (dir, id) = one_explained("history-export-shared");

    let shown = dir.plainly_with(&["history", "show", &id.to_string()], "", &[]);
    assert_eq!(code(&shown), SUCCESS, "{}", stderr(&shown));
    let exported = dir.plainly_with(&["history", "export", "markdown"], "", &[]);
    assert_eq!(code(&exported), SUCCESS, "{}", stderr(&exported));

    assert!(
        stdout(&exported).contains(&stdout(&shown)),
        "the five sections the detail view prints are the ones the export carries:\n\
         --- show ---\n{}\n--- export ---\n{}",
        stdout(&shown),
        stdout(&exported)
    );
}
