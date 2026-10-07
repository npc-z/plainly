//! `plainly providers`: the status report, the forced re-probe, and what the
//! capability cache does and does not survive.
//!
//! The real binary, temporary XDG directories, and the loopback stub from
//! ticket 03 — extended to answer a probe (`/v1/models`, and a capability of the
//! test's choosing) as well as a Passage.

mod support;

use support::TempDir;
use support::provider::{FakeProvider, Reply, StubCapability};
use support::{code, stderr, stdout};

const SUCCESS: i32 = 0;
const FAILURE: i32 = 1;
const NOT_CONFIGURED: i32 = 3;

const PASSAGE: &str = "The committee conducted a thorough investigation into the matter, \
                       but the manager had already gone to ground.";

const ANSWER: &str = r#"{
  "comprehensible": "The committee tried hard to find out what had happened, but the manager had already gone into hiding.",
  "glosses": [{ "expression": "go to ground", "gloss": "hide so that nobody can find you" }],
  "grammar": null,
  "translation": "委员会对此事进行了彻底调查，但那位经理已经躲了起来。"
}"#;

/// A provider Plainly ships no preset for: the endpoint and model are the
/// user's, which is the custom-provider path.
fn custom_provider(endpoint: &str) -> String {
    format!(
        "[app]\n\
         provider = \"stub\"\n\n\
         [providers.stub]\n\
         endpoint = \"{endpoint}\"\n\
         model = \"stub-model\"\n"
    )
}

/// The capability cache file the CLI writes for one provider.
fn cache_file(dir: &TempDir, provider: &str) -> std::path::PathBuf {
    dir.join(&format!("cache/dev.plainly.app/providers/{provider}.json"))
}

#[test]
fn the_status_report_names_the_provider_model_and_thinking() {
    let dir = TempDir::new("providers-status");
    dir.write_config(&custom_provider("http://127.0.0.1:1/v1"));

    let output = dir.plainly(&["providers"]);

    assert_eq!(code(&output), SUCCESS);
    let report = stdout(&output);
    for expected in ["provider   stub", "model      stub-model", "thinking   off"] {
        assert!(
            report.contains(expected),
            "missing {expected} in:\n{report}"
        );
    }
    assert_eq!(
        stderr(&output),
        "",
        "a read of local state says nothing else"
    );
}

/// Nothing has asked the endpoint anything, and the report says so rather than
/// showing a capability nobody established. The status is a read: it must not
/// reach the network, so the stub sees no request at all.
#[test]
fn the_status_report_says_when_nothing_has_been_probed_yet() {
    let dir = TempDir::new("providers-status-unprobed");
    let server = FakeProvider::start([]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly(&["providers"]);

    assert_eq!(code(&output), SUCCESS);
    assert!(
        stdout(&output).contains("not probed yet"),
        "{}",
        stdout(&output)
    );
    assert!(
        server.requests().is_empty(),
        "a display is not a probe: {:?}",
        server.requests()
    );
}

#[test]
fn probing_reports_the_enforced_tier_and_keeps_the_conclusion() {
    let dir = TempDir::new("providers-probe-enforced");
    let server = FakeProvider::start([]);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly(&["providers", "probe"]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let report = stdout(&output);
    assert!(report.contains("enforced by the endpoint"), "{report}");
    assert!(
        report.contains("models     1: stub-model"),
        "the list the endpoint served: {report}"
    );
    assert!(
        stderr(&output).contains("probing stub at"),
        "the wait is announced: {}",
        stderr(&output)
    );

    assert!(
        cache_file(&dir, "stub").exists(),
        "the conclusion belongs in the cache directory"
    );
    let cached = std::fs::read_to_string(cache_file(&dir, "stub")).expect("the entry is readable");
    assert!(cached.contains("\"enforced\""), "{cached}");
}

/// The documented DeepSeek signal, end to end: `json_schema` is a 400 and
/// `json_object` is a 200, so the endpoint is learned as best-effort and the
/// report says which tier a run will be on.
#[test]
fn probing_reports_best_effort_when_json_schema_is_rejected() {
    let dir = TempDir::new("providers-probe-besteffort");
    let server = FakeProvider::start_with(StubCapability::without_schema(), []);
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly(&["providers", "probe"]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let report = stdout(&output);
    assert!(report.contains("best effort"), "{report}");
    assert!(
        !report.contains("enforced by the endpoint"),
        "the two tiers are told apart: {report}"
    );

    // Two probe requests: the schema that was refused, and the JSON request
    // that was not.
    assert_eq!(server.probe_requests().len(), 2, "{:?}", server.requests());
}

#[test]
fn a_probe_that_cannot_reach_the_endpoint_says_so_and_exits_one() {
    let dir = TempDir::new("providers-probe-unreachable");
    // Nothing is listening on port 1: the endpoint exists in configuration and
    // nowhere else.
    dir.write_config(&custom_provider("http://127.0.0.1:1/v1"));

    let output = dir.plainly(&["providers", "probe"]);

    assert_eq!(code(&output), FAILURE);
    let report = stdout(&output);
    assert!(
        report.contains("best effort"),
        "a run would take the cautious shape: {report}"
    );
    let reason = stderr(&output).to_lowercase();
    assert!(
        reason.contains("stub:") && reason.contains("refused"),
        "the reason reaches a person, named after the provider: {}",
        stderr(&output)
    );
    assert!(
        !cache_file(&dir, "stub").exists(),
        "a failure to answer is not a capability"
    );
}

#[test]
fn probing_a_named_provider_does_not_need_it_to_be_the_current_one() {
    let dir = TempDir::new("providers-probe-named");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&format!(
        "[app]\nprovider = \"deepseek\"\n\n\
         [providers.stub]\nendpoint = \"{}\"\nmodel = \"stub-model\"\n",
        server.base_url()
    ));

    let output = dir.plainly(&["providers", "probe", "stub"]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    assert!(
        stdout(&output).contains("provider   stub"),
        "{}",
        stdout(&output)
    );
    assert!(cache_file(&dir, "stub").exists());
}

#[test]
fn probing_a_name_nobody_configured_is_not_configured() {
    let dir = TempDir::new("providers-probe-unknown");
    dir.write_config("[app]\nprovider = \"deepseek\"\n");

    let output = dir.plainly(&["providers", "probe", "nosuch"]);

    assert_eq!(code(&output), NOT_CONFIGURED);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("nosuch"), "{}", stderr(&output));
}

#[test]
fn a_cloud_probe_without_a_key_is_not_configured_and_sends_nothing() {
    let dir = TempDir::new("providers-probe-no-key");
    dir.write_config("[app]\nprovider = \"openai\"\n");

    let output = dir.plainly(&["providers", "probe"]);

    assert_eq!(code(&output), NOT_CONFIGURED);
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains("PLAINLY_OPENAI_API_KEY"),
        "{}",
        stderr(&output)
    );
}

/// The cached conclusion is what a run starts from, and it costs one probe
/// however many runs follow.
#[test]
fn a_run_probes_once_and_the_next_run_reads_the_conclusion() {
    let dir = TempDir::new("providers-cache-reuse");
    let server = FakeProvider::start([Reply::content(ANSWER), Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));

    for _ in 0..2 {
        let output = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
        assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    }

    assert_eq!(server.chat_requests().len(), 2, "one request per run");
    assert_eq!(
        server.probe_requests().len(),
        1,
        "the second run reads what the first one learned"
    );
}

/// Clearing the cache loses the conclusion and nothing else: the provider the
/// user chose lives in the configuration file, which the cache never touches.
#[test]
fn clearing_the_cache_does_not_lose_the_provider_choice() {
    let dir = TempDir::new("providers-cache-cleared");
    let server = FakeProvider::start([]);
    dir.write_config(&custom_provider(&server.base_url()));

    let probed = dir.plainly(&["providers", "probe"]);
    assert_eq!(code(&probed), SUCCESS, "{}", stderr(&probed));
    let before = std::fs::read_to_string(dir.config_file()).expect("the config is readable");

    std::fs::remove_dir_all(dir.join("cache")).expect("the cache directory is removable");

    let after = dir.plainly(&["providers"]);
    assert_eq!(code(&after), SUCCESS);
    let report = stdout(&after);
    assert!(report.contains("provider   stub"), "{report}");
    assert!(
        report.contains("not probed yet"),
        "the conclusion is gone, and the report says so: {report}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.config_file()).expect("the config is readable"),
        before,
        "the cache and the configuration are separate: a probe writes one of them"
    );
}

/// The conclusion belongs to one endpoint and one model: pointing the provider
/// somewhere else asks the new endpoint rather than trusting the old answer.
#[test]
fn a_changed_endpoint_is_probed_again() {
    let dir = TempDir::new("providers-endpoint-changed");
    let first = FakeProvider::start([]);
    dir.write_config(&custom_provider(&first.base_url()));
    assert_eq!(code(&dir.plainly(&["providers", "probe"])), SUCCESS);

    let second = FakeProvider::start_with(StubCapability::without_schema(), []);
    dir.write_config(&custom_provider(&second.base_url()));
    let output = dir.plainly(&["providers"]);

    assert!(
        stdout(&output).contains("not probed yet"),
        "the old conclusion does not describe this endpoint: {}",
        stdout(&output)
    );

    let reprobed = dir.plainly(&["providers", "probe"]);
    assert_eq!(code(&reprobed), SUCCESS);
    assert!(
        stdout(&reprobed).contains("best effort"),
        "the new endpoint's own answer: {}",
        stdout(&reprobed)
    );
}

/// The vendor route with a surface of its own: Ollama is asked on `/api/chat`,
/// with `format` carrying the schema.
#[test]
fn an_ollama_endpoint_is_probed_and_run_on_its_native_route() {
    let dir = TempDir::new("providers-ollama-native");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&format!(
        "[app]\nprovider = \"ollama\"\n\n\
         [providers.ollama]\nendpoint = \"http://{}/v1\"\nmodel = \"qwen3.5:4b\"\n",
        server.authority().trim_start_matches("http://")
    ));

    let output = dir.plainly_with(&[], PASSAGE, &[]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let probes = server.probe_requests();
    assert!(!probes.is_empty(), "the endpoint was probed");
    for request in probes.iter().chain(server.chat_requests().iter()) {
        assert_eq!(request.path, "/api/chat", "{request:?}");
        assert!(
            request.body["format"].is_object(),
            "the schema travels in format: {request:?}"
        );
        assert_eq!(request.body["stream"], false);
    }
}

/// An endpoint that speaks the protocol without serving the vendor's route is
/// still usable: the compatibility layer is the fallback, not a retry.
#[test]
fn an_ollama_endpoint_without_the_native_route_falls_back_to_the_v1_surface() {
    let dir = TempDir::new("providers-ollama-fallback");
    let server = FakeProvider::start_with(
        StubCapability {
            native: false,
            ..StubCapability::default()
        },
        [Reply::content(ANSWER)],
    );
    dir.write_config(&format!(
        "[app]\nprovider = \"ollama\"\n\n\
         [providers.ollama]\nendpoint = \"http://{}/v1\"\nmodel = \"qwen3.5:4b\"\n",
        server.authority().trim_start_matches("http://")
    ));

    let output = dir.plainly_with(&[], PASSAGE, &[]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    assert!(
        stdout(&output).contains("### Original"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        server
            .chat_requests()
            .last()
            .expect("a request was made")
            .path,
        "/v1/chat/completions",
        "the answer came from the compatibility route: {:?}",
        server.requests()
    );
    assert!(
        server
            .chat_requests()
            .iter()
            .any(|request| request.path == "/api/chat"),
        "the native route was tried first: {:?}",
        server.requests()
    );
}

/// "The endpoint serves no models" and "nobody could read the list" are
/// different facts, and the report says which one happened.
#[test]
fn a_model_list_that_cannot_be_read_is_not_reported_as_empty() {
    let dir = TempDir::new("providers-models-failed");
    let server = FakeProvider::start_with(
        StubCapability {
            models_status: Some(500),
            ..StubCapability::default()
        },
        [],
    );
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly(&["providers", "probe"]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let report = stdout(&output);
    assert!(report.contains("could not be listed"), "{report}");
    assert!(!report.contains("none listed"), "{report}");
}
