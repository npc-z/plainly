//! `plainly providers`: the status report, the forced re-probe, local discovery
//! and the model selection, and what the capability cache does and does not
//! survive.
//!
//! The real binary, temporary XDG directories, and the loopback stub from
//! ticket 03 — extended to answer a probe (`/v1/models`, and a capability of the
//! test's choosing) as well as a Passage.

mod support;

use plainly_core::Config;
use serde_json::{Value, json};

use support::TempDir;
use support::provider::{FakeProvider, Reply, StubCapability};
use support::{code, stderr, stdout};

const SUCCESS: i32 = 0;
const FAILURE: i32 = 1;
const USAGE: i32 = 2;
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

    let first = dir.plainly_with(&[], PASSAGE, &[("PLAINLY_STUB_API_KEY", "test-key")]);
    assert_eq!(code(&first), SUCCESS, "{}", stderr(&first));

    // `--regenerate`, because this is about the *capability* cache: without it
    // the second run would be answered from the history and would not reach the
    // endpoint at all, which proves nothing about probing.
    let second = dir.plainly_with(
        &["--regenerate"],
        PASSAGE,
        &[("PLAINLY_STUB_API_KEY", "test-key")],
    );
    assert_eq!(code(&second), SUCCESS, "{}", stderr(&second));

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

/// The listing a person reads before choosing: which runtime, at which endpoint,
/// and per model what the runtime says about it without loading anything.
#[test]
fn discovery_lists_a_runtime_and_what_it_serves() {
    let dir = TempDir::new("providers-discover");
    let server = FakeProvider::start_with(
        StubCapability {
            models: json!({ "data": [
                { "id": "qwen3.5:4b", "loaded": true, "max_context_length": 16384 },
                { "id": "qwen3.5:2b", "state": "unloaded", "context_length": 2048 },
            ]}),
            ..StubCapability::default()
        },
        [],
    );
    dir.write_config(&custom_provider(&server.base_url()));

    let output = dir.plainly(&["providers", "discover"]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let listing = stdout(&output);
    let runtime = block(&listing, &format!("stub at {}", server.base_url()));
    assert!(runtime.contains("qwen3.5:4b"), "{listing}");
    assert!(
        runtime.contains("qwen3.5:4b  loaded  context 16384"),
        "the state and context it reported: {runtime}"
    );
    assert!(
        runtime.contains("qwen3.5:2b  unloaded  context 2048"),
        "{runtime}"
    );
    assert!(
        runtime.contains("too small"),
        "a context below one Passage's budget is flagged before it is chosen: {runtime}"
    );
    assert!(
        listing.contains("plainly providers use"),
        "and the listing says how to choose one: {listing}"
    );
    assert!(
        stderr(&output).contains("looking for local runtimes"),
        "the scan says where it is about to look: {}",
        stderr(&output)
    );
}

/// The listing block one runtime occupies: from its header to the next blank
/// line.
///
/// Every scan also asks the real common ports, so a test that asserts what a
/// runtime's own models say has to look at that runtime's block and not at a
/// whole listing a developer's own Ollama may have added to.
fn block<'a>(listing: &'a str, header: &str) -> &'a str {
    let start = listing
        .find(header)
        .unwrap_or_else(|| panic!("no {header:?} in:\n{listing}"));
    let rest = &listing[start..];
    &rest[..rest.find("\n\n").unwrap_or(rest.len())]
}

/// Choosing a model is one act: it names the runtime, writes the model into
/// that provider's section, and makes it the provider a run uses.
#[test]
fn selecting_a_model_writes_it_into_the_provider_section() {
    let dir = TempDir::new("providers-use");
    let server = FakeProvider::start_with(
        StubCapability {
            models: json!({ "data": [{ "id": "chosen" }, { "id": "other" }] }),
            ..StubCapability::default()
        },
        [],
    );
    dir.write_config(&format!(
        "[app]\nprovider = \"deepseek\"\n\n\
         [providers.mine]\nendpoint = \"{}\"\n",
        server.base_url()
    ));

    let output = dir.plainly(&["providers", "use", "mine", "chosen"]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    assert_eq!(stdout(&output), "", "a selection is not a product");
    assert!(
        stderr(&output).contains("chosen"),
        "the choice is confirmed: {}",
        stderr(&output)
    );

    let written = Config::parse(
        &std::fs::read_to_string(dir.config_file()).expect("the configuration is readable"),
    )
    .expect("what Plainly wrote is valid configuration");
    assert_eq!(written.app.provider, "mine");
    assert_eq!(
        written.providers["mine"].model.as_deref(),
        Some("chosen"),
        "the model is the selection: {written:?}"
    );
    assert_eq!(
        written.providers["mine"].endpoint.as_deref(),
        Some(server.base_url().as_str()),
        "the endpoint the runtime was found at is kept"
    );
}

/// A model the runtime does not serve is not a selection: the ids it does serve
/// are the actionable part, and nothing is written.
#[test]
fn selecting_a_model_the_runtime_does_not_serve_changes_nothing() {
    let dir = TempDir::new("providers-use-unknown-model");
    let server = FakeProvider::start_with(
        StubCapability {
            models: json!({ "data": [{ "id": "chosen" }, { "id": "other" }] }),
            ..StubCapability::default()
        },
        [],
    );
    let config = format!(
        "[app]\nprovider = \"deepseek\"\n\n\
         [providers.mine]\nendpoint = \"{}\"\n",
        server.base_url()
    );
    dir.write_config(&config);

    let output = dir.plainly(&["providers", "use", "mine", "nosuch"]);

    assert_eq!(code(&output), USAGE);
    assert_eq!(stdout(&output), "");
    let message = stderr(&output);
    assert!(message.contains("nosuch"), "{message}");
    assert!(
        message.contains("chosen") && message.contains("other"),
        "the models it does serve: {message}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.config_file()).expect("the configuration is readable"),
        config,
        "a rejected selection writes nothing"
    );
}

/// No runtime answered: the command says where it looked and what to do, rather
/// than guessing a port or failing silently.
#[test]
fn selecting_a_model_for_a_runtime_that_does_not_answer_is_actionable() {
    let dir = TempDir::new("providers-use-unreachable");
    let config = "[app]\nprovider = \"deepseek\"\n\n\
                  [providers.mine]\nendpoint = \"http://127.0.0.1:1/v1\"\n";
    dir.write_config(config);

    let output = dir.plainly(&["providers", "use", "mine", "chosen"]);

    assert_eq!(code(&output), NOT_CONFIGURED);
    assert_eq!(stdout(&output), "");
    let message = stderr(&output);
    assert!(message.contains("mine"), "named: {message}");
    assert!(
        message.contains("http://127.0.0.1:1/v1"),
        "where it looked: {message}"
    );
    assert!(
        message.contains("plainly config set providers.mine.endpoint"),
        "and the fix: {message}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.config_file()).expect("the configuration is readable"),
        config,
        "nothing is written when there is nothing to select from"
    );
}

/// A provider that is not on this machine is not a local selection: the model
/// is set directly, and the message says so instead of scanning.
#[test]
fn selecting_a_model_for_a_remote_provider_points_at_config_set() {
    let dir = TempDir::new("providers-use-remote");
    dir.write_config("[app]\nprovider = \"deepseek\"\n");

    let output = dir.plainly(&["providers", "use", "deepseek", "deepseek-chat"]);

    assert_eq!(code(&output), NOT_CONFIGURED);
    assert_eq!(stdout(&output), "");
    let message = stderr(&output);
    assert!(message.contains("deepseek-chat"), "{message}");
    assert!(
        message.contains("plainly config set providers.deepseek.model"),
        "the way to set a hosted model: {message}"
    );
}

/// The shape the llama.cpp router on this machine actually answers with: the
/// state under `status.value` and the context length only in the model's launch
/// argv. Discovery reads both, which is what makes "is it big enough" answerable
/// before the model is loaded (tickets/06).
#[test]
fn discovery_reads_the_routers_own_model_shape() {
    let dir = TempDir::new("providers-discover-router");
    let server = FakeProvider::start_with(
        StubCapability {
            models: json!({ "data": [{
                "id": "unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M",
                "object": "model",
                "status": {
                    "value": "unloaded",
                    "args": ["llama-server", "--ctx-size", "16384", "--n-gpu-layers", "99"],
                },
            }]}),
            ..StubCapability::default()
        },
        [],
    );
    dir.write_config(&format!(
        "[app]\nprovider = \"deepseek\"\n\n\
         [providers.llamacpp]\nendpoint = \"{}\"\nmodel = \"unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M\"\n",
        server.base_url()
    ));

    let output = dir.plainly(&["providers", "discover"]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let listing = stdout(&output);
    let runtime = block(&listing, &format!("llamacpp at {}", server.base_url()));
    assert!(
        runtime.contains("unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M"),
        "{listing}"
    );
    assert!(
        runtime.contains("unloaded"),
        "the state the router reported: {runtime}"
    );
    assert!(
        runtime.contains("context 16384"),
        "the context from the launch argv: {runtime}"
    );
    assert!(
        !runtime.contains("too small"),
        "16384 holds a Passage: {runtime}"
    );
}

/// The context flag the listing shows is repeated at the moment of choosing: a
/// model whose reported context cannot hold a Passage is still selectable — the
/// runtime may serve more than it reports — but not silently.
#[test]
fn selecting_a_model_with_a_small_context_says_so() {
    let dir = TempDir::new("providers-use-small-context");
    let server = FakeProvider::start_with(
        StubCapability {
            models: json!({ "data": [{ "id": "small", "context_length": 2048 }] }),
            ..StubCapability::default()
        },
        [],
    );
    dir.write_config(&format!(
        "[app]\nprovider = \"deepseek\"\n\n\
         [providers.mine]\nendpoint = \"{}\"\n",
        server.base_url()
    ));

    let output = dir.plainly(&["providers", "use", "mine", "small"]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let message = stderr(&output);
    assert!(
        message.contains("small"),
        "the choice is confirmed: {message}"
    );
    assert!(
        message.contains("2048") && message.contains("cut off"),
        "and its context is named, not silently accepted: {message}"
    );
}

/// The listing judges a context against the budget the provider is configured
/// for: the detailed mode's budget is several times the default's (spec §7), so
/// the same model fits one and is flagged under the other.
#[test]
fn the_listing_judges_a_context_against_the_configured_budget() {
    let dir = TempDir::new("providers-discover-detailed-budget");
    let server = FakeProvider::start_with(
        StubCapability {
            models: json!({ "data": [{ "id": "mid", "context_length": 4096 }] }),
            ..StubCapability::default()
        },
        [],
    );
    let config = format!(
        "[app]\nprovider = \"mine\"\n\n\
         [providers.mine]\nendpoint = \"{}\"\nmodel = \"mid\"\n",
        server.base_url()
    );

    let header = format!("mine at {}", server.base_url());

    // Thinking off: 4096 holds a Passage, so nothing is flagged.
    dir.write_config(&config);
    let off = dir.plainly(&["providers", "discover"]);
    assert_eq!(code(&off), SUCCESS, "{}", stderr(&off));
    let listing = stdout(&off);
    assert!(!block(&listing, &header).contains("too small"), "{listing}");

    // Thinking on: the same model is below the detailed mode's budget.
    dir.write_config(&format!("{config}thinking = \"on\"\n"));
    let on = dir.plainly(&["providers", "discover"]);
    assert_eq!(code(&on), SUCCESS, "{}", stderr(&on));
    let listing = stdout(&on);
    assert!(
        block(&listing, &header).contains("too small"),
        "the detailed mode's budget is not the default's: {listing}"
    );
}

/// Where a context too small is raised is the runtime's own business, and for
/// Ollama it is not a request flag at all: `num_ctx` lives in the model's
/// Modelfile or in `OLLAMA_CONTEXT_LENGTH` (spec §7). That is the part of
/// ticket 05's handoff a selection can act on.
#[test]
fn selecting_a_small_model_from_ollama_names_the_environment_not_a_flag() {
    let dir = TempDir::new("providers-use-ollama-context");
    let server = FakeProvider::start_with(
        StubCapability {
            models: json!({ "data": [{ "id": "qwen3.5:2b", "context_length": 2048 }] }),
            ..StubCapability::default()
        },
        [],
    );
    dir.write_config(&format!(
        "[app]\nprovider = \"ollama\"\n\n\
         [providers.ollama]\nendpoint = \"http://{}/v1\"\n",
        server.authority().trim_start_matches("http://")
    ));

    let output = dir.plainly(&["providers", "use", "ollama", "qwen3.5:2b"]);

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    let message = stderr(&output);
    assert!(
        message.contains("2048"),
        "the context it reports: {message}"
    );
    assert!(
        message.contains("OLLAMA_CONTEXT_LENGTH"),
        "where an Ollama context is raised: {message}"
    );
}

/// A loopback runtime started with a key — which spec §7 asks of the llama.cpp
/// sidecar — is still discovered: the scan sends whatever key the candidate's
/// own name resolves to, so the setup this project recommends is not the one
/// discovery cannot see.
#[test]
fn discovery_sends_the_key_a_candidates_name_resolves_to() {
    let dir = TempDir::new("providers-discover-keyed");
    let server = FakeProvider::start_with(
        StubCapability {
            token: Some("local-key".to_string()),
            ..StubCapability::default()
        },
        [],
    );
    dir.write_config(&custom_provider(&server.base_url()));

    // Without the key the runtime refuses, and nothing about it is listed.
    let without = dir.plainly(&["providers", "discover"]);
    assert!(
        !stdout(&without).contains(&server.base_url()),
        "a keyed runtime is not listed without its key: {}",
        stdout(&without)
    );

    // With the key exported for that provider name, it is.
    let with = dir.plainly_with(
        &["providers", "discover"],
        "",
        &[("PLAINLY_STUB_API_KEY", "local-key")],
    );
    assert_eq!(code(&with), SUCCESS, "{}", stderr(&with));
    assert!(
        stdout(&with).contains(&server.base_url()),
        "{}",
        stdout(&with)
    );
    assert!(stdout(&with).contains("stub-model"), "{}", stdout(&with));
}

/// A runtime with a large catalogue is not pasted into the terminal whole: a
/// message about what it serves names a bounded number of ids.
#[test]
fn a_model_list_too_long_to_quote_is_cut_short() {
    let dir = TempDir::new("providers-use-long-list");
    let ids: Vec<Value> = (0..12)
        .map(|index| json!({ "id": format!("model-{index}") }))
        .collect();
    let server = FakeProvider::start_with(
        StubCapability {
            models: json!({ "data": ids }),
            ..StubCapability::default()
        },
        [],
    );
    dir.write_config(&format!(
        "[app]\nprovider = \"deepseek\"\n\n\
         [providers.mine]\nendpoint = \"{}\"\n",
        server.base_url()
    ));

    let output = dir.plainly(&["providers", "use", "mine", "nosuch"]);

    assert_eq!(code(&output), USAGE);
    let message = stderr(&output);
    assert!(message.contains("model-0"), "{message}");
    assert!(
        message.contains("and 2 more"),
        "the count is what matters: {message}"
    );
    assert!(
        !message.contains("model-11"),
        "the tail of a catalogue is not quoted: {message}"
    );
}

/// A runtime that is there and refuses the keyless scan is not "nothing
/// answered": the report names what it said and the key a name would have, so
/// the fix it offers is one that can work (tickets/06).
#[test]
fn a_runtime_that_asks_for_a_key_is_reported_as_a_refusal_not_as_absence() {
    let dir = TempDir::new("providers-refused-scan");
    let server = FakeProvider::start_with(
        StubCapability {
            token: Some("local-key".to_string()),
            ..StubCapability::default()
        },
        [],
    );
    dir.write_config(&format!(
        "[app]\nprovider = \"deepseek\"\n\n\
         [providers.mine]\nendpoint = \"{}\"\n",
        server.base_url()
    ));

    let output = dir.plainly(&["providers", "use", "mine", "chosen"]);

    assert_eq!(code(&output), NOT_CONFIGURED);
    let message = stderr(&output);
    assert!(
        message.contains(&server.base_url()),
        "the endpoint that refused is named: {message}"
    );
    assert!(message.contains("401"), "with what it said: {message}");
    assert!(
        message.contains("PLAINLY_MINE_API_KEY"),
        "and the key a run would look for: {message}"
    );

    // The listing says the same thing about the same runtime, whether or not
    // anything else on the machine answered.
    let discovered = dir.plainly(&["providers", "discover"]);
    assert!(
        stderr(&discovered).contains("PLAINLY_MINE_API_KEY"),
        "a refusal is worth a line even when other runtimes answered: {}",
        stderr(&discovered)
    );
}

/// A refusal that is not about a key does not offer one: a 404 means this is not
/// a model server where the name points, and the endpoint's own words are the
/// whole advice (tickets/06).
#[test]
fn a_refusal_that_is_not_a_credential_rejection_does_not_name_a_key() {
    let dir = TempDir::new("providers-refused-404");
    let server = FakeProvider::start_with(
        StubCapability {
            models_status: Some(404),
            ..StubCapability::default()
        },
        [],
    );
    dir.write_config(&format!(
        "[app]\nprovider = \"deepseek\"\n\n\
         [providers.mine]\nendpoint = \"{}\"\n",
        server.base_url()
    ));

    let output = dir.plainly(&["providers", "use", "mine", "chosen"]);

    assert_eq!(code(&output), NOT_CONFIGURED);
    let message = stderr(&output);
    assert!(message.contains("404"), "what it said: {message}");
    assert!(
        !message.contains("PLAINLY_MINE_API_KEY"),
        "a 404 is not fixed by a key: {message}"
    );
}
