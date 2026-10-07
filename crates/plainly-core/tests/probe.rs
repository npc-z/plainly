//! Capability probing and the downgrade path: what is learned from an endpoint,
//! where it is kept, and what a run does when the endpoint rejects the shape it
//! was sent.
//!
//! The endpoint here is a script rather than a server: a probe is a handful of
//! requests whose *shapes* are the whole question, and a fake that answers "yes"
//! or "no" per shape is what makes the decision reachable without a network.

mod support;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use plainly_core::{
    Cache, Config, Endpoint, EndpointModel, ExplainRequest, Failure, FailureKind, Origin,
    ProbeEndpoint, ProbeRequest, Provider, ProviderError, ProviderSetup, RunFailure, SchemaSupport,
    Surface, Thinking, ThinkingSwitch, Timestamp, explain, probe,
};

use support::TempDir;

/// The instant every fixture is stamped with.
fn now() -> Timestamp {
    Timestamp::from_unix_seconds(1_760_000_000).expect("the fixture instant is in range")
}

/// A provider resolved against a configuration fragment.
fn setup(name: &str, config: &str) -> ProviderSetup {
    let config = Config::parse(config).expect("the fixture is valid TOML");
    ProviderSetup::resolve(name, &config).expect("the fixture provider resolves")
}

/// An endpoint whose answers are a script, remembered per request.
#[derive(Clone)]
struct FakeEndpoint {
    inner: Rc<Inner>,
}

struct Inner {
    probes: RefCell<VecDeque<Result<(), ProviderError>>>,
    answers: RefCell<VecDeque<Result<String, ProviderError>>>,
    models: RefCell<Result<Vec<EndpointModel>, ProviderError>>,
    seen: RefCell<Vec<ProbeRequest>>,
    generated: Cell<usize>,
}

impl FakeEndpoint {
    /// An endpoint that accepts every shape, lists one model and answers with
    /// `answer` any number of times.
    fn accepting() -> Self {
        Self::new()
    }

    fn new() -> Self {
        Self {
            inner: Rc::new(Inner {
                probes: RefCell::new(VecDeque::new()),
                answers: RefCell::new(VecDeque::new()),
                models: RefCell::new(Ok(vec![EndpointModel {
                    id: "some-model".to_string(),
                    loaded: Some(true),
                    context_length: Some(8192),
                }])),
                seen: RefCell::new(Vec::new()),
                generated: Cell::new(0),
            }),
        }
    }

    /// The script a probe walks through, in order.
    fn probes(self, script: impl IntoIterator<Item = Result<(), ProviderError>>) -> Self {
        self.inner.probes.borrow_mut().extend(script);
        self
    }

    /// The script the run itself walks through, in order. Beyond the script a
    /// request succeeds with the contract fixture's answer.
    fn answers(self, script: impl IntoIterator<Item = Result<String, ProviderError>>) -> Self {
        self.inner.answers.borrow_mut().extend(script);
        self
    }

    fn models(self, models: Result<Vec<EndpointModel>, ProviderError>) -> Self {
        *self.inner.models.borrow_mut() = models;
        self
    }

    /// Every probe request it was given, in order.
    fn probes_seen(&self) -> Vec<ProbeRequest> {
        self.inner.seen.borrow().clone()
    }

    /// How many times the run itself asked for an Explanation.
    fn generations(&self) -> usize {
        self.inner.generated.get()
    }
}

impl ProbeEndpoint for FakeEndpoint {
    fn probe(&self, request: &ProbeRequest) -> Result<(), ProviderError> {
        self.inner.seen.borrow_mut().push(*request);
        self.inner.probes.borrow_mut().pop_front().unwrap_or(Ok(()))
    }

    fn models(&self) -> Result<Vec<EndpointModel>, ProviderError> {
        self.inner.models.borrow().clone()
    }
}

impl Provider for FakeEndpoint {
    fn generate(&self, _request: &ExplainRequest) -> Result<String, ProviderError> {
        self.inner.generated.set(self.inner.generated.get() + 1);
        self.inner
            .answers
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Ok(support::ANSWER.to_string()))
    }
}

/// An endpoint that refuses a shape the way DeepSeek does.
fn unsupported() -> ProviderError {
    ProviderError::unsupported_parameter("This response_format type is unavailable now")
}

fn cache(dir: &TempDir) -> Cache {
    Cache::new(dir.join("cache/dev.plainly.app/providers"))
}

/// The configuration a run is probed against: a provider Plainly ships no preset
/// for, so what the probe says is the only thing that decides the shape.
fn custom(endpoint: &str) -> String {
    format!(
        "[app]\nprovider = \"stub\"\n\n[providers.stub]\nendpoint = \"{endpoint}\"\nmodel = \"stub-model\"\n"
    )
}

#[test]
fn a_schema_enforcing_endpoint_is_learned_as_enforced() {
    let dir = TempDir::new("probe-enforced");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting();

    let found = probe::reprobe(&endpoint, &setup, &cache(&dir), now());

    assert_eq!(found.origin, Origin::Probed);
    assert_eq!(found.capability.schema, SchemaSupport::Enforced);
    assert_eq!(
        endpoint.probes_seen(),
        [ProbeRequest {
            schema: SchemaSupport::Enforced,
            disable_thinking: false,
        }],
        "the strongest shape is tried first, and nothing else is asked"
    );
    assert!(found.unanswered.is_none(), "{found:?}");
}

#[test]
fn an_endpoint_that_rejects_json_schema_is_learned_as_best_effort() {
    // The documented DeepSeek signal: json_schema is a 400 and json_object is a
    // 200 on the same endpoint (spec §7).
    let dir = TempDir::new("probe-besteffort");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting().probes([Err(unsupported()), Ok(())]);

    let found = probe::reprobe(&endpoint, &setup, &cache(&dir), now());

    assert_eq!(found.capability.schema, SchemaSupport::BestEffort);
    assert_eq!(
        endpoint.probes_seen().len(),
        2,
        "the second request is what tells 'no schema' apart from 'no request'"
    );
    assert!(found.unanswered.is_none(), "{found:?}");
}

#[test]
fn a_rejected_thinking_switch_takes_the_canonical_claim_away() {
    let dir = TempDir::new("probe-thinking-rejected");
    // DeepSeek is the one preset that claims the canonical spelling.
    let setup = setup("deepseek", "");
    let endpoint =
        FakeEndpoint::accepting().probes([Err(unsupported()), Ok(()), Err(unsupported())]);

    let found = probe::reprobe(&endpoint, &setup, &cache(&dir), now());

    assert_eq!(found.capability.schema, SchemaSupport::BestEffort);
    assert_eq!(found.capability.thinking, ThinkingSwitch::Unsupported);
    assert_eq!(
        endpoint.probes_seen()[2],
        ProbeRequest {
            schema: SchemaSupport::BestEffort,
            disable_thinking: true,
        },
        "the switch is tested only after the schema question is settled"
    );
}

#[test]
fn a_thinking_switch_that_is_accepted_keeps_the_claim() {
    let dir = TempDir::new("probe-thinking-kept");
    let setup = setup("deepseek", "");
    let endpoint = FakeEndpoint::accepting().probes([Ok(()), Ok(())]);

    let found = probe::reprobe(&endpoint, &setup, &cache(&dir), now());

    assert_eq!(found.capability.thinking, ThinkingSwitch::Canonical);
    assert_eq!(
        endpoint.probes_seen()[1],
        ProbeRequest {
            schema: SchemaSupport::Enforced,
            disable_thinking: true,
        }
    );
}

#[test]
fn a_provider_without_the_switch_is_never_asked_about_it() {
    let dir = TempDir::new("probe-thinking-untested");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting();

    probe::reprobe(&endpoint, &setup, &cache(&dir), now());

    assert!(
        endpoint
            .probes_seen()
            .iter()
            .all(|request| !request.disable_thinking),
        "a field nobody claimed would be a guess, and a 200 would not even prove it worked"
    );
}

#[test]
fn an_endpoint_that_rejects_every_shape_is_assumed_cautious_and_left_uncached() {
    let dir = TempDir::new("probe-all-rejected");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting().probes([Err(unsupported()), Err(unsupported())]);

    let found = probe::reprobe(&endpoint, &setup, &cache(&dir), now());

    assert_eq!(found.origin, Origin::Assumed);
    assert_eq!(found.capability.schema, SchemaSupport::BestEffort);
    assert!(found.unanswered.is_some(), "{found:?}");
    assert!(
        !cache(&dir).file("stub").exists(),
        "an endpoint that rejects everything has not told us what it accepts"
    );
}

#[test]
fn an_endpoint_that_never_answers_is_assumed_cautious_and_probed_again_next_time() {
    let dir = TempDir::new("probe-unreachable");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint =
        FakeEndpoint::accepting().probes([Err(ProviderError::unavailable("connection refused"))]);

    let first = probe::capability_for(&endpoint, &setup, &cache(&dir), now());
    assert_eq!(first.origin, Origin::Assumed);
    assert_eq!(first.capability.schema, SchemaSupport::BestEffort);
    assert!(
        !cache(&dir).file("stub").exists(),
        "a transient failure is not a capability: caching it would make it permanent"
    );

    let second = probe::capability_for(&endpoint, &setup, &cache(&dir), now());
    assert_eq!(
        second.origin,
        Origin::Probed,
        "the next run asks again rather than reusing a guess"
    );
}

#[test]
fn a_conclusion_is_cached_and_the_next_run_does_not_probe_again() {
    let dir = TempDir::new("probe-cached");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting().probes([Err(unsupported()), Ok(())]);

    let probed = probe::capability_for(&endpoint, &setup, &cache(&dir), now());
    let asked = endpoint.probes_seen().len();
    let cached = probe::capability_for(&endpoint, &setup, &cache(&dir), now());

    assert_eq!(cached.origin, Origin::Cached);
    assert_eq!(cached.capability, probed.capability);
    assert_eq!(
        endpoint.probes_seen().len(),
        asked,
        "a cached conclusion costs no request at all"
    );
}

#[test]
fn a_changed_endpoint_invalidates_the_cached_conclusion() {
    let dir = TempDir::new("probe-endpoint-changed");
    let first = setup(
        "stub",
        "[providers.stub]\nendpoint = \"http://127.0.0.1:1/v1\"\nmodel = \"m\"\n",
    );
    let second = setup(
        "stub",
        "[providers.stub]\nendpoint = \"http://127.0.0.1:2/v1\"\nmodel = \"m\"\n",
    );
    let endpoint = FakeEndpoint::accepting();

    probe::capability_for(&endpoint, &first, &cache(&dir), now());
    let asked = endpoint.probes_seen().len();
    let found = probe::capability_for(&endpoint, &second, &cache(&dir), now());

    assert_eq!(found.origin, Origin::Probed);
    assert!(endpoint.probes_seen().len() > asked);
}

#[test]
fn a_changed_model_invalidates_the_cached_conclusion() {
    let dir = TempDir::new("probe-model-changed");
    let first = setup(
        "stub",
        "[providers.stub]\nendpoint = \"http://127.0.0.1:1/v1\"\nmodel = \"one\"\n",
    );
    let second = setup(
        "stub",
        "[providers.stub]\nendpoint = \"http://127.0.0.1:1/v1\"\nmodel = \"two\"\n",
    );
    let endpoint = FakeEndpoint::accepting();

    probe::capability_for(&endpoint, &first, &cache(&dir), now());
    let found = probe::capability_for(&endpoint, &second, &cache(&dir), now());

    assert_eq!(found.origin, Origin::Probed);
}

#[test]
fn clearing_the_cache_loses_nothing_but_the_conclusion() {
    let dir = TempDir::new("probe-cache-cleared");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting();

    probe::capability_for(&endpoint, &setup, &cache(&dir), now());
    assert!(cache(&dir).file("stub").exists());

    std::fs::remove_dir_all(dir.join("cache")).expect("the cache directory is removable");

    // The capability is recomputable, so nothing about the provider is lost: the
    // choice lives in the configuration, which the cache never touched.
    let again = probe::capability_for(&endpoint, &setup, &cache(&dir), now());
    assert_eq!(again.origin, Origin::Probed);
    assert_eq!(again.capability.schema, SchemaSupport::Enforced);
}

#[test]
fn the_models_the_endpoint_lists_travel_with_the_conclusion() {
    let dir = TempDir::new("probe-models");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let models = vec![
        EndpointModel {
            id: "loaded-model".to_string(),
            loaded: Some(true),
            context_length: Some(16384),
        },
        EndpointModel {
            id: "cold-model".to_string(),
            loaded: Some(false),
            context_length: None,
        },
    ];
    let endpoint = FakeEndpoint::new().models(Ok(models.clone()));

    probe::reprobe(&endpoint, &setup, &cache(&dir), now());
    let cached = probe::capability_for(&endpoint, &setup, &cache(&dir), now());

    assert_eq!(cached.capability.models, Some(models));
    assert_eq!(cached.origin, Origin::Cached);
}

#[test]
fn a_model_list_that_cannot_be_read_costs_a_list_and_not_a_run() {
    let dir = TempDir::new("probe-models-failed");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::new().models(Err(ProviderError::misconfigured("no such route")));

    let found = probe::reprobe(&endpoint, &setup, &cache(&dir), now());

    assert_eq!(found.origin, Origin::Probed);
    assert_eq!(
        found.capability.models, None,
        "a list nobody could read is not a list of nothing"
    );
}

#[test]
fn a_manual_reprobe_ignores_what_the_cache_says() {
    let dir = TempDir::new("probe-manual");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting().probes([Ok(()), Err(unsupported()), Ok(())]);

    probe::capability_for(&endpoint, &setup, &cache(&dir), now());
    let again = probe::reprobe(&endpoint, &setup, &cache(&dir), now());

    assert_eq!(again.origin, Origin::Probed);
    assert_eq!(again.capability.schema, SchemaSupport::BestEffort);
    assert_eq!(endpoint.probes_seen().len(), 3);
}

#[test]
fn a_run_probes_first_and_asks_with_what_the_endpoint_takes() {
    let dir = TempDir::new("probe-run-first");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting().probes([Err(unsupported()), Ok(())]);
    let connect = |_: &ProviderSetup| -> Box<dyn Endpoint> { Box::new(endpoint.clone()) };

    let run = explain::run(
        &setup,
        &support::request(support::PASSAGE),
        &connect,
        &cache(&dir),
        now(),
        &|_: Duration| {},
    )
    .expect("the run succeeds on the best-effort tier");

    assert_eq!(run.resolution.origin, Origin::Probed);
    assert_eq!(run.resolution.capability.schema, SchemaSupport::BestEffort);
    assert!(run.downgrade.is_none(), "nothing was rejected");
    assert_eq!(endpoint.generations(), 1);
}

/// The setup the run actually asked with is what the capability decides, so the
/// fake records the setups its transports were built for.
#[test]
fn the_shape_the_run_sends_is_the_probed_one() {
    let dir = TempDir::new("probe-run-shape");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting().probes([Err(unsupported()), Ok(())]);
    let seen: RefCell<Vec<(SchemaSupport, ThinkingSwitch)>> = RefCell::new(Vec::new());
    let connect = |setup: &ProviderSetup| -> Box<dyn Endpoint> {
        seen.borrow_mut()
            .push((setup.schema, setup.thinking_switch));
        Box::new(endpoint.clone())
    };

    explain::run(
        &setup,
        &support::request(support::PASSAGE),
        &connect,
        &cache(&dir),
        now(),
        &|_: Duration| {},
    )
    .expect("the run succeeds");

    assert_eq!(
        seen.borrow().last().copied(),
        Some((SchemaSupport::BestEffort, ThinkingSwitch::Unsupported))
    );
}

#[test]
fn an_endpoint_that_rejects_the_shape_is_probed_again_and_asked_once_more() {
    let dir = TempDir::new("probe-run-downgrade");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::new()
        // The first probe says the endpoint enforces a schema …
        .probes([Ok(()), Err(unsupported()), Ok(())])
        // … and the run is rejected anyway, as a cache written by an older
        // Plainly would be: the endpoint changed its mind.
        .answers([Err(unsupported())]);
    let connect = |_: &ProviderSetup| -> Box<dyn Endpoint> { Box::new(endpoint.clone()) };

    let run = explain::run(
        &setup,
        &support::request(support::PASSAGE),
        &connect,
        &cache(&dir),
        now(),
        &|_: Duration| {},
    )
    .expect("the second attempt is answered");

    let downgrade = run.downgrade.expect("the correction is reported");
    assert_eq!(downgrade.from.schema, SchemaSupport::Enforced);
    assert_eq!(run.resolution.capability.schema, SchemaSupport::BestEffort);
    assert_eq!(endpoint.generations(), 2, "the Passage is asked for twice");
    assert_eq!(
        probe::capability_for(&endpoint, &setup, &cache(&dir), now())
            .capability
            .schema,
        SchemaSupport::BestEffort,
        "the correction is what the next run starts from"
    );
}

#[test]
fn an_endpoint_that_contradicts_its_own_probe_is_not_asked_again() {
    let dir = TempDir::new("probe-run-contradiction");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::new()
        // Both probes say the endpoint enforces a schema, and the run is
        // rejected: asking again with the same shape would buy the same 400.
        .probes([Ok(()), Ok(())])
        .answers([Err(unsupported())]);
    let connect = |_: &ProviderSetup| -> Box<dyn Endpoint> { Box::new(endpoint.clone()) };

    let failure = explain::run(
        &setup,
        &support::request(support::PASSAGE),
        &connect,
        &cache(&dir),
        now(),
        &|_: Duration| {},
    )
    .expect_err("the run fails");

    assert_eq!(endpoint.generations(), 1);
    assert!(failure.downgrade.is_none());
    assert_eq!(failure.failure.kind, FailureKind::UnsupportedParameter);
}

#[test]
fn a_failure_that_is_not_about_the_shape_is_reported_as_it_is() {
    let dir = TempDir::new("probe-run-other-failure");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint =
        FakeEndpoint::new().answers([Err(ProviderError::misconfigured("401 invalid key"))]);
    let connect = |_: &ProviderSetup| -> Box<dyn Endpoint> { Box::new(endpoint.clone()) };

    let failure: Box<RunFailure> = explain::run(
        &setup,
        &support::request(support::PASSAGE),
        &connect,
        &cache(&dir),
        now(),
        &|_: Duration| {},
    )
    .expect_err("the run fails");

    assert_eq!(failure.failure.kind, FailureKind::Misconfigured);
    assert!(
        failure.resolution.unanswered.is_none(),
        "the probe answered"
    );
    assert_eq!(endpoint.generations(), 1);
}

#[test]
fn a_second_run_with_a_cached_conclusion_probes_nothing() {
    let dir = TempDir::new("probe-run-cached");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting();
    let connect = |_: &ProviderSetup| -> Box<dyn Endpoint> { Box::new(endpoint.clone()) };

    for _ in 0..2 {
        explain::run(
            &setup,
            &support::request(support::PASSAGE),
            &connect,
            &cache(&dir),
            now(),
            &|_: Duration| {},
        )
        .expect("the run succeeds");
    }

    assert_eq!(
        endpoint.probes_seen().len(),
        1,
        "the first run probes, the second reads the conclusion"
    );
    assert_eq!(endpoint.generations(), 2);
}

/// The retry policy's own budget still applies on top of a downgrade: the two
/// are separate layers, and a run that downgraded and then hit malformed answers
/// gets the documented two extra attempts rather than a third cap.
#[test]
fn a_downgraded_run_still_obeys_the_retry_policy() {
    let dir = TempDir::new("probe-run-downgrade-retry");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::new()
        .probes([Ok(()), Err(unsupported()), Ok(())])
        .answers([
            Err(unsupported()),
            Ok("not an answer".to_string()),
            Ok("not an answer".to_string()),
        ]);
    let connect = |_: &ProviderSetup| -> Box<dyn Endpoint> { Box::new(endpoint.clone()) };

    let failure: Failure = explain::run(
        &setup,
        &support::request(support::PASSAGE),
        &connect,
        &cache(&dir),
        now(),
        &|_: Duration| {},
    )
    .expect_err("the answers never hold the contract")
    .failure;

    assert_eq!(failure.kind, FailureKind::Malformed);
    assert_eq!(
        failure.attempts, 2,
        "the same unusable answer twice is where the policy stops"
    );
    assert_eq!(
        endpoint.generations(),
        3,
        "one rejected shape, then two tries"
    );
}

/// The capability a run reports is not a placeholder: a cached one describes the
/// endpoint the run actually talked to.
#[test]
fn a_cached_conclusion_is_reported_as_cached() {
    let dir = TempDir::new("probe-run-origin");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting();
    let connect = |_: &ProviderSetup| -> Box<dyn Endpoint> { Box::new(endpoint.clone()) };

    probe::capability_for(&endpoint, &setup, &cache(&dir), now());
    let run = explain::run(
        &setup,
        &support::request(support::PASSAGE),
        &connect,
        &cache(&dir),
        now(),
        &|_: Duration| {},
    )
    .expect("the run succeeds");

    assert_eq!(run.resolution.origin, Origin::Cached);
    assert_eq!(run.resolution.capability.schema, SchemaSupport::Enforced);
}

#[test]
fn a_provider_name_cannot_put_the_cache_somewhere_else() {
    let dir = TempDir::new("probe-cache-escape");
    let cache = cache(&dir);

    let path = cache.file("../../escaped");

    assert_eq!(
        path.parent(),
        Some(cache.dir()),
        "the entry stays inside the cache directory"
    );
}

#[test]
fn a_cache_entry_that_cannot_be_read_is_a_miss_rather_than_an_error() {
    let dir = TempDir::new("probe-cache-corrupt");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting();

    probe::capability_for(&endpoint, &setup, &cache(&dir), now());
    std::fs::write(cache(&dir).file("stub"), "{ not an entry").expect("the fixture is writable");

    let found = probe::capability_for(&endpoint, &setup, &cache(&dir), now());

    assert_eq!(
        found.origin,
        Origin::Probed,
        "an unreadable conclusion is recomputed, not reported"
    );
}

/// A re-probe that learns nothing is not a correction. The endpoint refused the
/// shape *and* would not answer the probe, so the cautious shape is a guess —
/// asking the Passage again on a guess is the wasted call the policy exists to
/// refuse, and reporting it as the endpoint's own answer would be worse.
#[test]
fn a_re_probe_that_learns_nothing_does_not_ask_again() {
    let dir = TempDir::new("probe-run-reprobe-silent");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::new()
        // The first probe says the endpoint enforces a schema…
        .probes([Ok(()), Err(ProviderError::unavailable("connection reset"))])
        // …and the run is rejected anyway, as a cache written by an older
        // Plainly would be.
        .answers([Err(unsupported())]);
    let connect = |_: &ProviderSetup| -> Box<dyn Endpoint> { Box::new(endpoint.clone()) };

    let failure = explain::run(
        &setup,
        &support::request(support::PASSAGE),
        &connect,
        &cache(&dir),
        now(),
        &|_: Duration| {},
    )
    .expect_err("the run fails");

    assert_eq!(
        endpoint.generations(),
        1,
        "the Passage is not asked for twice"
    );
    assert!(
        failure.downgrade.is_none(),
        "nothing was learned to correct it"
    );
    assert_eq!(
        failure.resolution.origin,
        Origin::Probed,
        "the failure is reported under the capability the request carried"
    );
    assert_eq!(
        failure.resolution.capability.schema,
        SchemaSupport::Enforced
    );
    assert_eq!(
        failure.reprobe.expect("the re-probe is reported").origin,
        Origin::Assumed
    );
    assert_eq!(failure.failure.kind, FailureKind::UnsupportedParameter);
}

/// The switch question is only settled by a request that carried the field. A
/// provider whose route cannot carry it keeps its claim *unverified*, and an
/// unverified claim is not something to write into a cache with no TTL.
#[test]
fn a_switch_the_route_cannot_carry_is_not_probed() {
    let dir = TempDir::new("probe-switch-untestable");
    let setup = setup("ollama", "[providers.ollama]\nmodel = \"qwen3.5:4b\"\n")
        .with_capability(SchemaSupport::Enforced, ThinkingSwitch::Canonical);
    assert_eq!(setup.surface, Surface::Ollama);
    let endpoint = FakeEndpoint::accepting();

    let found = probe::reprobe(&endpoint, &setup, &cache(&dir), now());

    assert!(
        endpoint
            .probes_seen()
            .iter()
            .all(|request| !request.disable_thinking),
        "the canonical field does not travel on this route, so asking would prove nothing"
    );
    assert_eq!(found.capability.thinking, ThinkingSwitch::Canonical);
    assert!(
        !cache(&dir).file("ollama").exists(),
        "a claim nobody could verify is not a probed fact"
    );
    assert!(found.unwritten.is_some(), "{found:?}");
}

#[test]
fn a_thinking_switch_that_could_not_be_verified_is_not_cached() {
    let dir = TempDir::new("probe-switch-unanswered");
    let setup = setup("deepseek", "");
    let endpoint = FakeEndpoint::accepting().probes([
        Err(unsupported()),
        Ok(()),
        Err(ProviderError::unavailable("connection reset")),
    ]);

    let found = probe::reprobe(&endpoint, &setup, &cache(&dir), now());

    assert_eq!(found.capability.schema, SchemaSupport::BestEffort);
    assert_eq!(
        found.capability.thinking,
        ThinkingSwitch::Canonical,
        "a hiccup is not evidence against the vendor's own spelling"
    );
    assert!(
        !cache(&dir).file("deepseek").exists(),
        "an unverified claim must not become a cached fact"
    );
    assert!(found.unwritten.is_some(), "{found:?}");
}

/// A correction the request cannot reflect is not a correction. With thinking
/// asked for, the canonical field is never sent, so a switch-only change would
/// buy a byte-identical second ask.
#[test]
fn a_correction_the_request_cannot_reflect_does_not_ask_again() {
    let dir = TempDir::new("probe-switch-unreflected");
    let setup = setup("deepseek", "[providers.deepseek]\nthinking = \"on\"\n");
    assert_eq!(setup.thinking, Thinking::On);
    let endpoint = FakeEndpoint::new()
        // The first probe verifies both the schema and the switch, the run is
        // rejected, and the re-probe withdraws only the switch — which a request
        // made with thinking on never carried.
        .probes([Ok(()), Ok(()), Ok(()), Err(unsupported())])
        .answers([Err(unsupported())]);
    let connect = |_: &ProviderSetup| -> Box<dyn Endpoint> { Box::new(endpoint.clone()) };

    let failure = explain::run(
        &setup,
        &support::request(support::PASSAGE),
        &connect,
        &cache(&dir),
        now(),
        &|_: Duration| {},
    )
    .expect_err("the run fails");

    assert_eq!(
        endpoint.generations(),
        1,
        "the second request would have been identical"
    );
    assert!(failure.downgrade.is_none());
    assert_eq!(
        failure.resolution.capability.schema,
        SchemaSupport::Enforced,
        "the failure is reported under the shape the request carried"
    );
    assert_eq!(
        failure
            .reprobe
            .expect("the re-probe is reported")
            .capability
            .thinking,
        ThinkingSwitch::Unsupported,
        "the conclusion the request could not use is still reported"
    );
}

/// A cache that cannot be written costs a re-probe next time and nothing else.
#[test]
fn a_cache_that_cannot_be_written_does_not_fail_the_run() {
    let dir = TempDir::new("probe-cache-unwritable");
    let config = Config::parse(&custom("http://127.0.0.1:1/v1")).expect("valid TOML");
    let setup = ProviderSetup::resolve("stub", &config).expect("the provider resolves");
    let endpoint = FakeEndpoint::accepting();

    // The cache directory's parent is a file, so it cannot be created.
    let blocked = dir.join("blocked");
    std::fs::write(&blocked, "not a directory").expect("the fixture is writable");
    let unwritable = Cache::new(blocked.join("providers"));

    let found = probe::reprobe(&endpoint, &setup, &unwritable, now());

    assert_eq!(found.origin, Origin::Probed);
    assert_eq!(found.capability.schema, SchemaSupport::Enforced);
    assert!(
        found.unwritten.is_some(),
        "the reason travels with the conclusion"
    );
}
