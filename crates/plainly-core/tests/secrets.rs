//! API key resolution: environment, then keyring, then session memory — and
//! never a plaintext file.

use std::collections::BTreeMap;
use std::sync::Mutex;

use plainly_core::{
    Cleared, EnvSecrets, KeySource, MemorySecrets, SecretError, SecretStore, Secrets, Stored,
    env_var_name,
};

/// A keyring we control: it can hold keys, refuse to answer, or refuse to be
/// written to (a read-only collection is a real state, not a hypothetical).
#[derive(Default)]
struct FakeKeyring {
    entries: Mutex<BTreeMap<String, String>>,
    fail_reads: bool,
    write_error: Option<SecretError>,
}

impl FakeKeyring {
    fn working() -> Box<dyn SecretStore> {
        Box::new(Self::default())
    }

    /// No session bus at all: reads and writes both fail.
    fn broken() -> Box<dyn SecretStore> {
        Box::new(Self {
            fail_reads: true,
            write_error: Some(SecretError::Unavailable("no session bus".to_string())),
            ..Self::default()
        })
    }

    /// Reads answer honestly with "no entry"; writes are refused, as they are on
    /// a read-only collection.
    fn read_only() -> Box<dyn SecretStore> {
        Box::new(Self {
            write_error: Some(SecretError::Backend(
                "the collection is read-only".to_string(),
            )),
            ..Self::default()
        })
    }

    fn holding(provider: &str, secret: &str) -> Box<dyn SecretStore> {
        let keyring = Self::default();
        keyring
            .entries
            .lock()
            .expect("not poisoned")
            .insert(provider.to_string(), secret.to_string());
        Box::new(keyring)
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
        if let Some(error) = &self.write_error {
            return Err(error.clone());
        }
        self.entries
            .lock()
            .expect("not poisoned")
            .insert(provider.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, provider: &str) -> Result<(), SecretError> {
        if let Some(error) = &self.write_error {
            return Err(error.clone());
        }
        self.entries.lock().expect("not poisoned").remove(provider);
        Ok(())
    }
}

#[test]
fn the_environment_variable_is_named_after_the_provider() {
    assert_eq!(env_var_name("deepseek"), "PLAINLY_DEEPSEEK_API_KEY");
    assert_eq!(env_var_name("lm-studio"), "PLAINLY_LM_STUDIO_API_KEY");
    assert_eq!(env_var_name("llama.cpp"), "PLAINLY_LLAMA_CPP_API_KEY");
}

#[test]
fn the_environment_outranks_the_keyring() {
    let env = EnvSecrets::from_pairs([("PLAINLY_DEEPSEEK_API_KEY", "from-env")]);
    let secrets = Secrets::with_stores(env, Some(FakeKeyring::holding("deepseek", "from-keyring")));

    let key = secrets
        .resolve("deepseek")
        .expect("the environment is readable")
        .expect("a key is found");

    assert_eq!(key.secret, "from-env");
    assert_eq!(key.source, KeySource::Environment);
}

#[test]
fn the_keyring_is_used_when_the_environment_is_silent() {
    let secrets = Secrets::with_stores(
        EnvSecrets::empty(),
        Some(FakeKeyring::holding("deepseek", "k")),
    );

    let key = secrets
        .resolve("deepseek")
        .expect("the environment is readable")
        .expect("a key is found");

    assert_eq!(key.secret, "k");
    assert_eq!(key.source, KeySource::Keyring);
}

#[test]
fn a_working_keyring_without_an_entry_falls_through_to_session_memory() {
    // A read-only collection: reads answer "no entry", writes are refused, so
    // the key lands in memory and resolution must go looking for it there.
    let secrets = Secrets::with_stores(EnvSecrets::empty(), Some(FakeKeyring::read_only()));
    secrets.store("deepseek", "typed-just-now");

    assert_eq!(
        secrets.key_source("deepseek").expect("no store failed"),
        Some(KeySource::Session)
    );
}

#[test]
fn session_memory_is_the_last_resort() {
    let secrets = Secrets::with_stores(EnvSecrets::empty(), None);
    secrets.store("deepseek", "typed-just-now");

    let key = secrets
        .resolve("deepseek")
        .expect("the environment is readable")
        .expect("a key is found");

    assert_eq!(key.secret, "typed-just-now");
    assert_eq!(key.source, KeySource::Session);
}

#[test]
fn an_empty_environment_variable_is_not_a_key() {
    let env = EnvSecrets::from_pairs([("PLAINLY_DEEPSEEK_API_KEY", "")]);
    let secrets = Secrets::with_stores(env, Some(FakeKeyring::holding("deepseek", "k")));

    let key = secrets
        .resolve("deepseek")
        .expect("the environment is readable")
        .expect("a key is found");
    assert_eq!(key.source, KeySource::Keyring);
}

#[test]
fn a_missing_key_resolves_to_nothing() {
    let secrets = Secrets::with_stores(EnvSecrets::empty(), Some(FakeKeyring::working()));

    assert!(
        secrets
            .resolve("openai")
            .expect("the environment is readable")
            .is_none()
    );
    assert!(
        secrets
            .key_source("openai")
            .expect("the environment is readable")
            .is_none()
    );
}

/// A store that cannot answer says so. Swallowing the failure would make
/// "nowhere has a key" and "the keyring will not open" look identical to the
/// caller, and those call for different things from the user.
#[test]
fn a_broken_keyring_is_reported_rather_than_skipped() {
    let secrets = Secrets::with_stores(EnvSecrets::empty(), Some(FakeKeyring::broken()));
    let _ = secrets.store("deepseek", "a key in memory");

    let error = secrets
        .resolve("deepseek")
        .expect_err("the failure must reach the caller");

    assert_eq!(
        error,
        SecretError::Unavailable("no session bus".to_string())
    );
}

/// An environment key still outranks a broken keyring: the tiers are asked in
/// order, and the first answer wins.
#[test]
fn an_exported_key_is_found_before_the_keyring_is_consulted() {
    let secrets = Secrets::with_stores(
        EnvSecrets::from_pairs([("PLAINLY_OPENAI_API_KEY", "sk-env")]),
        Some(FakeKeyring::broken()),
    );

    let key = secrets
        .resolve("openai")
        .expect("the environment answers first")
        .expect("a key is found");

    assert_eq!(key.source, KeySource::Environment);
}

#[test]
fn storing_a_key_prefers_the_keyring_and_reports_it() {
    let secrets = Secrets::with_stores(EnvSecrets::empty(), Some(FakeKeyring::working()));

    assert_eq!(secrets.store("openai", "sk-test"), Stored::Keyring);
    assert_eq!(
        secrets
            .key_source("openai")
            .expect("the environment is readable"),
        Some(KeySource::Keyring)
    );
}

/// With no keyring, the honest outcome is a warning that names the way to supply
/// the key for this machine instead.
#[test]
fn storing_a_key_without_a_keyring_warns_and_names_the_environment_variable() {
    let secrets = Secrets::with_stores(EnvSecrets::empty(), None);

    let Stored::Session { warning } = secrets.store("openai", "sk-test") else {
        panic!("expected the session fallback");
    };

    assert!(warning.contains("NOT saved"), "{warning}");
    assert!(warning.contains("PLAINLY_OPENAI_API_KEY"), "{warning}");
}

/// The keyring is skipped entirely when the environment says so — for headless
/// machines where a D-Bus call would only fail.
#[test]
fn the_keyring_can_be_switched_off_from_the_environment() {
    let off = EnvSecrets::from_pairs([("PLAINLY_DISABLE_KEYRING", "1")]);
    let also_off = EnvSecrets::from_pairs([("PLAINLY_DISABLE_KEYRING", "true")]);
    let on = EnvSecrets::from_pairs([("PLAINLY_DISABLE_KEYRING", "0")]);
    let unset = EnvSecrets::empty();

    assert!(off.keyring_disabled());
    assert!(also_off.keyring_disabled());
    assert!(!on.keyring_disabled());
    assert!(!unset.keyring_disabled());
}

#[test]
fn clearing_a_key_removes_it_from_the_keyring() {
    let secrets = Secrets::with_stores(
        EnvSecrets::empty(),
        Some(FakeKeyring::holding("openai", "k")),
    );
    assert_eq!(
        secrets
            .resolve("openai")
            .expect("the environment is readable")
            .map(|k| k.secret),
        Some("k".into())
    );

    assert_eq!(secrets.clear("openai"), Ok(Cleared::Keyring));
    assert!(
        secrets
            .resolve("openai")
            .expect("the environment is readable")
            .is_none()
    );
}

#[test]
fn clearing_without_a_keyring_says_there_was_nothing_to_clear() {
    let secrets = Secrets::with_stores(EnvSecrets::empty(), None);
    assert_eq!(secrets.clear("openai"), Ok(Cleared::Nothing));
}

#[test]
fn clearing_surfaces_a_broken_keyring_rather_than_pretending_it_worked() {
    let secrets = Secrets::with_stores(EnvSecrets::empty(), Some(FakeKeyring::broken()));
    assert!(secrets.clear("openai").is_err());
}

/// The session tier is real memory, not a no-op: what goes in comes back out.
#[test]
fn session_memory_holds_and_releases_keys() {
    let session = MemorySecrets::new();
    assert_eq!(session.get("openai"), Ok(None));

    session.set("openai", "k").expect("memory accepts a write");
    assert_eq!(session.get("openai"), Ok(Some("k".to_string())));

    session.delete("openai").expect("memory accepts a delete");
    assert_eq!(session.get("openai"), Ok(None));
}

/// A keyring that is present but broken must be named, not flattened into "no
/// keyring": a locked collection and a missing session bus send the user to
/// different places.
#[test]
fn a_broken_keyring_travels_with_its_reason() {
    let secrets = Secrets::with_stores(EnvSecrets::empty(), Some(FakeKeyring::broken()));

    let Stored::Session { warning } = secrets.store("openai", "sk-test") else {
        panic!("expected the session fallback");
    };

    assert!(warning.contains("no session bus"), "{warning}");
    assert!(warning.contains("lives only in this process"), "{warning}");
    assert!(warning.contains("PLAINLY_OPENAI_API_KEY"), "{warning}");
}

/// The precedence note asks the environment only: it must not touch the
/// keyring, which is the tier that costs a D-Bus round trip.
#[test]
fn the_environment_check_ignores_the_other_tiers() {
    let secrets = Secrets::with_stores(
        EnvSecrets::from_pairs([("PLAINLY_OPENAI_API_KEY", "sk-env")]),
        Some(FakeKeyring::holding("deepseek", "k")),
    );

    assert!(secrets.environment_provides("openai"));
    assert!(!secrets.environment_provides("deepseek"));
}

/// An environment value that is not valid UTF-8 cannot be a secret. Rewriting
/// its bytes as U+FFFD would hand the provider a different key than the one that
/// was exported, and the failure would surface later as a bare 401.
#[cfg(unix)]
#[test]
fn a_non_utf8_environment_key_is_refused_rather_than_mangled() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let secrets = Secrets::with_stores(
        EnvSecrets::from_pairs([(
            "PLAINLY_OPENAI_API_KEY",
            OsString::from_vec(b"sk-\xff\xfe".to_vec()),
        )]),
        Some(FakeKeyring::holding("openai", "sk-good")),
    );

    let error = secrets
        .resolve("openai")
        .expect_err("an unusable key must be reported");

    assert_eq!(
        error,
        SecretError::NotUtf8 {
            variable: "PLAINLY_OPENAI_API_KEY".to_string()
        }
    );
    // The precedence note still sees that the variable is set, and says so,
    // without pretending to know whether its value is usable.
    assert!(secrets.environment_provides("openai"));
}
