//! The wire request and the answer: what the endpoint is sent, and where the
//! model's content is read back from.

mod support;

use serde_json::{Value, json};

use plainly_core::{
    ChatCompletions, Config, EndpointModel, ExplainRequest, MAX_TOKENS, MAX_TOKENS_THINKING,
    ProbeRequest, ProviderErrorKind, ProviderSetup, SchemaSupport, Thinking, ThinkingSwitch,
    wire_schema,
};

/// A client for `name`, with `model` overriding the preset's when given.
fn client(name: &str, model: Option<&str>, api_key: &str) -> ChatCompletions {
    let mut text = String::from("[app]\n");
    if model.is_some() {
        text.push_str(&format!("[providers.{name}]\n"));
        if let Some(model) = model {
            text.push_str(&format!("model = \"{model}\"\n"));
        }
    }
    let config = Config::parse(&text).expect("the fixture is valid TOML");
    let setup = ProviderSetup::resolve(name, &config).expect("the fixture provider resolves");
    ChatCompletions::new(setup, api_key)
}

/// The body for `name` with the standard fixture request.
fn body(name: &str, model: Option<&str>) -> Value {
    client(name, model, "sk-test").request_body(&support::request(support::PASSAGE))
}

/// A client for `name` whose thinking choice is `on`.
fn thinking_client(name: &str) -> ChatCompletions {
    let config = Config::parse(&format!("[providers.{name}]\nthinking = \"on\"\n"))
        .expect("the fixture is valid TOML");
    let setup = ProviderSetup::resolve(name, &config).expect("the fixture provider resolves");
    ChatCompletions::new(setup, "sk-test")
}

/// The same, with the user's thinking choice set to `on`.
fn body_with_thinking_on(name: &str) -> Value {
    thinking_client(name).request_body(&support::request(support::PASSAGE))
}

/// A client to read answers with. Which provider it is does not matter to
/// `answer_from` beyond the budget it carries.
fn reader() -> ChatCompletions {
    client("deepseek", None, "sk-test")
}

/// The request as the standard fixture builds it, for assertions that need it.
fn request() -> ExplainRequest {
    support::request(support::PASSAGE)
}

#[test]
fn a_schema_enforcing_endpoint_gets_the_schema_nested_where_it_is_read() {
    let body = body("llamacpp", Some("qwen3.5-4b"));

    assert_eq!(body["response_format"]["type"], "json_schema");
    assert_eq!(
        body["response_format"]["json_schema"]["name"],
        "explanation"
    );
    assert_eq!(body["response_format"]["json_schema"]["strict"], true);
    assert_eq!(
        body["response_format"]["json_schema"]["schema"],
        wire_schema(),
        "the schema on the wire is the one the answer is checked against"
    );
    // The flat form llama.cpp's own README documents is silently ignored by the
    // server, so it must never be what we send.
    assert!(
        body["response_format"].get("schema").is_none(),
        "the schema must not sit flat under response_format"
    );
}

#[test]
fn an_endpoint_that_cannot_enforce_a_schema_is_asked_for_json() {
    let body = body("deepseek", None);

    assert_eq!(body["response_format"], json!({ "type": "json_object" }));
    assert!(
        body["response_format"].get("json_schema").is_none(),
        "DeepSeek rejects response_format.type json_schema"
    );
}

#[test]
fn thinking_off_is_spelled_the_canonical_way() {
    let body = body("deepseek", None);

    assert_eq!(body["thinking"], json!({ "type": "disabled" }));
    // The near misses are not equivalent: {"thinking":false} is a 422 and
    // {"enable_thinking":false} is accepted but does nothing.
    assert_ne!(body["thinking"], json!(false));
    assert!(body.get("enable_thinking").is_none());
}

#[test]
fn a_provider_without_a_thinking_switch_is_not_sent_one() {
    let body = body("openai", None);

    assert!(body.get("thinking").is_none(), "{body}");
}

#[test]
fn thinking_on_is_left_to_the_provider() {
    // The switch exists to turn thinking *off*; a provider that reasons by
    // default reasons when the field is absent.
    let body = body_with_thinking_on("deepseek");

    assert!(body.get("thinking").is_none(), "{body}");
}

#[test]
fn the_request_carries_the_budget_and_the_prompt() {
    let body = body("deepseek", None);
    let request = request();

    assert_eq!(body["temperature"], 0);
    assert_eq!(body["max_tokens"], MAX_TOKENS);

    assert_eq!(body["model"], "deepseek-flash");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], request.system_prompt);
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], request.passage);
}

#[test]
fn the_detailed_mode_is_given_a_bigger_budget() {
    // A reasoning model spends most of its completion before the answer exists,
    // so the ordinary budget would truncate exactly the Passages the detailed
    // mode is for (spec §7). That the detailed budget is the larger of the two
    // is a const assertion in `chat`, next to the two numbers.
    assert_eq!(body("deepseek", None)["max_tokens"], MAX_TOKENS);
    assert_eq!(
        body_with_thinking_on("deepseek")["max_tokens"],
        MAX_TOKENS_THINKING
    );
}

#[test]
fn the_answer_is_the_content_of_the_first_choice() {
    let body = json!({
        "choices": [{ "message": { "role": "assistant", "content": "{\"ok\":true}" } }]
    });

    assert_eq!(
        reader().answer_from(&body).expect("the answer is there"),
        "{\"ok\":true}"
    );
}

#[test]
fn an_empty_content_falls_back_to_the_reasoning_field() {
    // The documented LM Studio routing bug with reasoning models: the
    // constrained JSON lands in `reasoning_content` and `content` is empty.
    let body = json!({
        "choices": [{
            "message": { "content": "", "reasoning_content": "{\"ok\":true}" },
            "finish_reason": "stop"
        }]
    });

    assert_eq!(
        reader().answer_from(&body).expect("the fallback finds it"),
        "{\"ok\":true}"
    );
}

#[test]
fn a_chain_of_thought_is_not_mistaken_for_an_answer() {
    // A reasoning model that thought in `reasoning_content` and then stopped
    // produced no answer. Handing the prose to the contract check would report
    // it as a malformed answer and spend the retries on it (tickets/04), so the
    // fallback above is limited to the JSON object the routing bug leaves.
    let body = json!({
        "choices": [{
            "message": {
                "content": "",
                "reasoning_content": "The user wants a paraphrase. First I should find the blockers..."
            },
            "finish_reason": "stop"
        }]
    });

    let error = reader().answer_from(&body).unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Empty);
    assert!(error.to_string().contains("reasoning"), "{error}");
}

#[test]
fn a_refusal_is_a_failure_and_not_an_answer_to_parse() {
    // A refusal does not follow the schema, so parsing it would report a
    // contract failure for what is actually the model declining.
    let body = json!({
        "choices": [{
            "message": { "content": null, "refusal": "I can't help with that." },
            "finish_reason": "stop"
        }]
    });

    let error = reader().answer_from(&body).unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Refused);
    assert!(error.to_string().contains("I can't help"), "{error}");
}

#[test]
fn a_refusal_in_the_array_form_of_content_is_a_failure_too() {
    let body = json!({
        "choices": [{
            "message": { "content": [{ "type": "refusal", "refusal": "No." }] }
        }]
    });

    let error = reader().answer_from(&body).unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Refused);
}

#[test]
fn an_unlabelled_cut_off_answer_is_still_a_truncation() {
    // Ollama's OpenAI-compatible layer reports a length cut-off as a null
    // finish_reason, and what is left is a JSON prefix. Left to the contract
    // check it would look like a badly answered question, and the policy would
    // spend its retries on the same exhausted budget (spec §7).
    let body = json!({
        "choices": [{
            "message": { "content": "{\"comprehensible\": \"the commit" },
            "finish_reason": null
        }]
    });

    let error = reader().answer_from(&body).unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Truncated);
    assert!(error.to_string().contains("max_tokens = 2048"), "{error}");
}

#[test]
fn a_null_finish_reason_on_a_whole_answer_is_not_a_truncation() {
    let body = json!({
        "choices": [{
            "message": { "content": "{\"ok\":true}" },
            "finish_reason": null
        }]
    });

    assert_eq!(
        reader().answer_from(&body).expect("the answer is whole"),
        "{\"ok\":true}"
    );
}

#[test]
fn a_transport_failure_is_classified_by_whether_resending_could_work() {
    use std::io::{Error as IoError, ErrorKind};

    let client = reader();
    let io = |kind| ureq::Error::Io(IoError::new(kind, "socket trouble"));

    for kind in [
        // A local runtime that is not listening yet is the reason a refusal is
        // on this list rather than with the permanent failures.
        ErrorKind::ConnectionRefused,
        ErrorKind::ConnectionReset,
        ErrorKind::ConnectionAborted,
        ErrorKind::TimedOut,
        ErrorKind::Interrupted,
        // The stream ended before the response did.
        ErrorKind::UnexpectedEof,
    ] {
        assert_eq!(
            client.transport_error(io(kind)).kind(),
            ProviderErrorKind::Unavailable,
            "{kind:?} is worth one more ask"
        );
    }

    // A name that does not resolve arrives as an `Io` error from ureq's
    // resolver (`to_socket_addrs`), not as `HostNotFound` — so it is the io
    // kind that has to catch it, or a typo in the endpoint costs the whole
    // backoff schedule and then says "try again later".
    assert_eq!(
        client.transport_error(io(ErrorKind::Other)).kind(),
        ProviderErrorKind::Misconfigured
    );
    assert_eq!(
        client.transport_error(ureq::Error::HostNotFound).kind(),
        ProviderErrorKind::Misconfigured
    );
    assert_eq!(
        client
            .transport_error(ureq::Error::BadUri("not a url".to_string()))
            .kind(),
        ProviderErrorKind::Misconfigured
    );
    assert_eq!(
        client.transport_error(ureq::Error::ConnectionFailed).kind(),
        ProviderErrorKind::Unavailable
    );
}

#[test]
fn a_cut_off_answer_names_the_budget_that_cut_it_off() {
    // Truncation is a budget problem, not a model one: reporting it as
    // malformed JSON would send the retry policy after the wrong thing. The
    // budget is named because the two modes do not share one.
    let body = json!({
        "choices": [{
            "message": { "content": "{\"comprehensible\": \"the commit" },
            "finish_reason": "length"
        }]
    });

    let error = thinking_client("deepseek").answer_from(&body).unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Truncated);
    assert!(error.to_string().contains("max_tokens = 8192"), "{error}");
}

#[test]
fn an_answer_with_nothing_in_it_is_a_failure() {
    // DeepSeek's docs admit occasional empty content; there is no answer to
    // check against the contract.
    let body = json!({
        "choices": [{ "message": { "content": "   " }, "finish_reason": "stop" }]
    });

    let error = reader().answer_from(&body).unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Empty);
    assert!(error.to_string().contains("no content"), "{error}");
}

#[test]
fn an_incomplete_response_says_so_instead_of_being_parsed_as_a_prefix() {
    // OpenAI marks an answer it did not finish with a top-level status; the
    // content, if any, is a prefix. Reported as incomplete rather than as
    // malformed JSON, which would send the retry policy after the wrong thing.
    let body = json!({
        "status": "incomplete",
        "incomplete_details": { "reason": "max_output_tokens" },
        "choices": []
    });

    let error = reader().answer_from(&body).unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Truncated);
    assert!(error.to_string().contains("incomplete"), "{error}");
    assert!(error.to_string().contains("max_output_tokens"), "{error}");
}

#[test]
fn a_status_the_endpoint_refused_is_classified_for_the_policy() {
    let client = reader();

    // 400/422 on a request whose shape we chose: the shape is the problem, so
    // the answer is a downgrade and a fresh probe rather than another call.
    assert_eq!(
        client
            .http_error(400, "This response_format type is unavailable now")
            .kind(),
        ProviderErrorKind::UnsupportedParameter
    );
    assert_eq!(
        client
            .http_error(422, "thinking.type must be an object")
            .kind(),
        ProviderErrorKind::UnsupportedParameter
    );

    // Transient: the backoff exists for these.
    for status in [408, 429, 500, 502, 503] {
        assert_eq!(
            client.http_error(status, "later").kind(),
            ProviderErrorKind::Unavailable,
            "HTTP {status}"
        );
    }

    // Asking again cannot help: the configuration is what has to change.
    for status in [401, 403, 404, 413] {
        assert_eq!(
            client.http_error(status, "no").kind(),
            ProviderErrorKind::Misconfigured,
            "HTTP {status}"
        );
    }
}

#[test]
fn a_failed_response_body_is_quoted_with_its_key_removed() {
    // Not `reader()`: the key this test asserts about is the fixture.
    let client = client("deepseek", None, "sk-secret-0123456789abcdef");

    let error = client.http_error(401, "Your api key: sk-secret-0123456789abcdef is invalid");

    let message = error.to_string();
    assert!(!message.contains("sk-secret-0123456789abcdef"), "{message}");
    assert!(message.contains("<redacted>"), "{message}");
    assert!(message.contains("401"), "{message}");
}

/// A client for Ollama, whose preset serves a surface of its own. The model is
/// the user's to name, as it is for every local runtime.
fn ollama(model: &str) -> ChatCompletions {
    let config = Config::parse(&format!("[providers.ollama]\nmodel = \"{model}\"\n"))
        .expect("the fixture is valid TOML");
    let setup = ProviderSetup::resolve("ollama", &config).expect("the fixture provider resolves");
    ChatCompletions::new(setup, "sk-test")
}

#[test]
fn ollama_is_asked_on_its_own_route_with_format() {
    // The native route is where Ollama's structured output lives: `format` takes
    // the schema itself, and `stream`/`options` replace the OpenAI top-level
    // fields (spec §7).
    let body = ollama("qwen3.5:4b").native_request_body(&support::request(support::PASSAGE));

    assert_eq!(body["model"], "qwen3.5:4b");
    assert_eq!(body["stream"], false);
    assert_eq!(body["format"], wire_schema());
    assert_eq!(body["options"]["temperature"], 0);
    assert_eq!(body["options"]["num_predict"], MAX_TOKENS);
    assert_eq!(body["messages"][1]["content"], support::PASSAGE);
    assert!(
        body.get("response_format").is_none(),
        "the compatibility layer's field does not belong on the native route: {body}"
    );
    assert!(
        body.get("thinking").is_none(),
        "below 0.31.2 a think field silently disables format, and the version \
         floor belongs to the local discovery path (tickets/06)"
    );
}

#[test]
fn ollama_without_a_schema_is_asked_for_json_by_name() {
    let setup = client_setup("ollama", "[providers.ollama]\nmodel = \"m\"\n");
    let downgraded = ChatCompletions::new(
        setup.with_capability(SchemaSupport::BestEffort, ThinkingSwitch::Unsupported),
        "sk-test",
    );

    assert_eq!(
        downgraded.native_request_body(&support::request(support::PASSAGE))["format"],
        json!("json"),
        "'json' is Ollama's own spelling of the weaker tier"
    );
}

#[test]
fn an_ollama_native_answer_is_read_out_of_its_own_envelope() {
    let body = json!({
        "model": "qwen3.5:4b",
        "message": { "role": "assistant", "content": "{\"ok\":true}" },
        "done": true,
        "done_reason": "stop"
    });

    assert_eq!(
        ollama("qwen3.5:4b")
            .native_answer(&body)
            .expect("the answer is there"),
        "{\"ok\":true}"
    );
}

#[test]
fn an_ollama_cut_off_answer_names_its_own_budget_field() {
    let body = json!({
        "message": { "content": "{\"comprehensible\": \"the commit" },
        "done": true,
        "done_reason": "length"
    });

    let error = ollama("qwen3.5:4b").native_answer(&body).unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Truncated);
    assert!(error.to_string().contains("num_predict"), "{error}");
}

#[test]
fn an_ollama_message_with_only_a_chain_of_thought_is_an_empty_answer() {
    let body = json!({
        "message": { "content": "", "thinking": "The user wants a paraphrase…" },
        "done": true,
        "done_reason": "stop"
    });

    let error = ollama("qwen3.5:4b").native_answer(&body).unwrap_err();
    assert_eq!(error.kind(), ProviderErrorKind::Empty);
}

#[test]
fn a_probe_asks_only_for_the_shape_it_is_testing() {
    let client = reader();

    let schema_probe = client.probe_request_body(&ProbeRequest {
        schema: SchemaSupport::Enforced,
        disable_thinking: false,
    });
    assert_eq!(schema_probe["response_format"]["type"], "json_schema");
    assert_eq!(
        schema_probe["response_format"]["json_schema"]["schema"],
        wire_schema()
    );
    assert!(schema_probe.get("thinking").is_none(), "{schema_probe}");
    assert_eq!(
        schema_probe["messages"][1]["content"],
        plainly_core::probe::PROBE_PASSAGE
    );

    let json_probe = client.probe_request_body(&ProbeRequest {
        schema: SchemaSupport::BestEffort,
        disable_thinking: true,
    });
    assert_eq!(
        json_probe["response_format"],
        json!({ "type": "json_object" })
    );
    assert_eq!(json_probe["thinking"], json!({ "type": "disabled" }));
}

#[test]
fn a_native_probe_uses_ollama_spellings_on_ollama_route() {
    let client = ollama("qwen3.5:4b");

    let enforced = client.native_probe_body(&ProbeRequest {
        schema: SchemaSupport::Enforced,
        disable_thinking: false,
    });
    assert_eq!(enforced["format"], wire_schema());

    let best_effort = client.native_probe_body(&ProbeRequest {
        schema: SchemaSupport::BestEffort,
        disable_thinking: true,
    });
    assert_eq!(best_effort["format"], json!("json"));
}

/// A setup resolved by name, for tests that need to change its capability.
fn client_setup(name: &str, text: &str) -> ProviderSetup {
    let config = Config::parse(text).expect("the fixture is valid TOML");
    ProviderSetup::resolve(name, &config).expect("the fixture provider resolves")
}

#[test]
fn a_model_list_is_read_under_the_names_the_runtimes_use() {
    // The OpenAI shape as LM Studio and llama.cpp print it…
    let openai = json!({
        "data": [
            { "id": "qwen3.5-4b", "loaded": true, "max_context_length": 16384 },
            { "id": "cold-model", "state": "not-loaded" }
        ]
    });
    assert_eq!(
        plainly_core::chat::models_from(&openai),
        Some(vec![
            EndpointModel {
                id: "qwen3.5-4b".to_string(),
                loaded: Some(true),
                context_length: Some(16384),
            },
            EndpointModel {
                id: "cold-model".to_string(),
                loaded: Some(false),
                context_length: None,
            },
        ])
    );

    // …and the shape a runtime that volunteers a nested `meta` uses.
    let nested = json!({ "models": [{ "name": "m", "meta": { "n_ctx": 4096 } }] });
    assert_eq!(
        plainly_core::chat::models_from(&nested),
        Some(vec![EndpointModel {
            id: "m".to_string(),
            loaded: None,
            context_length: Some(4096),
        }])
    );
}

#[test]
fn a_model_entry_without_an_id_is_not_a_model() {
    let body = json!({ "data": [{ "object": "model" }, { "id": "real" }] });

    assert_eq!(
        plainly_core::chat::models_from(&body),
        Some(vec![EndpointModel {
            id: "real".to_string(),
            loaded: None,
            context_length: None,
        }])
    );
}

/// A body with no list in it is not an endpoint that serves nothing: the two
/// are cached and reported differently.
#[test]
fn a_model_list_that_is_not_a_list_is_not_an_empty_one() {
    assert_eq!(
        plainly_core::chat::models_from(&json!({ "error": "nope" })),
        None
    );
    assert_eq!(
        plainly_core::chat::models_from(&json!({ "data": [] })),
        Some(Vec::new()),
        "an endpoint that lists nothing listed nothing"
    );
}

/// The llama.cpp router's own shape, taken from what the runtime on this machine
/// answers: the state is under `status.value`, and the context length is only in
/// the model's launch argv. Reading it is what makes "is this model big enough"
/// answerable without loading it (tickets/06).
#[test]
fn a_routers_model_reports_its_state_and_context_in_the_launch_argv() {
    let body = json!({
        "data": [{
            "id": "unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M",
            "object": "model",
            "owned_by": "llamacpp",
            "status": {
                "value": "unloaded",
                "args": [
                    "/nix/store/…/bin/llama-server",
                    "--host", "127.0.0.1",
                    "--port", "0",
                    "--alias", "unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M",
                    "--ctx-size", "16384",
                    "--n-gpu-layers", "99",
                ],
            },
        }]
    });

    assert_eq!(
        plainly_core::chat::models_from(&body),
        Some(vec![EndpointModel {
            id: "unsloth/Qwen3-4B-Instruct-2507-GGUF:Q4_K_M".to_string(),
            loaded: Some(false),
            context_length: Some(16384),
        }])
    );

    // The same runtime once the model is up, and the `--flag=value` spelling a
    // runtime is equally free to use.
    let loaded = json!({
        "data": [{
            "id": "m",
            "status": { "value": "loaded", "args": ["llama-server", "--ctx-size=4096"] }
        }]
    });
    assert_eq!(
        plainly_core::chat::models_from(&loaded),
        Some(vec![EndpointModel {
            id: "m".to_string(),
            loaded: Some(true),
            context_length: Some(4096),
        }])
    );
}

/// Reading a context length before loading the model is the point: the number
/// alone decides whether one Passage and its answer fit — and the answer's
/// budget is not the same in both modes (spec §7), so neither is the verdict.
#[test]
fn a_context_is_judged_against_the_budget_the_mode_actually_uses() {
    let model = |context: Option<u64>| EndpointModel {
        id: "m".to_string(),
        loaded: None,
        context_length: context,
    };

    // The default setting: the prompt plus MAX_TOKENS.
    assert_eq!(
        plainly_core::context_fits(&model(Some(16384)), Thinking::Off),
        Some(true)
    );
    assert_eq!(
        plainly_core::context_fits(&model(Some(4096)), Thinking::Off),
        Some(true)
    );
    assert_eq!(
        plainly_core::context_fits(&model(Some(2048)), Thinking::Off),
        Some(false)
    );

    // The detailed mode reasons before it answers and is given several times the
    // budget, so a context that fits one mode does not fit the other.
    assert_eq!(
        plainly_core::context_fits(&model(Some(4096)), Thinking::On),
        Some(false)
    );
    assert_eq!(
        plainly_core::context_fits(&model(Some(16384)), Thinking::On),
        Some(true)
    );
    assert!(
        plainly_core::min_context_length(Thinking::On)
            > plainly_core::min_context_length(Thinking::Off),
        "the detailed mode is not held to the default's number"
    );

    assert_eq!(
        plainly_core::context_fits(&model(None), Thinking::Off),
        None,
        "a runtime that reports nothing has not answered the question"
    );
}
