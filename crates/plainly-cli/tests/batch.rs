//! `plainly explain` on a long input: the split, the batch, and what survives a
//! chunk that fails.
//!
//! One explanation is one Passage (spec §4), so a file longer than a Passage is
//! several runs against the same provider. These tests are about what a script
//! observes of that: one product per chunk in the input's order, one Record per
//! chunk, and a failure that costs its own chunk and no more.

mod support;

use serde_json::Value;

use support::TempDir;
use support::provider::{FakeProvider, Reply};
use support::{code, stderr, stdout};

const SUCCESS: i32 = 0;
const FAILURE: i32 = 1;

/// A contract-shaped answer, as a model would return it.
const ANSWER: &str = r#"{
  "comprehensible": "The committee tried hard to find out what had happened, but the manager had already gone into hiding.",
  "glosses": [
    { "expression": "go to ground", "gloss": "hide so that nobody can find you" }
  ],
  "grammar": null,
  "translation": "委员会对此事进行了彻底调查，但那位经理已经躲了起来。"
}"#;

/// A configuration for a provider Plainly ships no preset for: the endpoint and
/// model are the user's, which is the custom-provider path.
fn custom_provider(endpoint: &str) -> String {
    format!(
        "[app]\n\
         provider = \"stub\"\n\n\
         [providers.stub]\n\
         endpoint = \"{endpoint}\"\n\
         model = \"stub-model\"\n"
    )
}

/// A paragraph of exactly `count` distinct words, so a test can say which chunk
/// a request carried and in which order.
fn paragraph(tag: &str, count: usize) -> String {
    (0..count)
        .map(|word| format!("{tag}{word}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The paragraphs as one file body: a blank line between them, which is what
/// makes a boundary a boundary.
fn file_body(paragraphs: &[String]) -> String {
    paragraphs.join("\n\n")
}

fn write(dir: &TempDir, name: &str, body: &str) -> String {
    let file = dir.join(name);
    std::fs::write(&file, body).expect("the fixture file is writable");
    file.to_str().expect("the path is UTF-8").to_string()
}

/// The chunk the `n`th explaining request carried.
fn sent_passage(server: &FakeProvider, n: usize) -> String {
    server.chat_requests()[n].body["messages"][1]["content"]
        .as_str()
        .expect("the Passage is text")
        .to_string()
}

/// A file over the panel's five-chunk limit is the CLI's business: it is not
/// refused, it is six runs, and `--format json` is an array in the file's order.
#[test]
fn a_file_longer_than_the_panel_limit_is_a_batch_in_the_original_order() {
    let dir = TempDir::new("batch-array");
    let server = FakeProvider::start((0..6).map(|_| Reply::content(ANSWER)));
    dir.write_config(&custom_provider(&server.base_url()));

    let paragraphs: Vec<String> = (0..6).map(|n| paragraph(&format!("p{n}w"), 150)).collect();
    let file = write(&dir, "long.md", &file_body(&paragraphs));

    let output = dir.plainly_with(
        &["explain", &file, "--format", "json"],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));

    let document: Value = serde_json::from_str(&stdout(&output)).expect("stdout is one document");
    let artifacts = document
        .as_array()
        .expect("more than one chunk is an array of Artifacts");
    assert_eq!(artifacts.len(), 6);
    for (n, artifact) in artifacts.iter().enumerate() {
        assert_eq!(
            artifact["passage"], paragraphs[n],
            "chunk {n} is out of order"
        );
    }

    assert_eq!(server.chat_requests().len(), 6, "one request per chunk");
    for (n, paragraph) in paragraphs.iter().enumerate() {
        assert_eq!(&sent_passage(&server, n), paragraph);
    }

    let message = stderr(&output);
    assert!(message.contains("6 chunks"), "{message}");
    assert!(message.contains("will make 6 requests"), "{message}");
    for n in 1..=6 {
        assert!(message.contains(&format!("chunk {n}/6")), "{message}");
    }
}

/// A batch in markdown is one document per chunk, in order, with a blank line
/// between them — the whole product on stdout and nothing else.
#[test]
fn a_markdown_batch_is_one_document_per_chunk_separated_by_a_blank_line() {
    let dir = TempDir::new("batch-markdown");
    let server = FakeProvider::start([Reply::content(ANSWER), Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let paragraphs: Vec<String> = (0..2).map(|n| paragraph(&format!("p{n}w"), 150)).collect();
    let file = write(&dir, "long.md", &file_body(&paragraphs));

    let output = dir.plainly_with(
        &["explain", &file],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));

    let product = stdout(&output);
    assert_eq!(product.matches("### Original").count(), 2, "{product}");
    assert!(
        product.contains("\n\n### Original"),
        "the documents are separated by a blank line: {product}"
    );
    let first = product.find(&paragraphs[0]).expect("the first chunk");
    let second = product.find(&paragraphs[1]).expect("the second chunk");
    assert!(
        first < second,
        "the chunks keep the input's order: {product}"
    );
}

/// A chunk that fails costs its own run and no more: the others are still asked,
/// what succeeded is still on stdout, and the exit code says something went
/// wrong (spec §10).
#[test]
fn a_failed_chunk_does_not_discard_the_rest_of_the_batch() {
    let dir = TempDir::new("batch-partial-failure");
    let server = FakeProvider::start([
        Reply::content(ANSWER),
        Reply::Status {
            code: 401,
            body: "invalid key".to_string(),
        },
        Reply::content(ANSWER),
    ]);
    dir.write_config(&custom_provider(&server.base_url()));

    let paragraphs: Vec<String> = (0..3).map(|n| paragraph(&format!("p{n}w"), 150)).collect();
    let file = write(&dir, "long.md", &file_body(&paragraphs));

    let output = dir.plainly_with(
        &["explain", &file, "--format", "json"],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&output), FAILURE);

    let document: Value = serde_json::from_str(&stdout(&output)).expect("stdout is one document");
    let artifacts = document
        .as_array()
        .expect("the array shape follows the input");
    assert_eq!(artifacts.len(), 2, "the chunks that succeeded are kept");
    assert_eq!(artifacts[0]["passage"], paragraphs[0]);
    assert_eq!(artifacts[1]["passage"], paragraphs[2]);

    assert_eq!(
        server.chat_requests().len(),
        3,
        "the failed chunk did not stop the third"
    );

    let message = stderr(&output);
    assert!(message.contains("chunk 2/3 failed"), "{message}");
    assert!(
        message.contains("1 of 3 chunks failed"),
        "the run says what was lost: {message}"
    );
}

/// The history doubles as the cache, one Record per chunk (spec §6): a second
/// run of the same file asks nothing at all.
#[test]
fn a_second_run_of_the_same_file_asks_nothing_and_leaves_one_record_per_chunk() {
    let dir = TempDir::new("batch-cache");
    let server = FakeProvider::start((0..3).map(|_| Reply::content(ANSWER)));
    dir.write_config(&custom_provider(&server.base_url()));

    let paragraphs: Vec<String> = (0..3).map(|n| paragraph(&format!("p{n}w"), 150)).collect();
    let file = write(&dir, "long.md", &file_body(&paragraphs));

    let first = dir.plainly_with(
        &["explain", &file],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );
    assert_eq!(code(&first), SUCCESS, "{}", stderr(&first));
    assert_eq!(server.chat_requests().len(), 3);

    let second = dir.plainly_with(
        &["explain", &file],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&second), SUCCESS, "{}", stderr(&second));
    assert_eq!(
        server.chat_requests().len(),
        3,
        "nothing was asked a second time: {:?}",
        server.requests()
    );
    let message = stderr(&second);
    assert!(message.contains("will make 0 requests"), "{message}");
    assert_eq!(message.matches("reusing the stored Explanation").count(), 3);
    assert_eq!(stdout(&second).matches("### Original").count(), 3);

    let history = dir.plainly(&["history", "list"]);
    assert_eq!(code(&history), SUCCESS, "{}", stderr(&history));
    assert_eq!(
        stdout(&history).lines().count(),
        3,
        "one Record per chunk:\n{}",
        stdout(&history)
    );
}

/// Only the chunk that changed is asked again: the Lookup Key is per chunk
/// (spec §6), so an edit inside one paragraph does not re-buy the rest.
#[test]
fn only_the_chunk_that_changed_is_asked_again() {
    let dir = TempDir::new("batch-changed-chunk");
    let server = FakeProvider::start((0..4).map(|_| Reply::content(ANSWER)));
    dir.write_config(&custom_provider(&server.base_url()));

    let mut paragraphs: Vec<String> = (0..3).map(|n| paragraph(&format!("p{n}w"), 150)).collect();
    let file = write(&dir, "long.md", &file_body(&paragraphs));

    let first = dir.plainly_with(
        &["explain", &file],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );
    assert_eq!(code(&first), SUCCESS, "{}", stderr(&first));
    assert_eq!(server.chat_requests().len(), 3);

    let mut words: Vec<String> = paragraphs[1]
        .split_whitespace()
        .map(str::to_string)
        .collect();
    words[0] = "changed".to_string();
    paragraphs[1] = words.join(" ");
    write(&dir, "long.md", &file_body(&paragraphs));

    let second = dir.plainly_with(
        &["explain", &file],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&second), SUCCESS, "{}", stderr(&second));
    assert_eq!(
        server.chat_requests().len(),
        4,
        "only the edited chunk is asked: {:?}",
        server.chat_requests()
    );
    assert_eq!(sent_passage(&server, 3), paragraphs[1]);

    let message = stderr(&second);
    assert!(message.contains("will make 1 request"), "{message}");
    assert_eq!(message.matches("reusing the stored Explanation").count(), 2);
}

/// A Passage the batch repeats is one question — the Lookup Key is over the text
/// — so it is asked once and both occurrences are answered from that: paying
/// twice would also leave the second `remember` overwriting the first's Record.
#[test]
fn a_repeated_passage_is_asked_once() {
    let dir = TempDir::new("batch-repeat");
    let server = FakeProvider::start((0..2).map(|_| Reply::content(ANSWER)));
    dir.write_config(&custom_provider(&server.base_url()));

    let repeated = paragraph("repw", 150);
    let bracketed = paragraph("midw", 150);
    let paragraphs = vec![repeated.clone(), bracketed, repeated.clone()];
    let file = write(&dir, "long.md", &file_body(&paragraphs));

    let output = dir.plainly_with(
        &["explain", &file],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));

    // Two distinct Passages, two requests, three products.
    assert_eq!(server.chat_requests().len(), 2, "{:?}", server.requests());
    assert_eq!(sent_passage(&server, 0), paragraphs[0]);
    assert_eq!(sent_passage(&server, 1), paragraphs[1]);
    assert_eq!(stdout(&output).matches("### Original").count(), 3);

    let message = stderr(&output);
    assert!(
        message.contains("the same Passage as chunk 1/3"),
        "{message}"
    );
    assert!(message.contains("will make 2 requests"), "{message}");

    // One Record for the repeated Passage, not two.
    let history = dir.plainly(&["history", "list"]);
    assert_eq!(stdout(&history).lines().count(), 2, "{}", stdout(&history));
}

/// A repeat whose first occurrence produced nothing has no answer to reuse, and
/// asking the same failing question again would not produce one: both
/// occurrences are reported failed and the rest of the batch carries on.
#[test]
fn a_repeated_passage_whose_first_try_failed_is_not_asked_twice() {
    let dir = TempDir::new("batch-repeat-failed");
    let server = FakeProvider::start([
        // The first occurrence of the repeated Passage is refused…
        Reply::Status {
            code: 401,
            body: "invalid key".to_string(),
        },
        // …and the Passage between its two occurrences is answered.
        Reply::content(ANSWER),
    ]);
    dir.write_config(&custom_provider(&server.base_url()));

    let repeated = paragraph("repw", 150);
    let bracketed = paragraph("midw", 150);
    let paragraphs = vec![repeated.clone(), bracketed, repeated.clone()];
    let file = write(&dir, "long.md", &file_body(&paragraphs));

    let output = dir.plainly_with(
        &["explain", &file, "--format", "json"],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&output), FAILURE);
    assert_eq!(
        server.chat_requests().len(),
        2,
        "the repeated Passage is not asked twice: {:?}",
        server.chat_requests()
    );

    let document: Value = serde_json::from_str(&stdout(&output)).expect("stdout is one document");
    let artifacts = document.as_array().expect("a batch is an array");
    assert_eq!(artifacts.len(), 1, "only the middle chunk was answered");
    assert_eq!(artifacts[0]["passage"], paragraphs[1]);

    let message = stderr(&output);
    assert!(
        message.contains("chunk 3/3 failed: the same Passage as chunk 1/3 was not explained"),
        "{message}"
    );
    assert!(message.contains("2 of 3 chunks failed"), "{message}");
}
