//! Provider readiness: whether one provider can be used right now, and why not.
//!
//! Everything here is driven through [`Readiness`] and the pure `local_of`
//! function — the seam the desktop, the panel and the CLI read — with injected
//! secret stores. There is no network, no display and no real keyring: these
//! tests keep their own local `SecretStore` fakes (the runner and secrets test
//! binaries keep theirs), because each integration-test binary is its own seam.

use std::collections::BTreeMap;
use std::sync::Mutex;

use plainly_core::{
    Config, CredentialError, CredentialState, EnvSecrets, KeyPresence, KeyRequirement, KeySource,
    ProviderSetup, Readiness, SecretError, SecretStore, Secrets, SetupError, local_of,
    resolve_credential,
};

fn config(text: &str) -> Config {
    Config::parse(text).expect("the fixture is valid TOML")
}

/// A keyring we control: it can hold a key, or refuse to answer.
#[derive(Default)]
struct FakeKeyring {
    entries: Mutex<BTreeMap<String, String>>,
    fail_reads: bool,
}

impl FakeKeyring {
    fn holding(provider: &str, secret: &str) -> Box<dyn SecretStore> {
        let keyring = Self::default();
        keyring
            .entries
            .lock()
            .expect("not poisoned")
            .insert(provider.to_string(), secret.to_string());
        Box::new(keyring)
    }

    /// No session bus at all: every read fails.
    fn broken() -> Box<dyn SecretStore> {
        Box::new(Self {
            fail_reads: true,
            ..Self::default()
        })
    }
}

impl SecretStore for FakeKeyring {
    fn get(&self, provider: &str) -> Result<Option<String>, SecretError> {
        if self.fail_reads {
            return Err(SecretError::Unavailable("no session bus".to_string()));
        }
        Ok(self
            .entries
            .lock()
            .expect("not poisoned")
            .get(provider)
            .cloned())
    }

    fn set(&self, provider: &str, secret: &str) -> Result<(), SecretError> {
        self.entries
            .lock()
            .expect("not poisoned")
            .insert(provider.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, _provider: &str) -> Result<(), SecretError> {
        Ok(())
    }
}

#[test]
fn a_required_key_found_in_the_environment_is_usable() {
    let config = config("[app]\nprovider = \"deepseek\"\n");
    let secrets = Secrets::with_stores(
        EnvSecrets::from_pairs([("PLAINLY_DEEPSEEK_API_KEY", "sk-env")]),
        None,
    );

    let readiness = Readiness::of("deepseek", &config, &secrets);

    assert_eq!(readiness.name, "deepseek");
    assert_eq!(readiness.requirement, KeyRequirement::Required);
    assert!(readiness.resolution.is_ok(), "the preset resolves");
    assert!(!readiness.local, "the preset's endpoint is hosted");
    assert_eq!(readiness.key, KeyPresence::Found(KeySource::Environment));
    assert!(readiness.usable());
}

/// The tier is part of the answer: a surface has to tell the user whether the
/// key will still be there after a restart.
#[test]
fn a_required_key_is_usable_from_any_tier_and_names_the_tier() {
    let config = config("[app]\nprovider = \"deepseek\"\n");

    let env = Secrets::with_stores(
        EnvSecrets::from_pairs([("PLAINLY_DEEPSEEK_API_KEY", "sk-env")]),
        None,
    );
    let keyring = Secrets::with_stores(
        EnvSecrets::empty(),
        Some(FakeKeyring::holding("deepseek", "sk")),
    );
    let session = Secrets::with_stores(EnvSecrets::empty(), None);
    session.store("deepseek", "typed-just-now");

    for (secrets, source) in [
        (env, KeySource::Environment),
        (keyring, KeySource::Keyring),
        (session, KeySource::Session),
    ] {
        let readiness = Readiness::of("deepseek", &config, &secrets);
        assert!(readiness.usable(), "a found key is a key: {source:?}");
        assert_eq!(readiness.key, KeyPresence::Found(source));
    }
}

/// Missing and unreadable are different facts: one means "set a key", the other
/// "the place the key should be is broken". Readiness reports both as unusable
/// for a required provider, and never folds one into the other.
#[test]
fn a_required_key_that_is_missing_or_unreadable_is_unusable_and_the_two_stay_distinct() {
    let config = config("[app]\nprovider = \"deepseek\"\n");
    let missing = Secrets::with_stores(EnvSecrets::empty(), None);
    let unreadable = Secrets::with_stores(EnvSecrets::empty(), Some(FakeKeyring::broken()));

    let no_key = Readiness::of("deepseek", &config, &missing);
    assert!(!no_key.usable());
    assert_eq!(no_key.key, KeyPresence::Missing);

    let broken = Readiness::of("deepseek", &config, &unreadable);
    assert!(!broken.usable());
    assert!(
        matches!(
            &broken.key,
            KeyPresence::Unreadable(SecretError::Unavailable(reason))
                if reason.contains("no session bus")
        ),
        "the store's own reason is kept, got {:?}",
        broken.key
    );
}

/// A provider that may authenticate nothing is not blocked by a store that
/// cannot answer; the reason still travels, because it is not the same fact as
/// having no key.
#[test]
fn an_optional_key_is_usable_when_missing_or_unreadable_and_keeps_the_reason() {
    let config =
        config("[providers.stub]\nendpoint = \"http://127.0.0.1:9/v1\"\nmodel = \"stub-model\"\n");
    let missing = Secrets::with_stores(EnvSecrets::empty(), None);
    let unreadable = Secrets::with_stores(EnvSecrets::empty(), Some(FakeKeyring::broken()));

    let no_key = Readiness::of("stub", &config, &missing);
    assert_eq!(no_key.requirement, KeyRequirement::Optional);
    assert!(no_key.usable(), "an optional provider needs no key");
    assert_eq!(no_key.key, KeyPresence::Missing);

    let broken = Readiness::of("stub", &config, &unreadable);
    assert_eq!(broken.requirement, KeyRequirement::Optional);
    assert!(broken.usable(), "an unreadable store does not block it");
    assert!(
        matches!(&broken.key, KeyPresence::Unreadable(_)),
        "the unreadable reason is kept, got {:?}",
        broken.key
    );
}

/// Every reason a provider cannot be used is its own fact, and none of them is
/// "there is no such provider" once a preset or a table names one.
#[test]
fn a_provider_that_does_not_resolve_is_unusable_whatever_the_key_says() {
    let secrets = Secrets::with_stores(EnvSecrets::empty(), None);

    let unknown = Readiness::of("no-such-provider", &Config::default(), &secrets);
    assert!(!unknown.usable());
    assert!(
        matches!(unknown.resolution, Err(SetupError::Unknown { .. })),
        "got {:?}",
        unknown.resolution
    );
    assert!(
        !unknown.local,
        "an unknown name has no endpoint to be local"
    );

    let no_endpoint = Readiness::of(
        "mine",
        &config("[providers.mine]\nmodel = \"some-model\"\n"),
        &secrets,
    );
    assert!(!no_endpoint.usable());
    assert!(
        matches!(no_endpoint.resolution, Err(SetupError::NoEndpoint { .. })),
        "got {:?}",
        no_endpoint.resolution
    );
    assert!(!no_endpoint.local, "nowhere to send is not on this machine");

    let no_model = Readiness::of("llamacpp", &Config::default(), &secrets);
    assert!(!no_model.usable());
    assert!(
        matches!(no_model.resolution, Err(SetupError::NoModel { .. })),
        "got {:?}",
        no_model.resolution
    );
    assert!(
        no_model.local,
        "a local preset is on this machine even while its model is chosen"
    );
}

/// A provider that cannot resolve cannot run whatever the stores would say, and
/// no surface draws its key. Readiness therefore must not turn a settings-screen
/// render into a keyring round trip for such a name.
#[test]
fn a_provider_that_does_not_resolve_does_not_ask_the_stores() {
    /// A store that fails the test if it is asked at all.
    struct NeverAsked;

    impl SecretStore for NeverAsked {
        fn get(&self, _provider: &str) -> Result<Option<String>, SecretError> {
            panic!("a provider that cannot resolve must not ask the secret stores")
        }

        fn set(&self, _provider: &str, _secret: &str) -> Result<(), SecretError> {
            unreachable!("this test never stores a key")
        }

        fn delete(&self, _provider: &str) -> Result<(), SecretError> {
            unreachable!("this test never clears a key")
        }
    }

    let secrets = Secrets::with_stores(EnvSecrets::empty(), Some(Box::new(NeverAsked)));

    for (name, fixture) in [
        ("no-such-provider", Config::default()),
        ("llamacpp", Config::default()),
        ("mine", config("[providers.mine]\nmodel = \"some-model\"\n")),
    ] {
        let readiness = Readiness::of(name, &fixture, &secrets);
        assert!(
            readiness.resolution.is_err(),
            "{name} is a fixture that must not resolve"
        );
        assert_eq!(
            readiness.key,
            KeyPresence::Missing,
            "{name}: no store was asked, so no key state is claimed"
        );
        assert!(!readiness.usable());
    }
}

/// Pointing a name elsewhere withdraws what its preset knew. Readiness answers
/// from the endpoint the configuration names: a hosted preset pointed at loopback
/// is local and asks for no key, and one pointed at another service does not
/// inherit the preset's demand for one either.
#[test]
fn an_endpoint_override_moves_both_the_requirement_and_the_locality() {
    let secrets = Secrets::with_stores(EnvSecrets::empty(), None);

    let onbox = Readiness::of(
        "deepseek",
        &config("[providers.deepseek]\nendpoint = \"http://127.0.0.1:11434/v1\"\n"),
        &secrets,
    );
    assert_eq!(onbox.requirement, KeyRequirement::Optional);
    assert!(onbox.local);
    assert!(onbox.usable(), "a loopback endpoint needs no key");

    let elsewhere = Readiness::of(
        "deepseek",
        &config("[providers.deepseek]\nendpoint = \"https://api.example.invalid/v1\"\n"),
        &secrets,
    );
    assert_eq!(
        elsewhere.requirement,
        KeyRequirement::Optional,
        "the preset's Required applies to the service it names, not this one"
    );
    assert!(!elsewhere.local);
    assert!(
        elsewhere.usable(),
        "the endpoint the configuration names demands no key"
    );
}

/// The pure locality question, without touching a secret store: endpoints that
/// loop back are local, everything else (a LAN box, another service, an unknown
/// name) is not.
#[test]
fn local_of_answers_from_the_configured_endpoint() {
    let config = config(
        r#"
        [providers.named-host]
        endpoint = "http://localhost:1/v1"
        model = "m"

        [providers.loopback-v4]
        endpoint = "http://127.0.0.9:2/v1"
        model = "m"

        [providers.loopback-v6]
        endpoint = "http://[::1]:3/v1"
        model = "m"

        [providers.lan]
        endpoint = "http://192.168.1.5:4/v1"
        model = "m"

        [providers.dns]
        endpoint = "https://api.example.invalid/v1"
        model = "m"
        "#,
    );

    for name in ["named-host", "loopback-v4", "loopback-v6"] {
        assert!(local_of(name, &config), "{name} is on this machine");
    }
    for name in ["lan", "dns", "no-such-provider"] {
        assert!(!local_of(name, &config), "{name} is not on this machine");
    }

    assert!(local_of("ollama", &Config::default()));
    assert!(!local_of("deepseek", &Config::default()));
}

/// The run path and the display path are two projections of one judgement. For
/// every requirement and key state they must reach the same conclusion: a
/// required key that is missing or unreadable stops a run, an optional one does
/// not, and the unreadable reason is the same on both sides.
#[test]
fn readiness_and_the_run_path_agree_on_every_requirement_and_key_state() {
    let required = || config("[app]\nprovider = \"deepseek\"\n");
    let optional = || {
        config("[providers.stub]\nendpoint = \"http://127.0.0.1:9/v1\"\nmodel = \"stub-model\"\n")
    };
    let session = Secrets::with_stores(EnvSecrets::empty(), None);
    session.store("deepseek", "sk-session");

    let cases = [
        (
            required(),
            "deepseek",
            Secrets::with_stores(
                EnvSecrets::from_pairs([("PLAINLY_DEEPSEEK_API_KEY", "sk-env")]),
                None,
            ),
        ),
        (
            required(),
            "deepseek",
            Secrets::with_stores(
                EnvSecrets::empty(),
                Some(FakeKeyring::holding("deepseek", "sk-keyring")),
            ),
        ),
        (required(), "deepseek", session),
        (
            required(),
            "deepseek",
            Secrets::with_stores(EnvSecrets::empty(), None),
        ),
        (
            required(),
            "deepseek",
            Secrets::with_stores(EnvSecrets::empty(), Some(FakeKeyring::broken())),
        ),
        (
            optional(),
            "stub",
            Secrets::with_stores(
                EnvSecrets::from_pairs([("PLAINLY_STUB_API_KEY", "sk-env")]),
                None,
            ),
        ),
        (
            optional(),
            "stub",
            Secrets::with_stores(EnvSecrets::empty(), None),
        ),
        (
            optional(),
            "stub",
            Secrets::with_stores(EnvSecrets::empty(), Some(FakeKeyring::broken())),
        ),
    ];

    for (config, name, secrets) in cases {
        let setup = ProviderSetup::resolve(name, &config).expect("the fixture resolves");
        let readiness = Readiness::of(name, &config, &secrets);
        let credential = resolve_credential(&setup, &secrets);

        assert_eq!(
            readiness.usable(),
            credential.is_ok(),
            "{name}: the display path and the run path disagree about usability"
        );
        match (&readiness.key, &credential) {
            (KeyPresence::Found(_), Ok(CredentialState::Key(_))) => {}
            (KeyPresence::Missing, Ok(CredentialState::NoKey)) => {}
            (KeyPresence::Missing, Err(CredentialError::Missing { .. })) => {}
            (KeyPresence::Unreadable(shown), Ok(CredentialState::Unreadable(run))) => {
                assert_eq!(shown, run, "{name}: the same store reason, both sides")
            }
            (KeyPresence::Unreadable(shown), Err(CredentialError::Unreadable { source, .. })) => {
                assert_eq!(shown, source, "{name}: the same store reason, both sides")
            }
            (shown, run) => {
                panic!("{name}: readiness's {shown:?} is not the run path's {run:?}")
            }
        }
    }
}
