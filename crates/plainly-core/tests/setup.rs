//! Resolving a provider name into the thing a request is sent to: the shipped
//! preset merged with the user's configuration.

use plainly_core::{
    Config, KeyRequirement, ProviderSetup, SchemaSupport, SetupError, Surface, Thinking,
    ThinkingSwitch,
};

fn config(text: &str) -> Config {
    Config::parse(text).expect("the fixture is valid TOML")
}

#[test]
fn the_default_provider_is_the_shipped_deepseek_preset() {
    let setup =
        ProviderSetup::resolve("deepseek", &Config::default()).expect("the preset resolves");

    assert_eq!(setup.label, "DeepSeek");
    assert_eq!(setup.endpoint, "https://api.deepseek.com/v1");
    assert_eq!(setup.model, "deepseek-flash");
    // Thinking off by default, which is the whole point of the preset: about a
    // 10x latency improvement over the provider's own default (spec §7).
    assert_eq!(setup.thinking, Thinking::Off);
    assert_eq!(setup.schema, SchemaSupport::BestEffort);
    assert_eq!(setup.thinking_switch, ThinkingSwitch::Canonical);
    assert_eq!(setup.key, KeyRequirement::Required);
}

#[test]
fn every_shipped_preset_names_a_cloud_model_or_asks_for_one() {
    for name in ["deepseek", "openai", "llamacpp", "ollama", "lmstudio"] {
        let result = ProviderSetup::resolve(name, &Config::default());
        match name {
            // The cloud presets are usable as they stand.
            "deepseek" | "openai" => {
                let setup = result.expect("a cloud preset resolves on its own");
                assert!(!setup.model.is_empty(), "{name} has a model");
            }
            // A local runtime serves whatever the user has, so the model is
            // theirs to name; tickets/06 fills it in from the probe.
            _ => {
                let error = result.expect_err("a local preset asks for a model first");
                assert!(
                    matches!(error, SetupError::NoModel { .. }),
                    "{name}: got {error:?}"
                );
                assert!(
                    error
                        .to_string()
                        .contains(&format!("providers.{name}.model")),
                    "{name}: {error}"
                );
            }
        }
    }
}

#[test]
fn the_configuration_wins_over_the_preset_field_by_field() {
    let setup = ProviderSetup::resolve(
        "deepseek",
        &config(
            r#"
            [app]
            provider = "deepseek"

            [providers.deepseek]
            endpoint = "https://api.deepseek.com/v2/"
            model = "a-newer-deepseek"
            thinking = "on"
            "#,
        ),
    )
    .expect("the configured provider resolves");

    // The trailing slash is trimmed so the path is joined once, not twice.
    assert_eq!(setup.endpoint, "https://api.deepseek.com/v2");
    assert_eq!(setup.model, "a-newer-deepseek");
    assert_eq!(setup.thinking, Thinking::On);
    // Same service, different path: what the preset knows still applies.
    assert_eq!(setup.schema, SchemaSupport::BestEffort);
    assert_eq!(setup.thinking_switch, ThinkingSwitch::Canonical);
}

/// The preset describes one service, not a name. Point the name at another
/// machine and its call-surface knowledge is void: `json_object` is a 400 on
/// LM Studio, and the canonical thinking field is a 400 on anything strict.
#[test]
fn an_endpoint_that_names_another_service_voids_what_the_preset_knew() {
    let setup = ProviderSetup::resolve(
        "deepseek",
        &config(
            r#"
            [providers.deepseek]
            endpoint = "http://127.0.0.1:11434/v1"
            "#,
        ),
    )
    .expect("the configured provider resolves");

    // The model is a knob the user did not touch, so the preset's still stands.
    assert_eq!(setup.model, "deepseek-flash");
    // And so is the name they wrote: a 400 from 127.0.0.1 must not be reported
    // as "DeepSeek answered", which would blame a service never contacted.
    assert_eq!(setup.label, "deepseek");
    // Everything else is "we do not know this endpoint yet": ask for a schema,
    // send nothing that might be rejected, demand no key. tickets/05 replaces
    // this with what a probe finds.
    assert_eq!(setup.schema, SchemaSupport::Enforced);
    assert_eq!(setup.thinking_switch, ThinkingSwitch::Unsupported);
    assert_eq!(setup.key, KeyRequirement::Optional);
}

#[test]
fn writing_the_preset_endpoint_out_in_full_changes_nothing() {
    let setup = ProviderSetup::resolve(
        "deepseek",
        &config(
            r#"
            [providers.deepseek]
            endpoint = "https://api.deepseek.com/v1"
            "#,
        ),
    )
    .expect("the configured provider resolves");

    assert_eq!(setup.schema, SchemaSupport::BestEffort);
    assert_eq!(setup.thinking_switch, ThinkingSwitch::Canonical);
    assert_eq!(setup.key, KeyRequirement::Required);
    assert_eq!(setup.label, "DeepSeek");
}

#[test]
fn a_custom_provider_is_whatever_the_configuration_says() {
    let setup = ProviderSetup::resolve(
        "mine",
        &config(
            r#"
            [app]
            provider = "mine"

            [providers.mine]
            endpoint = "http://127.0.0.1:11434/v1"
            model = "some-model"
            "#,
        ),
    )
    .expect("a configured provider resolves");

    assert_eq!(setup.label, "mine");
    assert_eq!(setup.endpoint, "http://127.0.0.1:11434/v1");
    assert_eq!(setup.model, "some-model");
    assert_eq!(setup.thinking, Thinking::Off);
    // Nothing is known about a name Plainly does not ship, so nothing that might
    // be rejected is sent: a schema is asked for (correcting that is the probe's
    // job), no thinking field, and no key is demanded.
    assert_eq!(setup.schema, SchemaSupport::Enforced);
    assert_eq!(setup.thinking_switch, ThinkingSwitch::Unsupported);
    assert_eq!(setup.key, KeyRequirement::Optional);
}

#[test]
fn an_empty_configured_value_falls_back_to_the_preset() {
    let setup = ProviderSetup::resolve(
        "deepseek",
        &config(
            r#"
            [providers.deepseek]
            endpoint = ""
            model = "   "
            "#,
        ),
    )
    .expect("an empty value is not a value");

    assert_eq!(setup.endpoint, "https://api.deepseek.com/v1");
    assert_eq!(setup.model, "deepseek-flash");
}

#[test]
fn a_name_nobody_configured_is_an_error_that_lists_what_can_be_chosen() {
    let error = ProviderSetup::resolve("deapseak", &Config::default()).unwrap_err();

    assert!(matches!(error, SetupError::Unknown { .. }), "got {error:?}");
    let message = error.to_string();
    for name in ["deepseek", "lmstudio"] {
        assert!(message.contains(name), "{message}");
    }
}

#[test]
fn a_custom_provider_without_an_endpoint_says_there_is_nowhere_to_send_to() {
    let error = ProviderSetup::resolve(
        "mine",
        &config(
            r#"
            [providers.mine]
            model = "some-model"
            "#,
        ),
    )
    .unwrap_err();

    assert!(
        matches!(error, SetupError::NoEndpoint { .. }),
        "got {error:?}"
    );
    assert!(error.to_string().contains("[providers.mine]"), "{error}");
}

#[test]
fn ollama_is_resolved_to_its_own_route_and_everyone_else_to_the_shared_one() {
    let ollama = ProviderSetup::resolve(
        "ollama",
        &config("[providers.ollama]\nmodel = \"qwen3.5:4b\"\n"),
    )
    .expect("the fixture resolves");
    assert_eq!(ollama.surface, Surface::Ollama);

    let deepseek =
        ProviderSetup::resolve("deepseek", &Config::default()).expect("the preset resolves");
    assert_eq!(deepseek.surface, Surface::OpenAi);
}

#[test]
fn the_surface_follows_the_provider_name_rather_than_the_endpoint() {
    // The surface is how Plainly talks to this provider, not a claim about what
    // the endpoint accepts: someone running Ollama on another host still wants
    // `/api/chat`, and an endpoint that does not serve it is caught by the
    // compatibility fallback in `chat` rather than by guessing from a host name.
    let setup = ProviderSetup::resolve(
        "ollama",
        &config("[providers.ollama]\nmodel = \"m\"\nendpoint = \"http://127.0.0.1:9999/v1\"\n"),
    )
    .expect("the fixture resolves");

    assert_eq!(setup.surface, Surface::Ollama);
    // What the preset knew about the *call surface* is still withdrawn: the
    // schema and the switch are properties of the vendor's own endpoint.
    assert_eq!(
        ProviderSetup::resolve(
            "deepseek",
            &config("[providers.deepseek]\nendpoint = \"http://127.0.0.1:9999/v1\"\n"),
        )
        .expect("the fixture resolves")
        .thinking_switch,
        ThinkingSwitch::Unsupported
    );
}

#[test]
fn a_capability_replaces_only_what_a_probe_can_know() {
    let setup =
        ProviderSetup::resolve("deepseek", &Config::default()).expect("the preset resolves");

    let probed = setup.with_capability(SchemaSupport::Enforced, ThinkingSwitch::Unsupported);

    assert_eq!(probed.schema, SchemaSupport::Enforced);
    assert_eq!(probed.thinking_switch, ThinkingSwitch::Unsupported);
    // Everything a probe has no say in is carried over untouched: the model and
    // the endpoint are the user's, and the profile still names the vendor.
    assert_eq!(probed.name, setup.name);
    assert_eq!(probed.label, setup.label);
    assert_eq!(probed.endpoint, setup.endpoint);
    assert_eq!(probed.model, setup.model);
    assert_eq!(probed.thinking, setup.thinking);
    assert_eq!(probed.key, setup.key);
}
