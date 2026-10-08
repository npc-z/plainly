//! `plainly explain` end to end: the real binary, temporary XDG directories, and
//! a fake provider on loopback.
//!
//! Nothing here needs an API key, a model or the outside network, which is the
//! point: the contract with a provider is testable without one.

mod support;

use serde_json::{Value, json};

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

    let requests = server.chat_requests();
    assert_eq!(
        requests.len(),
        1,
        "one Passage, one request: {:?}",
        server.requests()
    );
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
/// yet, so the request takes what the *probe* found rather than what the preset
/// knew. The preset's own shape (json_object plus the canonical thinking switch)
/// is a property of api.deepseek.com and is asserted in core's `tests/chat.rs`,
/// which needs no network to ask for the body.
#[test]
fn an_overridden_endpoint_is_probed_rather_than_trusted() {
    let dir = TempDir::new("explain-wire-overridden");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&deepseek_at(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_DEEPSEEK_API_KEY", "test-key")]);
    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));

    assert_eq!(
        server.probe_requests().len(),
        1,
        "an endpoint the preset does not name is asked what it takes"
    );
    let body = &server.chat_requests()[0].body;
    // The model is still the preset's: the user did not choose one.
    assert_eq!(body["model"], "deepseek-flash");
    // The schema is asked for because the probe found an endpoint that takes
    // one, not because DeepSeek's name says so.
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
    assert!(server.chat_requests()[0].body.get("thinking").is_none());
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
    assert_eq!(document["prompt_label"], "v7-descriptors");
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
        server.chat_requests()[0].body["messages"][1]["content"],
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
    assert_eq!(
        server.chat_requests()[0].body["messages"][1]["content"],
        passage
    );
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
    assert_eq!(
        server.chat_requests().len(),
        1,
        "a 401 is not worth retrying: {:?}",
        server.requests()
    );
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
    assert_eq!(server.chat_requests().len(), 2);
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
        server.chat_requests().len(),
        2,
        "the same answer twice is enough"
    );
}

/// A 4xx that rejects the request's shape is not retried as-is: the capability
/// is what is wrong, so the endpoint is probed again and the Passage is asked
/// for once more with the shape the endpoint says it takes (tickets/05).
#[test]
fn a_rejected_request_shape_is_probed_again_and_asked_once_more() {
    let dir = TempDir::new("explain-unsupported");
    let server = FakeProvider::start([
        // The first run: the endpoint takes a schema, and answers.
        Reply::content(ANSWER),
        // The second run: the same shape is now rejected…
        Reply::Status {
            code: 400,
            body: "This response_format type is unavailable now".to_string(),
        },
        // …and the downgraded shape is answered.
        Reply::content(ANSWER),
    ]);
    dir.write_config(&custom_provider(&server.base_url()));

    let first = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&first), SUCCESS, "{}", stderr(&first));

    // The endpoint changes its mind, which is the case the cached conclusion
    // cannot survive and the probe has to hear about.
    server.set_capability(support::provider::StubCapability::without_schema());

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    assert!(stdout(&output).contains("### Original"));

    let message = stderr(&output);
    assert!(
        message.contains("rejected the shape Plainly assumed"),
        "the correction is reported rather than made silently: {message}"
    );
    assert!(message.contains("best effort"), "{message}");

    let chats = server.chat_requests();
    assert_eq!(
        chats.len(),
        3,
        "one each run, plus the corrected one: {chats:?}"
    );
    assert_eq!(chats[1].body["response_format"]["type"], "json_schema");
    assert_eq!(
        chats[2].body["response_format"],
        json!({ "type": "json_object" }),
        "the second ask uses the downgraded shape"
    );
}

/// A probe the endpoint will not answer is not a capability: the run takes the
/// cautious shape and says why.
#[test]
fn a_probe_that_cannot_be_answered_falls_back_to_best_effort() {
    let dir = TempDir::new("explain-probe-failed");
    let server = FakeProvider::start_with(
        support::provider::StubCapability {
            probe_status: Some(500),
            ..support::provider::StubCapability::default()
        },
        [Reply::content(ANSWER)],
    );
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let message = stderr(&output);
    assert!(message.contains("could not probe"), "{message}");
    assert!(message.contains("best effort"), "{message}");
    assert_eq!(
        server.chat_requests()[0].body["response_format"],
        json!({ "type": "json_object" }),
        "nothing is known, so nothing risky is asked for"
    );
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

/// A run says which tier it was held to. "The endpoint enforces the contract"
/// and "we check the answer ourselves and ask again" are not the same promise,
/// and the user cannot tell them apart from a run that says nothing (spec §7).
#[test]
fn a_run_says_which_contract_tier_it_was_held_to() {
    let dir = TempDir::new("explain-tier-enforced");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    let enforced = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&enforced), SUCCESS, "{}", stderr(&enforced));
    assert!(
        stderr(&enforced).contains("contract: enforced by the endpoint"),
        "{}",
        stderr(&enforced)
    );

    let dir = TempDir::new("explain-tier-besteffort");
    let server = FakeProvider::start_with(
        support::provider::StubCapability::without_schema(),
        [Reply::content(ANSWER)],
    );
    dir.write_config(&custom_provider(&server.base_url()));

    let best_effort = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&best_effort), SUCCESS, "{}", stderr(&best_effort));
    assert!(
        stderr(&best_effort).contains("contract: best effort"),
        "{}",
        stderr(&best_effort)
    );
}

/// A failed run reports the tier the *rejected* request carried, not whatever a
/// later probe concluded. Reporting the re-probe's cautious guess would tell the
/// user their request was held to a contract it never was.
#[test]
fn a_failed_run_reports_the_tier_its_request_carried() {
    let dir = TempDir::new("explain-failure-tier");
    let server = FakeProvider::start([
        // The first run learns the endpoint takes a schema, and is answered.
        Reply::content(ANSWER),
        // The second run sends that shape and is rejected; the probe cannot be
        // answered either, so there is nothing to correct it to.
        Reply::Status {
            code: 400,
            body: "This response_format type is unavailable now".to_string(),
        },
    ]);
    dir.write_config(&custom_provider(&server.base_url()));

    let first = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&first), SUCCESS, "{}", stderr(&first));

    server.set_capability(support::provider::StubCapability {
        probe_status: Some(500),
        ..support::provider::StubCapability::default()
    });

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), FAILURE);
    assert_eq!(stdout(&output), "");
    let message = stderr(&output);
    assert!(
        message.contains("contract: enforced by the endpoint"),
        "the rejected request carried a schema: {message}"
    );
    assert!(
        !message.contains("contract: best effort"),
        "the cautious shape was never sent: {message}"
    );
    assert!(
        message.contains("could not settle it"),
        "why the Passage was not re-sent: {message}"
    );
    assert_eq!(server.chat_requests().len(), 2, "one request per run");
}

/// A conclusion that cannot be cached costs a re-probe next time and nothing
/// else: the run still produces its Explanation, and says what was lost.
#[test]
fn a_probe_result_that_cannot_be_cached_is_said_so() {
    let dir = TempDir::new("explain-cache-unwritable");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    // The cache directory is occupied by a file, so the conclusion cannot be
    // written.
    let cache = dir.join("cache/dev.plainly.app");
    std::fs::create_dir_all(&cache).expect("the cache directory is creatable");
    std::fs::write(cache.join("providers"), "not a directory").expect("the fixture is writable");

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    assert!(stdout(&output).contains("### Original"));
    assert!(
        stderr(&output).contains("could not be cached"),
        "{}",
        stderr(&output)
    );
}

/// A local provider is named as local: the model that answered is provenance,
/// and so is the fact that nobody can vouch for it (spec §12, tickets/06).
#[test]
fn a_local_provider_is_named_as_local_beside_its_model() {
    let dir = TempDir::new("explain-local-provenance");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&format!(
        "[app]\nprovider = \"ollama\"\n\n\
         [providers.ollama]\nendpoint = \"http://{}/v1\"\nmodel = \"qwen3.5:4b\"\n",
        server.authority().trim_start_matches("http://")
    ));

    let output = dir.plainly_with(&[], PASSAGE, &[]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let rendered = stdout(&output);
    for section in [
        "### Original",
        "### Comprehensible English",
        "### Key Help",
        "### Translation",
    ] {
        assert!(rendered.contains(section), "missing {section}:\n{rendered}");
    }

    let message = stderr(&output);
    assert!(
        message.contains("qwen3.5:4b"),
        "the model that answered: {message}"
    );
    assert!(
        message.contains("local"),
        "and that it runs on this machine: {message}"
    );
    assert!(
        !rendered.contains("local"),
        "the product stays the Explanation; the caveat is stderr: {rendered}"
    );
}

/// A user's appendix is what failed the contract, and the way out is one flag.
/// The retry records the prompt that actually produced the answer — the factory
/// one — rather than the prompt that failed (spec §5).
#[test]
fn a_prompt_that_fails_the_contract_offers_the_factory_one() {
    let dir = TempDir::new("explain-prompt-contract");
    let server = FakeProvider::start([
        // The appended prompt fails the contract twice, which is drift, not noise.
        Reply::content("I'd rather not, sorry."),
        Reply::content("I'd rather not, sorry."),
        // The retry on the factory prompt is answered.
        Reply::content(ANSWER),
    ]);
    dir.write_config(&format!(
        "[app]\nprovider = \"stub\"\n\n\
         [providers.stub]\nendpoint = \"{}\"\nmodel = \"stub-model\"\n\n\
         [prompts]\nappendix = \"Always answer in one line.\"\n",
        server.base_url()
    ));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), FAILURE);
    assert_eq!(stdout(&output), "", "a failure produces no product");
    let message = stderr(&output);
    assert!(
        message.contains("your prompt did not pass the contract"),
        "the failure names what did not pass: {message}"
    );
    assert!(
        message.contains("--factory-prompt"),
        "and how to retry: {message}"
    );

    let first_run = server.chat_requests();
    let sent = first_run[0].body["messages"][0]["content"]
        .as_str()
        .expect("the system message is text");
    assert!(
        sent.contains("Always answer in one line."),
        "the appendix reached the model: {sent}"
    );

    let retry = dir.plainly_with(
        &["--factory-prompt", "--format", "json"],
        PASSAGE,
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&retry), SUCCESS, "{}", stderr(&retry));
    let document: Value =
        serde_json::from_str(&stdout(&retry)).expect("stdout is one JSON document");
    assert_eq!(document["prompt_version"], plainly_core::Prompt::factory().version());
    assert_eq!(document["prompt_label"], "v7-descriptors");

    let both_runs = server.chat_requests();
    let retried = both_runs
        .last()
        .expect("the retry was sent")
        .body["messages"][0]["content"]
        .as_str()
        .expect("the system message is text");
    assert!(
        !retried.contains("Always answer in one line."),
        "the retry leaves the user's appendix out: {retried}"
    );
    assert!(
        retried.contains("B2 ("),
        "the level still travels with its meaning: {retried}"
    );
}

/// The record carries the prompt's hash and its human-readable label, and not
/// the prompt's text: a record is rendered from its own fields, so a later
/// factory prompt cannot invalidate it (spec §5).
#[test]
fn a_record_carries_the_hash_and_the_label_but_not_the_prompt() {
    let dir = TempDir::new("explain-prompt-version");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    let appendix = "Always answer in one line.";
    dir.write_config(&format!(
        "[app]\nprovider = \"stub\"\n\n\
         [providers.stub]\nendpoint = \"{}\"\nmodel = \"stub-model\"\n\n\
         [prompts]\nappendix = \"{appendix}\"\n",
        server.base_url()
    ));

    let output = dir.plainly_with(
        &["--format", "json"],
        PASSAGE,
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let product = stdout(&output);
    let document: Value = serde_json::from_str(&product).expect("stdout is one JSON document");

    let effective = plainly_core::prompt::Prompt::from_config(&plainly_core::Prompts {
        appendix: appendix.to_string(),
        ..Default::default()
    });
    assert_eq!(document["prompt_version"], effective.version());
    assert_ne!(
        document["prompt_version"],
        plainly_core::Prompt::factory().version(),
        "the appended prompt is a different prompt"
    );
    assert_eq!(document["prompt_label"], "v7-descriptors");
    assert!(
        !product.contains(appendix) && !product.contains("You explain hard English"),
        "the prompt body is not stored in the record: {product}"
    );
}

/// The factory prompt failing the contract — the default configuration, where
/// there is no appendix to blame — still says what did not pass it, and does not
/// offer a fallback that is already what ran.
#[test]
fn the_factory_prompt_failing_the_contract_says_so_without_offering_itself() {
    let dir = TempDir::new("explain-factory-contract");
    let server = FakeProvider::start([
        Reply::content("I'd rather not, sorry."),
        Reply::content("I'd rather not, sorry."),
    ]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);

    assert_eq!(code(&output), FAILURE);
    let message = stderr(&output);
    assert!(
        message.contains("the factory prompt did not pass the contract"),
        "{message}"
    );
    assert!(
        !message.contains("--factory-prompt"),
        "nothing is offered when the factory prompt is already what ran: {message}"
    );
}
