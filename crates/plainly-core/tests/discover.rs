//! Local runtime discovery: which endpoints are asked, what counts as an
//! answer, and what the models a runtime lists say about it.
//!
//! The transport here is a script rather than a server: discovery's whole
//! question is "did this endpoint answer, and with what", and a fake that
//! answers per endpoint is what makes the decision reachable without a network
//! or a running model.

mod support;

use std::cell::RefCell;

use plainly_core::discover::{self, Candidate, ModelLister};
use plainly_core::{Config, EndpointModel, LocalRuntime, ProviderError, ProviderErrorKind};

/// One answer the fake gives: the endpoint it belongs to, and what it says.
type Answer = (&'static str, Result<Vec<EndpointModel>, ProviderError>);

/// The scripted answers, keyed by endpoint.
type Answers = Vec<(String, Result<Vec<EndpointModel>, ProviderError>)>;

/// A model-list reader whose answers are keyed by endpoint: an endpoint with no
/// answer is one nothing is listening on.
struct FakeLister {
    answers: RefCell<Answers>,
    asked: RefCell<Vec<(String, String)>>,
}

impl FakeLister {
    fn new(answers: impl IntoIterator<Item = Answer>) -> Self {
        Self {
            answers: RefCell::new(
                answers
                    .into_iter()
                    .map(|(endpoint, answer)| (endpoint.to_string(), answer))
                    .collect(),
            ),
            asked: RefCell::new(Vec::new()),
        }
    }

    /// The endpoints it was asked about, in order.
    fn asked(&self) -> Vec<String> {
        self.asked
            .borrow()
            .iter()
            .map(|(endpoint, _)| endpoint.clone())
            .collect()
    }

    /// What it was asked, as `(endpoint, provider)`: the name travels with the
    /// endpoint because a credential belongs to the name (tickets/06).
    fn asked_with_names(&self) -> Vec<(String, String)> {
        self.asked.borrow().clone()
    }
}

impl ModelLister for FakeLister {
    fn models_at(
        &self,
        endpoint: &str,
        provider: &str,
    ) -> Result<Vec<EndpointModel>, ProviderError> {
        self.asked
            .borrow_mut()
            .push((endpoint.to_string(), provider.to_string()));
        self.answers
            .borrow_mut()
            .iter()
            .find(|(known, _)| known == endpoint)
            .map(|(_, answer)| answer.clone())
            .unwrap_or_else(|| Err(ProviderError::unavailable("connection refused")))
    }
}

/// A model as a runtime volunteers it.
fn model(id: &str, loaded: Option<bool>, context: Option<u64>) -> EndpointModel {
    EndpointModel {
        id: id.to_string(),
        loaded,
        context_length: context,
    }
}

/// A configuration with nothing in it: the candidate list is then exactly the
/// shipped common ports, which is the list this test pins.
fn empty_config() -> Config {
    Config::parse("").expect("an empty document is the default configuration")
}

#[test]
fn a_runtime_that_answers_is_found_under_the_provider_its_port_belongs_to() {
    let config = empty_config();
    let lister = FakeLister::new([(
        "http://127.0.0.1:11434/v1",
        Ok(vec![model("qwen3.5:4b", Some(true), Some(16384))]),
    )]);

    let scan = discover::scan(&lister, &discover::candidates(&config));

    assert_eq!(scan.runtimes.len(), 1, "{scan:?}");
    assert_eq!(scan.runtimes[0].provider, "ollama");
    assert_eq!(scan.runtimes[0].endpoint, "http://127.0.0.1:11434/v1");
    assert_eq!(
        scan.runtimes[0].models,
        vec![model("qwen3.5:4b", Some(true), Some(16384))]
    );
    assert!(
        scan.unanswered
            .iter()
            .all(|candidate| candidate.error.kind() == ProviderErrorKind::Unavailable),
        "the other three ports are the ordinary absence: {scan:?}"
    );
}

/// The four ports the ticket names, in the order it names them: the list is what
/// "the common ports" means, so it is pinned rather than described.
#[test]
fn the_common_ports_are_the_documented_ones() {
    let lister = FakeLister::new([]);

    discover::scan(&lister, &discover::candidates(&empty_config()));

    assert_eq!(
        lister.asked(),
        [
            "http://127.0.0.1:3060/v1",
            "http://127.0.0.1:11434/v1",
            "http://127.0.0.1:8080/v1",
            "http://127.0.0.1:1234/v1",
        ]
    );
}

/// Every common port is tried and none of them answered: the answer is "nothing
/// is there", not a port guessed from a provider's reputation.
#[test]
fn a_scan_that_finds_nothing_finds_nothing() {
    let lister = FakeLister::new([]);

    let scan = discover::scan(&lister, &discover::candidates(&empty_config()));

    assert!(scan.runtimes.is_empty(), "{scan:?}");
    assert_eq!(lister.asked().len(), 4, "every common port was asked");
    assert_eq!(scan.unanswered.len(), 4, "and every one of them said why");
}

/// A port that refused does not hide a runtime that answered, and what the
/// runtime said about a model — loaded or not — travels with it.
#[test]
fn a_silent_port_does_not_hide_a_runtime_that_answered() {
    let lister = FakeLister::new([(
        "http://127.0.0.1:1234/v1",
        Ok(vec![
            model("lmstudio-community/qwen3.5-4b", Some(false), Some(32768)),
            model("cold-model", None, None),
        ]),
    )]);

    let scan = discover::scan(&lister, &discover::candidates(&empty_config()));

    assert_eq!(scan.runtimes.len(), 1, "{scan:?}");
    assert_eq!(scan.runtimes[0].provider, "lmstudio");
    assert_eq!(scan.runtimes[0].models[0].loaded, Some(false));
    assert_eq!(scan.runtimes[0].models[1].context_length, None);
}

/// A runtime on a port of the user's own choosing is found too: its configured
/// endpoint is not a guess, and the name it is found under is the user's own.
#[test]
fn a_configured_loopback_endpoint_is_a_candidate_under_the_configured_name() {
    let config =
        Config::parse("[providers.mine]\nendpoint = \"http://127.0.0.1:5353/v1\"\nmodel = \"m\"\n")
            .expect("the fixture is valid TOML");

    let candidates = discover::candidates(&config);

    assert!(
        candidates.contains(&Candidate {
            provider: "mine".to_string(),
            endpoint: "http://127.0.0.1:5353/v1".to_string(),
        }),
        "{candidates:?}"
    );
}

/// A hosted endpoint is not scanned: discovery is about this machine, and
/// asking a vendor's API what it serves is a different question with a different
/// answer (and would need a key).
#[test]
fn a_configured_remote_endpoint_is_not_scanned() {
    let config = Config::parse(
        "[providers.deepseek]\nendpoint = \"https://api.deepseek.com/v1\"\nmodel = \"m\"\n",
    )
    .expect("the fixture is valid TOML");

    let candidates = discover::candidates(&config);

    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.provider != "deepseek"),
        "{candidates:?}"
    );
}

/// A name pointing at a common port's own endpoint is not asked twice under two
/// names: the preset already names that port, and the configured spelling is the
/// same one.
#[test]
fn an_endpoint_with_the_name_it_already_has_is_not_asked_twice() {
    let config = Config::parse(
        "[providers.ollama]\nendpoint = \"http://127.0.0.1:11434/v1\"\nmodel = \"m\"\n",
    )
    .expect("the fixture is valid TOML");

    let candidates = discover::candidates(&config);
    let on_the_port: Vec<&Candidate> = candidates
        .iter()
        .filter(|candidate| candidate.endpoint == "http://127.0.0.1:11434/v1")
        .collect();

    assert_eq!(on_the_port.len(), 1, "{candidates:?}");
    assert_eq!(candidates.len(), 4, "the other three ports are still there");
}

/// One endpoint under two names keeps both: merging them by service would make
/// one of the user's providers unselectable, while asking twice costs a GET.
#[test]
fn one_service_under_two_names_is_asked_under_both() {
    let config = Config::parse(
        "[providers.mine]\nendpoint = \"http://127.0.0.1:11434/v1\"\nmodel = \"m\"\n\
         [providers.other]\nendpoint = \"http://localhost:11434/v1\"\nmodel = \"m\"\n",
    )
    .expect("the fixture is valid TOML");

    let candidates = discover::candidates(&config);
    let names: Vec<&str> = candidates
        .iter()
        .filter(|candidate| candidate.provider == "mine" || candidate.provider == "other")
        .map(|candidate| candidate.provider.as_str())
        .collect();

    assert_eq!(names, ["mine", "other"], "{candidates:?}");
}

/// A runtime, as the scan would have found it, listing exactly `id`.
fn runtime_with(provider: &str, endpoint: &str, id: &str) -> LocalRuntime {
    LocalRuntime {
        provider: provider.to_string(),
        endpoint: endpoint.to_string(),
        models: vec![model(id, Some(true), Some(16384))],
    }
}

/// A runtime, as the scan would have found it, listing `m`.
fn runtime(provider: &str, endpoint: &str) -> LocalRuntime {
    runtime_with(provider, endpoint, "m")
}

/// Serving a model is the runtime's own list, and it is what a selection asks
/// before it writes anything.
#[test]
fn a_runtime_knows_which_models_it_serves() {
    let runtime = runtime("llamacpp", "http://127.0.0.1:3060/v1");

    assert!(runtime.serves("m"));
    assert_eq!(runtime.model("m").map(|entry| entry.id.as_str()), Some("m"));
    assert!(!runtime.serves("nosuch"));
    assert!(runtime.model("nosuch").is_none());
}

/// The name travels with the endpoint into the transport, because a credential
/// belongs to the name: an implementation that authenticates has nothing else to
/// look a key up by.
#[test]
fn a_scan_asks_each_candidate_under_its_own_provider_name() {
    let config =
        Config::parse("[providers.mine]\nendpoint = \"http://127.0.0.1:5353/v1\"\nmodel = \"m\"\n")
            .expect("the fixture is valid TOML");
    let lister = FakeLister::new([]);

    discover::scan(&lister, &discover::candidates(&config));

    let asked = lister.asked_with_names();
    assert_eq!(asked.len(), 5, "{asked:?}");
    assert_eq!(
        asked[0],
        (
            "http://127.0.0.1:3060/v1".to_string(),
            "llamacpp".to_string()
        )
    );
    assert_eq!(
        asked[4],
        ("http://127.0.0.1:5353/v1".to_string(), "mine".to_string())
    );
}

/// The name's own runtime, when it has the model, is the one — whether the name
/// was configured or only guessed a port, and whether the endpoint is spelled
/// the same way or is the same service spelled differently.
#[test]
fn a_selection_takes_the_names_own_runtime_when_it_has_the_model() {
    let at_3060 = runtime("llamacpp", "http://127.0.0.1:3060/v1");
    let at_8080 = runtime("llamacpp", "http://127.0.0.1:8080/v1");
    let runtimes = [at_3060, at_8080];

    assert_eq!(
        discover::select(&runtimes, "llamacpp", "m", "http://127.0.0.1:3060/v1", true),
        discover::Selection::Use(&runtimes[0])
    );
    assert_eq!(
        discover::select(&runtimes, "llamacpp", "m", "http://127.0.0.1:8080", true),
        discover::Selection::Use(&runtimes[1]),
        "the same service spelled without its path is still the name's endpoint"
    );
}

/// A name that only guessed a port takes the runtime that has the model: the
/// model is what tells two runtimes under one name apart, and finding the right
/// port without being told is the point (tickets/06).
#[test]
fn a_guessed_name_takes_the_runtime_that_lists_the_model() {
    let holding = runtime_with("llamacpp", "http://127.0.0.1:3060/v1", "wanted");
    let other = runtime_with("llamacpp", "http://127.0.0.1:8080/v1", "different");
    let runtimes = [holding, other];

    assert_eq!(
        discover::select(
            &runtimes,
            "llamacpp",
            "wanted",
            "http://127.0.0.1:8080/v1",
            false
        ),
        discover::Selection::Use(&runtimes[0]),
        "the preset's port is a guess, so the model decides"
    );
}

/// A name the user pointed somewhere is never moved: the runtime that has the
/// model is reported instead, and so is one that answered where the user's own
/// endpoint did not.
#[test]
fn a_configured_name_is_told_where_the_model_is_instead_of_moved() {
    let holding = runtime_with("llamacpp", "http://127.0.0.1:3060/v1", "wanted");
    let pointed = runtime_with("llamacpp", "http://127.0.0.1:8080/v1", "different");
    let silent = [holding.clone(), pointed.clone()];

    // The endpoint the user pointed at answered, and does not have the model.
    assert_eq!(
        discover::select(
            &silent,
            "llamacpp",
            "wanted",
            "http://127.0.0.1:8080/v1",
            true
        ),
        discover::Selection::Elsewhere(&silent[0])
    );

    // The endpoint the user pointed at did not answer at all.
    let only = [holding];
    assert_eq!(
        discover::select(&only, "llamacpp", "wanted", "http://127.0.0.1:9/v1", true),
        discover::Selection::Silent(&only[0]),
        "a runtime that answered somewhere else is reported, not selected"
    );
}

/// Nothing under the name has the model, or nothing under the name answered at
/// all: two different reports, both from what answered rather than from a guess.
#[test]
fn a_model_nobody_serves_and_a_name_nobody_answered_are_told_apart() {
    let runtimes = [runtime_with(
        "llamacpp",
        "http://127.0.0.1:3060/v1",
        "different",
    )];

    assert_eq!(
        discover::select(
            &runtimes,
            "llamacpp",
            "wanted",
            "http://127.0.0.1:3060/v1",
            false
        ),
        discover::Selection::Unserved(&runtimes[0])
    );
    assert_eq!(
        discover::select(
            &runtimes,
            "ollama",
            "wanted",
            "http://127.0.0.1:11434/v1",
            false
        ),
        discover::Selection::Unanswered
    );
}

/// A candidate that answered and refused is kept with what it said: "nothing is
/// listening" and "something wants a key" are different facts, and only one of
/// them is fixed by starting a runtime (tickets/06).
#[test]
fn a_candidate_that_answered_and_refused_keeps_its_reason() {
    let lister = FakeLister::new([(
        "http://127.0.0.1:11434/v1",
        Err(ProviderError::misconfigured(
            "ollama answered HTTP 401: unauthorized",
        )),
    )]);

    let scan = discover::scan(&lister, &discover::candidates(&empty_config()));

    let refused = scan
        .unanswered
        .iter()
        .find(|candidate| candidate.endpoint == "http://127.0.0.1:11434/v1")
        .expect("the refusal is kept");
    assert_eq!(refused.error.kind(), ProviderErrorKind::Misconfigured);
    assert!(refused.error.to_string().contains("401"), "{refused:?}");
}

/// A silent configured endpoint is reported with the same-name runtime that can
/// actually serve the model, not merely the first one that answered: sending a
/// reader to a runtime that does not have the model is a report that cannot be
/// acted on.
#[test]
fn a_silent_name_is_reported_with_the_runtime_that_serves_the_model() {
    let without = runtime_with("llamacpp", "http://127.0.0.1:3060/v1", "different");
    let holding = runtime_with("llamacpp", "http://127.0.0.1:8080/v1", "wanted");
    let runtimes = [without, holding];

    assert_eq!(
        discover::select(
            &runtimes,
            "llamacpp",
            "wanted",
            "http://127.0.0.1:9/v1",
            true
        ),
        discover::Selection::Silent(&runtimes[1])
    );
}
