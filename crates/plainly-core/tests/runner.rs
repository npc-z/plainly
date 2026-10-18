//! The Runner's interface: one run's machine state, and one Passage's turn.
//!
//! Everything below is driven through [`plainly_core::Runner`] — the seam a
//! surface actually calls — with a scripted endpoint, injected paths and secret
//! stores, and a temporary history. There is no network, no display and no real
//! keyring: the endpoint is the same scripted `Endpoint` the probe tests use.

mod support;

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use plainly_core::{
    CredentialError, EndpointModel, EnvSecrets, ExplainRequest, FailureKind, Level, Origin, RunNotice, TurnNotice,
    PathInputs, Paths, ProbeEndpoint, ProbeRequest, Prompt, PromptChoice, Provider, ProviderError,
    Recall, Runner, RunnerError, SchemaSupport, SecretError, SecretStore, Secrets, SetupError,
    Store, StoreError, Timestamp, TurnFailure, UnavailableError,
};

use support::TempDir;
use support::{ANSWER, PASSAGE};

/// The instant the first fixture run is stamped with.
fn now() -> Timestamp {
    Timestamp::from_unix_seconds(1_760_000_000).expect("the fixture instant is in range")
}

/// A later instant, for a run that must move `last_seen` without moving
/// `created_at`.
fn later() -> Timestamp {
    Timestamp::from_unix_seconds(1_760_000_100).expect("the fixture instant is in range")
}

/// The setup name and credential each scripted transport was built with.
type Seen = Rc<RefCell<Vec<(String, Option<String>)>>>;

/// A provider Plainly ships no preset for: what the scripted endpoint says is
/// the only thing that decides the shape.
const CONFIG: &str = "[app]\nprovider = \"stub\"\n\n\
                      [providers.stub]\nendpoint = \"http://127.0.0.1:9/v1\"\n\
                      model = \"stub-model\"\n";

fn paths(dir: &TempDir) -> Paths {
    Paths::resolve(&PathInputs::new(dir.join("home"))).expect("the fixture paths resolve")
}

fn write_config(paths: &Paths, text: &str) {
    std::fs::create_dir_all(paths.config_dir()).expect("the config directory is creatable");
    std::fs::write(paths.config_file(), text).expect("the fixture config is writable");
}

/// An environment with no keys and no keyring: a headless machine.
fn secrets() -> Secrets {
    Secrets::with_stores(EnvSecrets::empty(), None)
}

/// A keyring that cannot answer, the way a locked one does.
struct BrokenKeyring;

impl SecretStore for BrokenKeyring {
    fn get(&self, _provider: &str) -> Result<Option<String>, SecretError> {
        Err(SecretError::Unavailable(
            "the keyring is locked".to_string(),
        ))
    }

    fn set(&self, _provider: &str, _secret: &str) -> Result<(), SecretError> {
        Ok(())
    }

    fn delete(&self, _provider: &str) -> Result<(), SecretError> {
        Ok(())
    }
}

/// An environment with no keys and a keyring that cannot answer.
fn broken_secrets() -> Secrets {
    Secrets::with_stores(EnvSecrets::empty(), Some(Box::new(BrokenKeyring)))
}

/// Open a run against the scripted endpoint.
fn open(paths: &Paths, endpoint: FakeEndpoint, now: Timestamp) -> Runner {
    Runner::open_at(paths.clone(), secrets(), PromptChoice::Configured, now)
        .expect("the fixture opens")
        .with_transport(Box::new(move |_, _| Box::new(endpoint.clone())))
}

/// Open the fixture and return why it could not; opening is the thing under
/// test, so a success is the failure of the test.
fn open_error(paths: Paths) -> RunnerError {
    match Runner::open_at(paths, secrets(), PromptChoice::Configured, now()) {
        Ok(_) => panic!("the fixture must not open"),
        Err(error) => error,
    }
}

/// An endpoint whose probes and answers are a script.
#[derive(Clone)]
struct FakeEndpoint {
    inner: Rc<Inner>,
}

struct Inner {
    probes: RefCell<VecDeque<Result<(), ProviderError>>>,
    answers: RefCell<VecDeque<Result<String, ProviderError>>>,
    seen: RefCell<Vec<ExplainRequest>>,
    generations: Cell<usize>,
}

impl FakeEndpoint {
    /// An endpoint that accepts every shape and answers with the contract
    /// fixture, any number of times.
    fn accepting() -> Self {
        Self {
            inner: Rc::new(Inner {
                probes: RefCell::new(VecDeque::new()),
                answers: RefCell::new(VecDeque::new()),
                seen: RefCell::new(Vec::new()),
                generations: Cell::new(0),
            }),
        }
    }

    /// The script a probe walks through, in order. Beyond it a probe succeeds.
    fn probes(self, script: impl IntoIterator<Item = Result<(), ProviderError>>) -> Self {
        self.inner.probes.borrow_mut().extend(script);
        self
    }

    /// The script the run itself walks through, in order. Beyond it a request
    /// succeeds with the contract fixture's answer.
    fn answers(self, script: impl IntoIterator<Item = Result<String, ProviderError>>) -> Self {
        self.inner.answers.borrow_mut().extend(script);
        self
    }

    /// How many times the endpoint was asked for an Explanation.
    fn generations(&self) -> usize {
        self.inner.generations.get()
    }

    fn requests(&self) -> Vec<ExplainRequest> {
        self.inner.seen.borrow().clone()
    }
}

impl ProbeEndpoint for FakeEndpoint {
    fn probe(&self, _request: &ProbeRequest) -> Result<(), ProviderError> {
        self.inner.probes.borrow_mut().pop_front().unwrap_or(Ok(()))
    }

    fn models(&self) -> Result<Vec<EndpointModel>, ProviderError> {
        Ok(Vec::new())
    }
}

impl Provider for FakeEndpoint {
    fn generate(&self, request: &ExplainRequest) -> Result<String, ProviderError> {
        self.inner.generations.set(self.inner.generations.get() + 1);
        self.inner.seen.borrow_mut().push(request.clone());
        self.inner
            .answers
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Ok(ANSWER.to_string()))
    }
}

/// An endpoint that refuses a shape the way DeepSeek does.
fn unsupported() -> ProviderError {
    ProviderError::unsupported_parameter("This response_format type is unavailable now")
}

#[test]
fn a_reuse_hit_answers_from_the_store_and_moves_last_seen() {
    let dir = TempDir::new("runner-reuse");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);
    let endpoint = FakeEndpoint::accepting();

    let first = open(&paths, endpoint.clone(), now())
        .explain(PASSAGE, Recall::Reuse)
        .expect("the fixture answers");
    let stored = first.record.expect("the first turn stored its answer");

    let mut runner = open(&paths, endpoint.clone(), later());
    let turn = runner
        .explain(PASSAGE, Recall::Reuse)
        .expect("the store answers");

    assert_eq!(endpoint.generations(), 1, "a hit asks the provider nothing");
    assert!(turn.from_cache);
    assert!(
        turn.resolution.is_none(),
        "a Record carries no tier to report"
    );
    assert!(turn.downgrade.is_none());
    assert_eq!(
        turn.artifact, stored.artifact,
        "the stored answer is the answer"
    );

    let record = turn.record.expect("a hit is a Record");
    assert_eq!(record.id, stored.id, "the same row is reused");
    assert_eq!(
        record.last_seen,
        later(),
        "the row moves to the top of the history"
    );
    assert_eq!(
        record.artifact.created_at,
        now(),
        "reuse does not rewrite creation"
    );
}

#[test]
fn a_configuration_that_does_not_parse_is_a_config_error() {
    let dir = TempDir::new("runner-config-error");
    let paths = paths(&dir);
    write_config(&paths, "[app\n");

    let error = open_error(paths);

    assert!(matches!(error, RunnerError::Config(_)), "got {error:?}");
}

#[test]
fn a_provider_that_does_not_resolve_is_a_setup_error() {
    let dir = TempDir::new("runner-setup-error");
    let paths = paths(&dir);
    write_config(&paths, "[app]\nprovider = \"not-configured\"\n");

    let error = open_error(paths);

    assert!(matches!(error, RunnerError::Setup(_)), "got {error:?}");
}

#[test]
fn a_history_that_cannot_open_is_a_store_error() {
    let dir = TempDir::new("runner-store-error");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);
    // The data directory is a file, so the history file cannot be created
    // inside it: local state a person has to fix, not a provider problem.
    std::fs::create_dir_all(paths.data_dir().parent().expect("the data root"))
        .expect("the data root is creatable");
    std::fs::write(paths.data_dir(), "not a directory").expect("the fixture is writable");

    let error = open_error(paths);

    assert!(matches!(error, RunnerError::Store(_)), "got {error:?}");
}

#[test]
fn a_required_key_that_is_missing_is_a_credential_error() {
    let dir = TempDir::new("runner-credential-error");
    let paths = paths(&dir);
    // The shipped DeepSeek preset requires a key; there is none anywhere.
    write_config(&paths, "[app]\nprovider = \"deepseek\"\n");

    let error = open_error(paths);

    let RunnerError::Credential(CredentialError::Missing { provider, .. }) = error else {
        panic!("expected a missing credential, got {error:?}");
    };
    assert_eq!(provider, "deepseek");
}

#[test]
fn a_required_key_that_is_unreadable_is_not_a_missing_key() {
    let dir = TempDir::new("runner-credential-unreadable");
    let paths = paths(&dir);
    // The shipped DeepSeek preset requires a key, and the keyring cannot
    // answer: a broken store is not the same failure as no key anywhere.
    write_config(&paths, "[app]\nprovider = \"deepseek\"\n");

    let error = match Runner::open_at(paths, broken_secrets(), PromptChoice::Configured, now()) {
        Ok(_) => panic!("the fixture must not open"),
        Err(error) => error,
    };

    let RunnerError::Credential(CredentialError::Unreadable { provider, source }) = error else {
        panic!("expected an unreadable credential, got {error:?}");
    };
    assert_eq!(provider, "deepseek");
    assert!(
        matches!(source, SecretError::Unavailable(_)),
        "the keyring's own reason is kept"
    );
}

#[test]
fn an_optional_key_that_is_unreadable_opens_with_one_notice_and_explains_without_a_key() {
    let dir = TempDir::new("runner-credential-optional");
    let paths = paths(&dir);
    // A provider Plainly ships no preset for may authenticate nothing, so a
    // broken keyring must not stop the run — but it must not be silent either.
    write_config(&paths, CONFIG);
    let endpoint = FakeEndpoint::accepting();
    let keys: Rc<RefCell<Vec<Option<String>>>> = Rc::new(RefCell::new(Vec::new()));
    let mut runner = Runner::open_at(
        paths.clone(),
        broken_secrets(),
        PromptChoice::Configured,
        now(),
    )
    .expect("a provider that may need no key is not blocked by a broken keyring")
    .with_transport(Box::new({
        let endpoint = endpoint.clone();
        let keys = keys.clone();
        move |_, key| {
            keys.borrow_mut().push(key.map(str::to_string));
            Box::new(endpoint.clone())
        }
    }));

    assert_eq!(
        runner.notices().len(),
        1,
        "the unreadable keyring is reported exactly once"
    );
    assert!(
        matches!(
            &runner.notices()[0],
            RunNotice::KeyringUnavailable { provider, .. } if provider == "stub"
        ),
        "got {:?}",
        runner.notices()
    );

    let turn = runner
        .explain(PASSAGE, Recall::Reuse)
        .expect("the turn proceeds without a key");
    assert!(turn.record.is_some(), "the answer was still stored");
    assert_eq!(
        endpoint.generations(),
        1,
        "the provider answered without a credential"
    );
    assert!(
        keys.borrow().iter().all(Option::is_none),
        "no key was invented for the turn"
    );
    assert_eq!(
        runner.notices().len(),
        1,
        "a turn does not repeat the run-level notice"
    );
}

#[test]
fn an_optional_key_that_is_missing_opens_without_a_notice() {
    let dir = TempDir::new("runner-credential-optional-missing");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);

    let runner = Runner::open_at(paths, secrets(), PromptChoice::Configured, now())
        .expect("a provider that may need no key opens without one");

    assert!(
        runner.notices().is_empty(),
        "nothing to report when there is simply no key"
    );
}

#[test]
fn a_bypass_ignores_the_cache_and_overwrites_the_same_record_in_place() {
    let dir = TempDir::new("runner-bypass");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);
    let endpoint = FakeEndpoint::accepting();

    let first = open(&paths, endpoint.clone(), now())
        .explain(PASSAGE, Recall::Reuse)
        .expect("the fixture answers");
    let stored = first.record.expect("the first turn stored its answer");

    let mut runner = open(&paths, endpoint.clone(), later());
    let turn = runner
        .explain(PASSAGE, Recall::Bypass { provider: None })
        .expect("the bypass answers");

    assert_eq!(
        endpoint.generations(),
        2,
        "the bypass asks even though the store already has the answer"
    );
    assert!(!turn.from_cache);
    let record = turn.record.expect("the bypass stored its answer");
    assert_eq!(record.id, stored.id, "the same row is overwritten");
    assert_eq!(
        record.artifact.created_at,
        now(),
        "the record was made when it was made"
    );
    assert_eq!(
        record.last_seen,
        now(),
        "an overwrite does not move the row in the history"
    );
    assert_eq!(
        record.artifact.generated_at,
        later(),
        "only the generation is new"
    );
}

#[test]
fn a_bypass_with_a_provider_uses_that_providers_setup_and_credential() {
    let dir = TempDir::new("runner-bypass-provider");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);
    // The configured provider is the keyless stub; the turn asks DeepSeek,
    // which requires a key and has one exported.
    let secrets = Secrets::with_stores(
        EnvSecrets::from_pairs([("PLAINLY_DEEPSEEK_API_KEY", "override-key")]),
        None,
    );
    let endpoint = FakeEndpoint::accepting();
    let seen: Seen = Rc::new(RefCell::new(Vec::new()));
    let mut runner = Runner::open_at(paths, secrets, PromptChoice::Configured, now())
        .expect("the fixture opens")
        .with_transport(Box::new({
            let endpoint = endpoint.clone();
            let seen = seen.clone();
            move |setup, key| {
                seen.borrow_mut()
                    .push((setup.name.clone(), key.map(str::to_string)));
                Box::new(endpoint.clone())
            }
        }));

    let turn = runner
        .explain(
            PASSAGE,
            Recall::Bypass {
                provider: Some("deepseek".to_string()),
            },
        )
        .expect("the override provider answers");

    assert_eq!(
        endpoint.generations(),
        1,
        "the bypass asked the override provider once"
    );
    assert_eq!(turn.artifact.provider, "deepseek");
    assert_eq!(
        turn.artifact.model, "deepseek-flash",
        "the override's model is the one the request carries"
    );
    assert!(!turn.from_cache, "a bypass never reads the cache");
    assert!(
        !seen.borrow().is_empty(),
        "the override's transport was built"
    );
    assert!(
        seen.borrow().iter().all(|(name, _)| name == "deepseek"),
        "the transport was built for the override setup, got {:?}",
        seen.borrow()
    );
    assert!(
        seen.borrow()
            .iter()
            .all(|(_, key)| key.as_deref() == Some("override-key")),
        "the override's own credential reached the transport, got {:?}",
        seen.borrow()
    );
    assert!(
        !turn.local,
        "a hosted provider is not on this machine, whichever name it was asked under"
    );
}

#[test]
fn a_turns_local_fact_is_the_provider_it_actually_asked() {
    let dir = TempDir::new("runner-turn-local");
    let paths = paths(&dir);
    // The configured provider is hosted; the one a bypass names is on this
    // machine.
    write_config(
        &paths,
        "[app]\nprovider = \"hosted\"\n\n\
         [providers.hosted]\nendpoint = \"https://api.example.invalid/v1\"\n\
         model = \"hosted-model\"\n\n\
         [providers.onbox]\nendpoint = \"http://127.0.0.1:9/v1\"\n\
         model = \"onbox-model\"\n",
    );
    let endpoint = FakeEndpoint::accepting();
    let mut runner = open(&paths, endpoint.clone(), now());

    let configured = runner
        .explain(PASSAGE, Recall::Reuse)
        .expect("the configured provider answers");
    assert!(!configured.local, "the configured endpoint is hosted");

    let replayed = runner
        .explain(PASSAGE, Recall::Bypass { provider: None })
        .expect("the configured provider answers again");
    assert!(
        !replayed.local,
        "bypassing without a provider keeps the configured endpoint"
    );

    let override_turn = runner
        .explain(
            PASSAGE,
            Recall::Bypass {
                provider: Some("onbox".to_string()),
            },
        )
        .expect("the override provider answers");

    assert_eq!(override_turn.artifact.provider, "onbox");
    assert!(
        override_turn.local,
        "the override's configured endpoint is on this machine"
    );
}

#[test]
fn a_bypass_with_an_unknown_provider_is_unavailable() {
    let dir = TempDir::new("runner-bypass-unknown");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);
    let endpoint = FakeEndpoint::accepting();
    let mut runner = open(&paths, endpoint.clone(), now());

    let failure = match runner.explain(
        PASSAGE,
        Recall::Bypass {
            provider: Some("no-such-provider".to_string()),
        },
    ) {
        Err(TurnFailure::Unavailable { provider, error }) => {
            assert_eq!(provider, "no-such-provider");
            assert!(
                matches!(error, UnavailableError::Setup(SetupError::Unknown { .. })),
                "got {error:?}"
            );
            error
        }
        other => panic!("expected an unavailable provider, got {other:?}"),
    };
    let UnavailableError::Setup(SetupError::Unknown { name }) = failure else {
        panic!("expected an unknown setup, got {failure:?}");
    };
    assert_eq!(name, "no-such-provider");
    assert_eq!(
        endpoint.generations(),
        0,
        "an unresolvable provider is never asked"
    );
}

#[test]
fn a_bypass_with_an_unreadable_credential_is_unavailable() {
    let dir = TempDir::new("runner-bypass-unreadable");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);
    let endpoint = FakeEndpoint::accepting();
    // The configured stub is key-optional, so the broken keyring does not stop
    // the run; the bypass asks DeepSeek, whose key is required.
    let mut runner = Runner::open_at(paths, broken_secrets(), PromptChoice::Configured, now())
        .expect("the configured key-optional provider opens")
        .with_transport(Box::new({
            let endpoint = endpoint.clone();
            move |_, _| Box::new(endpoint.clone())
        }));

    let failure = match runner.explain(
        PASSAGE,
        Recall::Bypass {
            provider: Some("deepseek".to_string()),
        },
    ) {
        Err(TurnFailure::Unavailable { provider, error }) => {
            assert_eq!(provider, "deepseek");
            assert!(
                matches!(
                    error,
                    UnavailableError::Credential(CredentialError::Unreadable { .. })
                ),
                "got {error:?}"
            );
            error
        }
        other => panic!("expected an unavailable credential, got {other:?}"),
    };
    let UnavailableError::Credential(CredentialError::Unreadable { provider, .. }) = failure else {
        panic!("expected an unreadable credential, got {failure:?}");
    };
    assert_eq!(provider, "deepseek");
    assert_eq!(
        endpoint.generations(),
        0,
        "a provider without its required key is never asked"
    );
    assert_eq!(
        runner.notices().len(),
        1,
        "the configured provider's own notice is still the only one"
    );
}

#[test]
fn an_answer_that_cannot_be_stored_is_kept_and_reported_as_not_stored() {
    let dir = TempDir::new("runner-not-stored");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);
    let endpoint = FakeEndpoint::accepting();
    let mut runner = open(&paths, endpoint.clone(), now());
    refuse_writes(&paths);

    let turn = runner
        .explain(PASSAGE, Recall::Reuse)
        .expect("an unstored answer is not a failed turn");

    assert_eq!(endpoint.generations(), 1, "the provider still answered");
    assert_eq!(
        turn.artifact.passage, PASSAGE,
        "the answer is not thrown away"
    );
    assert!(turn.record.is_none(), "there is no stored row to report");
    assert_eq!(turn.notices.len(), 1, "the failure is on the turn");
    let TurnNotice::NotStored { reason } = &turn.notices[0];
    assert!(
        reason.contains("refused the write"),
        "the store's own reason is kept, got {reason:?}"
    );
    assert!(!turn.from_cache);
    assert!(turn.resolution.is_some(), "the run itself succeeded");
}

/// Make the store's next write fail at the storage layer.
///
/// A directory turned read-only after open does not do it: SQLite in WAL mode
/// writes through the `-wal` file descriptor it already holds. A trigger is the
/// same failure at the same seam — a real write reaches SQLite and is refused —
/// without adding a `Store` trait for the test.
fn refuse_writes(paths: &Paths) {
    let conn = rusqlite::Connection::open(paths.history_file()).expect("the store opens");
    conn.execute_batch(
        "CREATE TRIGGER refuse_write BEFORE INSERT ON records \
         BEGIN SELECT RAISE(ABORT, 'the history refused the write'); END;",
    )
    .expect("the refusing trigger is installable");
}

#[test]
fn a_failed_run_keeps_the_probe_and_the_downgrade_it_learned() {
    let dir = TempDir::new("runner-run-failure");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);
    let endpoint = FakeEndpoint::accepting()
        // The first probe accepts the strong shape; the re-probe walks down to
        // the best-effort one, which is what a real endpoint does when it
        // rejects the schema.
        .probes([Ok(()), Err(unsupported()), Ok(())])
        .answers([
            Err(unsupported()),
            Err(ProviderError::misconfigured("the model is not there")),
        ]);
    let mut runner = open(&paths, endpoint.clone(), now());

    let failure = match runner.explain(PASSAGE, Recall::Reuse) {
        Err(TurnFailure::Run(failure)) => failure,
        other => panic!("expected a failed run, got {other:?}"),
    };

    assert_eq!(failure.failure.kind, FailureKind::Misconfigured);
    assert_eq!(
        failure.resolution.capability.schema,
        SchemaSupport::BestEffort,
        "the reported capability is the one the last attempt was sent under"
    );
    assert_eq!(failure.resolution.origin, Origin::Probed);
    let downgrade = failure.downgrade.expect("the rejected shape is reported");
    assert_eq!(downgrade.from.schema, SchemaSupport::Enforced);
    assert!(
        failure.reprobe.is_none(),
        "the re-probe changed the request, so it is not a failed second look"
    );
    assert_eq!(endpoint.generations(), 2, "the Passage was asked for twice");
}

#[test]
fn the_factory_prompt_choice_leaves_the_configured_appendix_out() {
    let dir = TempDir::new("runner-factory-prompt");
    let paths = paths(&dir);
    write_config(
        &paths,
        &format!("{CONFIG}\n[prompts]\nappendix = \"A rule the user wrote.\"\n"),
    );
    let endpoint = FakeEndpoint::accepting();
    let mut runner = Runner::open_at(paths, secrets(), PromptChoice::Factory, now())
        .expect("the fixture opens")
        .with_transport(Box::new({
            let endpoint = endpoint.clone();
            move |_, _| Box::new(endpoint.clone())
        }));

    runner
        .explain(PASSAGE, Recall::Reuse)
        .expect("the fixture answers");

    let request = &endpoint.requests()[0];
    assert_eq!(
        request.prompt_version,
        Prompt::factory().version(),
        "the request is stamped with the factory prompt, not the configured one"
    );
    assert!(
        !request.system_prompt.contains("A rule the user wrote."),
        "--factory-prompt is the factory text, without the appendix"
    );
}

#[test]
fn a_miss_asks_the_provider_once_and_stores_the_answer() {
    let dir = TempDir::new("runner-miss");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);
    let endpoint = FakeEndpoint::accepting();
    let mut runner = open(&paths, endpoint.clone(), now());

    let turn = runner
        .explain(PASSAGE, Recall::Reuse)
        .expect("the fixture answers");

    assert_eq!(
        endpoint.generations(),
        1,
        "the provider is asked exactly once"
    );
    assert!(!turn.from_cache);
    assert_eq!(turn.artifact.passage, PASSAGE);
    assert_eq!(turn.artifact.provider, "stub");
    assert_eq!(turn.artifact.model, "stub-model");
    assert!(
        turn.resolution.is_some(),
        "a generated answer resolves a capability"
    );
    assert!(turn.downgrade.is_none(), "nothing was rejected");

    let record = turn
        .record
        .clone()
        .expect("a generated answer is remembered");
    assert_eq!(record.artifact, turn.artifact);

    let store = Store::open(paths.history_file()).expect("the store reopens");
    assert_eq!(
        store.show(record.id).expect("the store reads"),
        Some(record),
        "the answer reached the history"
    );
}

#[test]
fn cached_answers_with_the_record_a_reuse_turn_would_recall() {
    let dir = TempDir::new("runner-cached");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);

    let runner = open(&paths, FakeEndpoint::accepting(), now());
    assert!(
        runner.cached(PASSAGE).expect("the store reads").is_none(),
        "nothing is stored before the first turn"
    );

    open(&paths, FakeEndpoint::accepting(), now())
        .explain(PASSAGE, Recall::Reuse)
        .expect("the fixture answers")
        .record
        .expect("the answer was stored");

    let mut runner = open(&paths, FakeEndpoint::accepting(), later());
    let cached = runner
        .cached(PASSAGE)
        .expect("the stored answer is recalled")
        .expect("the stored answer is there");
    assert_eq!(cached.artifact.passage, PASSAGE);
    let turn = runner
        .explain(PASSAGE, Recall::Reuse)
        .expect("the turn agrees with the plan");
    assert_eq!(
        turn.record.map(|record| record.artifact),
        Some(cached.artifact),
        "the plan and the turn recall the same answer"
    );
    assert!(
        runner
            .cached("A different passage.")
            .expect("the store reads")
            .is_none(),
        "a different Passage was never asked"
    );
}

#[test]
fn a_history_that_cannot_be_read_fails_the_turn_without_asking_the_provider() {
    let dir = TempDir::new("runner-store-read-error");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);
    let endpoint = FakeEndpoint::accepting();
    let mut runner = open(&paths, endpoint.clone(), now());
    hide_records(&paths);

    assert!(
        runner.cached(PASSAGE).is_err(),
        "a store read failure is not a cache miss"
    );

    let error = match runner.explain(PASSAGE, Recall::Reuse) {
        Err(TurnFailure::Store(error)) => error,
        other => panic!("expected a store failure, got {other:?}"),
    };
    assert!(
        matches!(error, StoreError::Sqlite { .. }),
        "the store's own reason is kept, got {error:?}"
    );
    assert_eq!(
        endpoint.generations(),
        0,
        "a failed recall must not buy a generation"
    );
    assert_eq!(
        runner.notices().len(),
        0,
        "a read failure is not a run notice"
    );
}

/// Make the store's next read fail after it has been opened.
///
/// Renaming the table out from under the open connection is a real read error at
/// the same seam a damaged or locked store hits: SQLite answers "no such table"
/// where the row should be. A write trigger would not do — this test has to fail
/// the recall, before the provider is ever reached.
fn hide_records(paths: &Paths) {
    let conn = rusqlite::Connection::open(paths.history_file()).expect("the store opens");
    conn.execute_batch("ALTER TABLE records RENAME TO records_hidden;")
        .expect("the records table is renamable");
}

#[test]
fn the_lookup_key_is_stable_and_distinguishes_passages() {
    let dir = TempDir::new("runner-lookup-stable");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);
    let runner = open(&paths, FakeEndpoint::accepting(), now());

    let key = runner.lookup(PASSAGE);

    assert_eq!(
        key,
        runner.lookup(PASSAGE),
        "the same Passage asks the same question"
    );
    assert_ne!(
        key,
        runner.lookup("A different passage."),
        "a different Passage is a different question"
    );
}

#[test]
fn the_lookup_key_agrees_with_the_key_a_reuse_turn_uses() {
    let dir = TempDir::new("runner-lookup-agrees");
    let paths = paths(&dir);
    write_config(&paths, CONFIG);

    let stored = open(&paths, FakeEndpoint::accepting(), now())
        .explain(PASSAGE, Recall::Reuse)
        .expect("the fixture answers")
        .record
        .expect("the answer was stored");

    let planned = open(&paths, FakeEndpoint::accepting(), later()).lookup(PASSAGE);
    let store = Store::open(paths.history_file()).expect("the store reopens");
    let recalled = store
        .recall(&planned, later())
        .expect("the store reads")
        .expect("the planned key recalls the stored answer");
    assert_eq!(recalled.id, stored.id, "the planned key found the same row");
    assert_eq!(recalled.artifact, stored.artifact);
}

#[test]
fn the_level_is_the_one_the_configuration_snapshot_resolved() {
    let dir = TempDir::new("runner-level");
    let paths = paths(&dir);
    // A1 is not the default, so this cannot pass by the default alone.
    write_config(
        &paths,
        "[app]\nprovider = \"stub\"\nlevel = \"A1\"\n\n\
         [providers.stub]\nendpoint = \"http://127.0.0.1:9/v1\"\n\
         model = \"stub-model\"\n",
    );
    let runner = open(&paths, FakeEndpoint::accepting(), now());

    assert_ne!(Level::default(), Level::A1);
    assert_eq!(runner.level(), Level::A1);
}
