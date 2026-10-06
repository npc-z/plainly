//! API key resolution: environment, then keyring, then session memory — and
//! never a plaintext file.

use std::collections::BTreeMap;
use std::sync::Mutex;

use plainly_core::{
    Cleared, EnvSecrets, KeySource, MemorySecrets, SecretError, SecretStore, Secrets, Stored,
    env_var_name,
};

/// A keyring we control: it can hold keys, and it can be broken on purpose.
#[derive(Default)]
struct FakeKeyring {
    entries: Mutex<BTreeMap<String, String>>,
    fail: bool,
}

impl FakeKeyring {
    fn working() -> Box<dyn SecretStore> {
        Box::new(Self::default())
    }

    fn broken() -> Box<dyn SecretStore> {
        Box::new(Self {
            entries: Mutex::new(BTreeMap::new()),
            fail: true,
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
        if self.fail {
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
        if self.fail {
            return Err(SecretError::Unavailable("no session bus".to_string()));
        }
        self.entries
            .lock()
            .expect("not poisoned")
            .insert(provider.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, provider: &str) -> Result<(), SecretError> {
        if self.fail {
            return Err(SecretError::Unavailable("no session bus".to_string()));
        }
        self.entries.lock().expect("not poisoned").remove(provider);
        Ok(())
    }
}

fn secrets_with(env: EnvSecrets, keyring: Option<Box<dyn SecretStore>>) -> Secrets {
    Secrets::with_stores(env, keyring)
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
    let secrets = secrets_with(env, Some(FakeKeyring::holding("deepseek", "from-keyring")));

    let key = secrets.resolve("deepseek").expect("a key is found");

    assert_eq!(key.secret, "from-env");
    assert_eq!(key.source, KeySource::Environment);
}

#[test]
fn the_keyring_is_used_when_the_environment_is_silent() {
    let secrets = secrets_with(
        EnvSecrets::empty(),
        Some(FakeKeyring::holding("deepseek", "k")),
    );

    let key = secrets.resolve("deepseek").expect("a key is found");

    assert_eq!(key.secret, "k");
    assert_eq!(key.source, KeySource::Keyring);
}

#[test]
fn session_memory_is_the_last_resort() {
    let secrets = secrets_with(EnvSecrets::empty(), None);
    secrets.store("deepseek", "typed-just-now");

    let key = secrets.resolve("deepseek").expect("a key is found");

    assert_eq!(key.secret, "typed-just-now");
    assert_eq!(key.source, KeySource::Session);
}

#[test]
fn an_empty_environment_variable_is_not_a_key() {
    let env = EnvSecrets::from_pairs([("PLAINLY_DEEPSEEK_API_KEY", "")]);
    let secrets = secrets_with(env, Some(FakeKeyring::holding("deepseek", "k")));

    let key = secrets.resolve("deepseek").expect("a key is found");
    assert_eq!(key.source, KeySource::Keyring);
}

#[test]
fn a_missing_key_resolves_to_nothing() {
    let secrets = secrets_with(EnvSecrets::empty(), Some(FakeKeyring::working()));

    assert!(secrets.resolve("openai").is_none());
    assert!(secrets.key_source("openai").is_none());
}

/// A keyring that is present but broken must not block the other tiers: a
/// locked keychain is not a reason to refuse to work.
#[test]
fn a_broken_keyring_falls_through_to_session_memory() {
    let secrets = secrets_with(EnvSecrets::empty(), Some(FakeKeyring::broken()));
    let _ = secrets.store("deepseek", "still-usable");

    assert_eq!(secrets.key_source("deepseek"), Some(KeySource::Session));
}

#[test]
fn storing_a_key_prefers_the_keyring_and_reports_it() {
    let secrets = secrets_with(EnvSecrets::empty(), Some(FakeKeyring::working()));

    assert_eq!(secrets.store("openai", "sk-test"), Stored::Keyring);
    assert_eq!(secrets.key_source("openai"), Some(KeySource::Keyring));
}

/// With no keyring, the honest outcome is a warning that names the way to supply
/// the key for this machine instead.
#[test]
fn storing_a_key_without_a_keyring_warns_and_names_the_environment_variable() {
    let secrets = secrets_with(EnvSecrets::empty(), None);

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
    let secrets = secrets_with(
        EnvSecrets::empty(),
        Some(FakeKeyring::holding("openai", "k")),
    );
    assert_eq!(
        secrets.resolve("openai").map(|k| k.secret),
        Some("k".into())
    );

    assert_eq!(secrets.clear("openai"), Ok(Cleared::Keyring));
    assert!(secrets.resolve("openai").is_none());
}

#[test]
fn clearing_without_a_keyring_says_there_was_nothing_to_clear() {
    let secrets = secrets_with(EnvSecrets::empty(), None);
    assert_eq!(secrets.clear("openai"), Ok(Cleared::Nothing));
}

#[test]
fn clearing_surfaces_a_broken_keyring_rather_than_pretending_it_worked() {
    let secrets = secrets_with(EnvSecrets::empty(), Some(FakeKeyring::broken()));
    assert!(secrets.clear("openai").is_err());
}

/// The session tier is real memory, not a no-op: what goes in comes back out.
#[test]
fn session_memory_holds_and_releases_keys() {
    let session = MemorySecrets::new();
    assert!(!session.holds("openai"));

    session.set("openai", "k").expect("memory accepts a write");
    assert!(session.holds("openai"));
    assert_eq!(session.get("openai"), Ok(Some("k".to_string())));

    session.delete("openai").expect("memory accepts a delete");
    assert!(!session.holds("openai"));
}
