//! `plainly explain` end to end: the real binary, temporary XDG directories, and
//! a fake provider on loopback.
//!
//! Nothing here needs an API key, a model or the outside network, which is the
//! point: the contract with a provider is testable without one.

mod support;

use serde_json::Value;

use support::TempDir;
use support::provider::{FakeProvider, Reply};
use support::{code, stderr, stdout};

const SUCCESS: i32 = 0;
const FAILURE: i32 = 1;
const USAGE: i32 = 2;
const NOT_CONFIGURED: i32 = 3;

const PASSAGE: &str = "The committee conducted a thorough investigation into the matter, \
                       but the manager had already gone to ground.";

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

/// A configuration that keeps the shipped DeepSeek preset but points it at the
/// stub, so the preset's own choices stay in play.
fn deepseek_at(endpoint: &str) -> String {
    format!(
        "[app]\n\
         provider = \"deepseek\"\n\n\
         [providers.deepseek]\n\
         endpoint = \"{endpoint}\"\n"
    )
}

#[test]
fn a_passage_on_stdin_comes_back_as_the_five_sections() {
    let dir = TempDir::new("explain-markdown");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));

    let product = stdout(&output);
    for heading in [
        "### Original",
        "### Comprehensible English",
        "### Key Help",
        "### Translation",
    ] {
        assert!(
            product.contains(heading),
            "missing {heading} in:\n{product}"
        );
    }
    // `grammar` is null, so the document leaves the section out (spec §4). A
    // headed but empty section would read as "generated nothing".
    assert!(
        !product.contains("### Grammar"),
        "no structural blocker means no section:\n{product}"
    );
    assert!(
        product.contains(PASSAGE),
        "the Original section:\n{product}"
    );
    assert!(
        product.contains("- `go to ground` → hide so that nobody can find you"),
        "the Key Help section:\n{product}"
    );

    // Human information goes to stderr, so the pipeline sees only the product.
    assert!(
        stderr(&output).contains("explaining with"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_custom_provider_is_sent_the_nested_schema() {
    let dir = TempDir::new("explain-wire-custom");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/v1/chat/completions");

    let body = &requests[0].body;
    assert_eq!(body["model"], "stub-model");
    assert_eq!(body["temperature"], 0);
    assert_eq!(body["max_tokens"], 2048);
    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(
        body["response_format"]["json_schema"]["schema"],
        plainly_core::wire_schema()
    );
    assert!(
        body["response_format"].get("schema").is_none(),
        "the schema must be nested, not flat: {body}"
    );

    assert_eq!(body["messages"][0]["role"], "system");
    let prompt = body["messages"][0]["content"]
        .as_str()
        .expect("the system message is text");
    assert!(
        prompt.contains("B2"),
        "the prompt carries the Level: {prompt}"
    );
    assert!(
        prompt.contains("Chinese"),
        "the prompt carries the Native Language: {prompt}"
    );
    assert_eq!(body["messages"][1]["content"], PASSAGE);
}

/// A shipped name pointed at another machine is an endpoint nobody has probed
/// yet, so the request takes the cautious shape rather than DeepSeek's. The
/// preset's own shape (json_object plus the canonical thinking switch) is a
/// property of api.deepseek.com and is asserted in core's `tests/chat.rs`,
/// which needs no network to ask for the body.
#[test]
fn an_overridden_endpoint_gets_the_cautious_shape() {
    let dir = TempDir::new("explain-wire-overridden");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&deepseek_at(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_DEEPSEEK_API_KEY", "test-key")]);
    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));

    let body = &server.requests()[0].body;
    // The model is still the preset's: the user did not choose one.
    assert_eq!(body["model"], "deepseek-flash");
    // A schema is asked for, because the risky guess is the other way: a
    // `json_object` request is a documented 400 on LM Studio.
    assert_eq!(body["response_format"]["type"], "json_schema");
    // And no thinking field at all: an unknown endpoint may reject one, and
    // DeepSeek's canonical spelling is not everyone's.
    assert!(body.get("thinking").is_none(), "{body}");
}

/// A setting that cannot take effect is said out loud rather than looking like
/// it worked: the shipped OpenAI preset has no thinking switch.
#[test]
fn thinking_that_cannot_take_effect_is_reported() {
    let dir = TempDir::new("explain-thinking-unsupported");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&format!(
        "[app]\nprovider = \"openai\"\n\n[providers.openai]\nthinking = \"on\"\nendpoint = \"{}\"\n",
        server.base_url()
    ));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_OPENAI_API_KEY", "test-key")]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    assert!(
        stderr(&output).contains("no thinking switch"),
        "{}",
        stderr(&output)
    );
    assert!(server.requests()[0].body.get("thinking").is_none());
}

#[test]
fn the_json_format_is_one_document_with_the_metadata() {
    let dir = TempDir::new("explain-json");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(
        &["--format", "json"],
        PASSAGE,
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));

    let document: Value =
        serde_json::from_str(&stdout(&output)).expect("stdout is one JSON document");

    assert_eq!(document["passage"], PASSAGE);
    assert_eq!(document["level"], "B2");
    assert_eq!(document["source_language"], "en");
    assert_eq!(document["native_language"], "Chinese");
    assert_eq!(document["provider"], "stub");
    assert_eq!(document["model"], "stub-model");
    assert_eq!(document["thinking"], "off");
    assert_eq!(document["artifact_version"], 1);
    assert_eq!(document["prompt_label"], "v6-synthesis");
    assert_eq!(
        document["prompt_version"]
            .as_str()
            .expect("the prompt version is a string")
            .len(),
        64,
        "the prompt version is the content hash"
    );

    // The model's four fields are flattened into the same document.
    assert_eq!(
        document["comprehensible"],
        "The committee tried hard to find out what had happened, but the manager had already gone into hiding."
    );
    assert_eq!(document["glosses"][0]["expression"], "go to ground");
    assert!(document["grammar"].is_null());
    assert!(document["translation"].is_string());
}

#[test]
fn a_named_file_is_read_verbatim() {
    let dir = TempDir::new("explain-file");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let passage = "Not until the auditors had gone did the manager admit that the figures had been doctored.\n";
    let file = dir.join("passage.md");
    std::fs::write(&file, passage).expect("the fixture file is writable");

    let output = dir.plainly_with(
        &["explain", file.to_str().expect("the path is UTF-8")],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    assert_eq!(
        server.requests()[0].body["messages"][1]["content"],
        passage,
        "the Passage goes to the model exactly as the file had it"
    );
}

/// `explain`'s arguments are accepted on both sides of the verb, so a flag
/// written before it is not silently dropped.
#[test]
fn a_format_written_before_the_subcommand_still_counts() {
    let dir = TempDir::new("explain-root-format");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(
        &["--format", "json", "explain"],
        PASSAGE,
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let document: Value = serde_json::from_str(&stdout(&output)).expect("the root flag chose json");
    assert_eq!(document["passage"], PASSAGE);
}

/// Because `explain` is the default command, its file argument is the root's
/// too: `plainly passage.md` is the same run as `plainly explain passage.md`.
#[test]
fn a_bare_file_path_is_the_same_run_as_the_explain_subcommand() {
    let dir = TempDir::new("explain-bare-file");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let passage = "The committee conducted a thorough investigation.";
    let file = dir.join("passage.md");
    std::fs::write(&file, passage).expect("the fixture file is writable");

    let output = dir.plainly_with(
        &[file.to_str().expect("the path is UTF-8")],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    assert_eq!(server.requests()[0].body["messages"][1]["content"], passage);
}

/// A failure asking again cannot fix. The status is 401 on purpose: a 5xx would
/// be retried, which would make this a slow test (five seconds of real backoff)
/// and would change what it asserts about how many calls were made.
#[test]
fn a_provider_failure_exits_one_and_leaves_stdout_empty() {
    let dir = TempDir::new("explain-http-error");
    let server = FakeProvider::start([Reply::Status {
        code: 401,
        body: "invalid key".to_string(),
    }]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), FAILURE);
    assert_eq!(stdout(&output), "", "a failure produces no product");
    let message = stderr(&output);
    assert!(message.contains("401"), "{message}");
    assert!(message.contains("invalid key"), "{message}");
    assert_eq!(server.requests().len(), 1, "a 401 is not worth retrying");
}

/// A transient failure is asked again, and the run carries on: the caller sees
/// one Explanation and the backoff, never the first 429.
#[test]
fn a_transient_failure_is_asked_again_and_succeeds() {
    let dir = TempDir::new("explain-retry-success");
    let server = FakeProvider::start([
        Reply::Status {
            code: 429,
            body: "slow down".to_string(),
        },
        Reply::content(ANSWER),
    ]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    assert!(stdout(&output).contains("### Original"));
    assert_eq!(server.requests().len(), 2);
    assert!(
        stderr(&output).contains("asking again in 1s"),
        "the wait is reported rather than looking like a hang: {}",
        stderr(&output)
    );
}

/// The same unusable answer twice is systematic drift, so the run stops there
/// and says what it did rather than spending a third call.
#[test]
fn an_answer_that_is_not_the_contract_exits_one() {
    let dir = TempDir::new("explain-malformed");
    let server = FakeProvider::start([
        Reply::content("I'd rather not, sorry."),
        Reply::content("I'd rather not, sorry."),
    ]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), FAILURE);
    assert_eq!(stdout(&output), "");
    let message = stderr(&output);
    assert!(
        message.contains("JSON"),
        "the failure names what went wrong: {message}"
    );
    assert!(
        message.contains("tried 2 times"),
        "how many attempts it took is part of the answer: {message}"
    );
    assert!(
        message.contains("Nothing was stored and the input was not changed"),
        "{message}"
    );
    assert_eq!(
        server.requests().len(),
        2,
        "the same answer twice is enough"
    );
}

/// A 4xx that rejects the request's shape is not retried: asking again would be
/// asking the same question, and tickets/05 downgrades the capability instead.
#[test]
fn a_rejected_request_shape_is_not_asked_again() {
    let dir = TempDir::new("explain-unsupported");
    let server = FakeProvider::start([Reply::Status {
        code: 400,
        body: "This response_format type is unavailable now".to_string(),
    }]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), FAILURE);
    assert_eq!(stdout(&output), "");
    let message = stderr(&output);
    assert!(message.contains("response_format"), "{message}");
    assert!(message.contains("not retried"), "{message}");
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn a_missing_key_is_not_configured_and_nothing_is_sent() {
    let dir = TempDir::new("explain-no-key");
    // The shipped OpenAI preset: an endpoint we ship, and a key it needs. No
    // endpoint override, so nothing about this test depends on a stub.
    let server = FakeProvider::start([]);
    dir.write_config("[app]\nprovider = \"openai\"\n");

    // No PLAINLY_OPENAI_API_KEY in the environment, and no keyring in tests.
    let output = dir.plainly_with(&[], PASSAGE, &[]);

    assert_eq!(code(&output), NOT_CONFIGURED);
    assert_eq!(stdout(&output), "");
    let message = stderr(&output);
    assert!(message.contains("PLAINLY_OPENAI_API_KEY"), "{message}");
    assert!(
        server.requests().is_empty(),
        "a run with no key must not reach the network"
    );
}

/// Fail fast, not after the pipe is drained: a missing key must be reported
/// without waiting for stdin to end, because stdin may be a large file or a
/// stream that never ends.
#[test]
fn a_missing_key_is_reported_without_waiting_for_stdin_to_end() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let dir = TempDir::new("explain-no-key-open-stdin");
    dir.write_config("[app]\nprovider = \"openai\"\n");

    let mut child = dir
        .command()
        .stdin(Stdio::piped())
        .spawn()
        .expect("the plainly binary is built alongside its tests");
    // Hold stdin open for the whole wait: a CLI that read first would block.
    let stdin = child.stdin.take().expect("stdin is piped");

    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().expect("the process is waitable") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("plainly waited for stdin before reporting the missing key");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    drop(stdin);

    assert_eq!(status.code(), Some(NOT_CONFIGURED));
}

/// A configuration file that is not valid TOML is a configuration problem, and
/// scripts branch on 3 to mean exactly that.
#[test]
fn a_configuration_file_that_cannot_be_parsed_is_not_configured() {
    let dir = TempDir::new("explain-broken-config");
    dir.write_config("this is not = = toml\n");

    let output = dir.plainly_with(&[], PASSAGE, &[]);

    assert_eq!(code(&output), NOT_CONFIGURED);
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains("not valid TOML"),
        "{}",
        stderr(&output)
    );
}

/// Providers do echo keys back in a 401, and stderr is where a CI log keeps it.
/// The status is 401, not 500, so the run stops at the first attempt: a 5xx
/// would be retried, and the test would assert about the wrong call.
#[test]
fn a_provider_error_does_not_repeat_a_key_back() {
    let dir = TempDir::new("explain-redacted-error");
    let secret = "sk-test-secret-0123456789abcdef";
    let server = FakeProvider::start([Reply::Status {
        code: 401,
        body: format!(
            r#"{{"error":"Your api key: {secret} is invalid. Authorization: Bearer {secret}"}}"#
        ),
    }]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", secret)]);

    assert_eq!(code(&output), FAILURE);
    let message = stderr(&output);
    assert!(!message.contains(secret), "the key was repeated: {message}");
    assert!(message.contains("<redacted>"), "{message}");
    assert!(message.contains("401"), "{message}");
}

/// The keyword heuristic alone would miss a short key with no recognisable
/// prefix and no label in front of it. The request knows the value it sent, so
/// the value is removed whatever shape the body put it in.
#[test]
fn a_short_key_without_a_prefix_is_removed_by_value() {
    let dir = TempDir::new("explain-redacted-short-key");
    let secret = "abc123";
    let server = FakeProvider::start([Reply::Status {
        code: 401,
        body: r#"{"error":"unauthorized for abc123"}"#.to_string(),
    }]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", secret)]);

    assert_eq!(code(&output), FAILURE);
    let message = stderr(&output);
    assert!(!message.contains(secret), "the key was repeated: {message}");
    assert!(message.contains("<redacted>"), "{message}");
}

#[test]
fn an_unknown_provider_name_is_not_configured() {
    let dir = TempDir::new("explain-unknown-provider");
    dir.write_config("[app]\nprovider = \"nosuch\"\n");

    let output = dir.plainly_with(&[], PASSAGE, &[]);

    assert_eq!(code(&output), NOT_CONFIGURED);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("nosuch"), "{}", stderr(&output));
}

#[test]
fn a_pipe_with_nothing_in_it_is_a_usage_error() {
    let dir = TempDir::new("explain-empty");
    // A usable provider first: with no key at all the run stops earlier, at
    // "not configured", which is the more actionable thing to say.
    dir.write_config(&custom_provider("http://127.0.0.1:1/v1"));

    let output = dir.plainly_with(&[], "", &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), USAGE);
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains("no Passage"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn a_file_that_cannot_be_read_is_a_usage_error() {
    let dir = TempDir::new("explain-missing-file");

    let output = dir.plainly_with(&["explain", "no/such/file.md"], "", &[]);

    assert_eq!(code(&output), USAGE);
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains("no/such/file.md"),
        "{}",
        stderr(&output)
    );
}
