//! The wire request and the answer: what the endpoint is sent, and where the
//! model's content is read back from.

mod support;

use serde_json::{Value, json};

use plainly_core::{
    ChatCompletions, Config, ExplainRequest, MAX_TOKENS, ProviderSetup, wire_schema,
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

/// The same, with the user's thinking choice set to `on`.
fn body_with_thinking_on(name: &str) -> Value {
    let config = Config::parse(&format!("[providers.{name}]\nthinking = \"on\"\n"))
        .expect("the fixture is valid TOML");
    let setup = ProviderSetup::resolve(name, &config).expect("the fixture provider resolves");
    ChatCompletions::new(setup, "sk-test").request_body(&support::request(support::PASSAGE))
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
fn the_answer_is_the_content_of_the_first_choice() {
    let body = json!({
        "choices": [{ "message": { "role": "assistant", "content": "{\"ok\":true}" } }]
    });

    assert_eq!(
        ChatCompletions::answer_from(&body).expect("the answer is there"),
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
        ChatCompletions::answer_from(&body).expect("the fallback finds it"),
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
                "reasoning_content": "The user wants a paraphrase. First I should find the blockers…"
            },
            "finish_reason": "stop"
        }]
    });

    let error = ChatCompletions::answer_from(&body).unwrap_err();
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

    let error = ChatCompletions::answer_from(&body).unwrap_err();
    assert!(error.to_string().contains("refused"), "{error}");
    assert!(error.to_string().contains("I can't help"), "{error}");
}

#[test]
fn a_refusal_in_the_array_form_of_content_is_a_failure_too() {
    let body = json!({
        "choices": [{
            "message": { "content": [{ "type": "refusal", "refusal": "No." }] }
        }]
    });

    let error = ChatCompletions::answer_from(&body).unwrap_err();
    assert!(error.to_string().contains("refused"), "{error}");
}

#[test]
fn a_cut_off_answer_says_it_was_cut_off() {
    // Truncation is a budget problem, not a model one: reporting it as
    // malformed JSON would send the retry policy after the wrong thing.
    let body = json!({
        "choices": [{
            "message": { "content": "{\"comprehensible\": \"the commit" },
            "finish_reason": "length"
        }]
    });

    let error = ChatCompletions::answer_from(&body).unwrap_err();
    assert!(error.to_string().contains("cut off"), "{error}");
    assert!(error.to_string().contains("max_tokens"), "{error}");
}

#[test]
fn an_answer_with_nothing_in_it_is_a_failure() {
    // DeepSeek's docs admit occasional empty content; there is no answer to
    // check against the contract.
    let body = json!({
        "choices": [{ "message": { "content": "   " }, "finish_reason": "stop" }]
    });

    let error = ChatCompletions::answer_from(&body).unwrap_err();
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

    let error = ChatCompletions::answer_from(&body).unwrap_err();
    assert!(error.to_string().contains("incomplete"), "{error}");
    assert!(error.to_string().contains("max_output_tokens"), "{error}");
}
