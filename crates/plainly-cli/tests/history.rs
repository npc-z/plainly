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
    // was deleted can still read what it stored.
    let dir = TempDir::new("history-empty");

    let listing = dir.plainly_with(&["history"], "", &[]);

    assert_eq!(code(&listing), SUCCESS, "{}", stderr(&listing));
    assert_eq!(stdout(&listing), "", "nothing stored, nothing printed");
}

#[test]
fn showing_a_record_that_is_not_there_is_a_usage_error() {
    let dir = TempDir::new("history-missing");

    let shown = dir.plainly_with(&["history", "show", "7"], "", &[]);

    assert_eq!(code(&shown), USAGE);
    assert_eq!(stdout(&shown), "");
    assert!(stderr(&shown).contains('7'), "{}", stderr(&shown));
}
