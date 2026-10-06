//! API keys, in three tiers: the environment, then the OS keyring, then this
//! session's memory — and never a plaintext file.
//!
//! The environment comes first for a reason that is not only about safety:
//! scripts and agents can hand a key to one invocation without touching the
//! user's keyring at all. The last tier exists so a machine with no usable
//! keyring is not simply blocked; it is told, in as many words, that the key
//! dies with the process.
//!
//! The keyring is reached through a [`SecretStore`] rather than directly, so
//! the precedence and the fallback are testable without a D-Bus session.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// The keyring *service* name; the provider name is the keyring *user*.
pub const KEYRING_SERVICE: &str = "plainly";

/// Set this (to `1`, `true`, `yes` or `on`) to skip the keyring entirely — for
/// headless machines where a D-Bus call would only ever fail, and for tests.
pub const DISABLE_KEYRING_VAR: &str = "PLAINLY_DISABLE_KEYRING";

/// The environment variable a provider's key can be supplied in.
pub fn env_var_name(provider: &str) -> String {
    let name: String = provider
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("PLAINLY_{name}_API_KEY")
}

/// Where a key came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Environment,
    Keyring,
    Session,
}

impl KeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            KeySource::Environment => "environment",
            KeySource::Keyring => "keyring",
            KeySource::Session => "this session only",
        }
    }
}

/// A key that was found, and where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKey {
    pub secret: String,
    pub source: KeySource,
}

/// What happened when a key was stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stored {
    Keyring,
    /// The keyring was unavailable, so the key is only in memory. The string is
    /// the sentence to show the user.
    Session {
        warning: String,
    },
}

/// What happened when a key was cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cleared {
    Keyring,
    Nothing,
}

/// Why a secret store failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    #[error("the OS keyring is unavailable: {0}")]
    Unavailable(String),
    #[error("the OS keyring failed: {0}")]
    Backend(String),
}

/// Somewhere a secret can live. Implementations: the environment (read-only),
/// the OS keyring, and process memory.
pub trait SecretStore: Send + Sync {
    fn get(&self, provider: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, provider: &str, secret: &str) -> Result<(), SecretError>;
    fn delete(&self, provider: &str) -> Result<(), SecretError>;
}

/// Keys supplied in the environment, captured as data.
#[derive(Debug, Clone, Default)]
pub struct EnvSecrets {
    vars: BTreeMap<String, String>,
}

impl EnvSecrets {
    /// An environment with nothing in it.
    pub fn empty() -> Self {
        Self::default()
    }

    /// An environment built from explicit pairs.
    pub fn from_pairs<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            vars: pairs
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    /// The real process environment.
    pub fn from_process_env() -> Self {
        Self {
            vars: std::env::vars().collect(),
        }
    }

    /// Whether the keyring has been switched off for this environment.
    pub fn keyring_disabled(&self) -> bool {
        self.vars
            .get(DISABLE_KEYRING_VAR)
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    }
}

impl SecretStore for EnvSecrets {
    fn get(&self, provider: &str) -> Result<Option<String>, SecretError> {
        Ok(self
            .vars
            .get(&env_var_name(provider))
            .filter(|value| !value.is_empty())
            .cloned())
    }

    fn set(&self, _provider: &str, _secret: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable(
            "the environment cannot be written to; export the variable instead".to_string(),
        ))
    }

    fn delete(&self, _provider: &str) -> Result<(), SecretError> {
        Ok(())
    }
}

/// Keys that live only as long as this process.
#[derive(Debug, Default)]
pub struct MemorySecrets {
    entries: Mutex<BTreeMap<String, String>>,
}

impl MemorySecrets {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether anything is held for this provider.
    pub fn holds(&self, provider: &str) -> bool {
        self.entries
            .lock()
            .expect("the session store is never poisoned")
            .contains_key(provider)
    }
}

impl SecretStore for MemorySecrets {
    fn get(&self, provider: &str) -> Result<Option<String>, SecretError> {
        Ok(self
            .entries
            .lock()
            .expect("the session store is never poisoned")
            .get(provider)
            .cloned())
    }

    fn set(&self, provider: &str, secret: &str) -> Result<(), SecretError> {
        self.entries
            .lock()
            .expect("the session store is never poisoned")
            .insert(provider.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, provider: &str) -> Result<(), SecretError> {
        self.entries
            .lock()
            .expect("the session store is never poisoned")
            .remove(provider);
        Ok(())
    }
}

/// The OS keyring, through the pure-Rust Secret Service client — no libsecret,
/// and nothing to install.
#[derive(Debug, Clone)]
pub struct KeyringSecrets {
    service: String,
}

impl Default for KeyringSecrets {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyringSecrets {
    pub fn new() -> Self {
        Self {
            service: KEYRING_SERVICE.to_string(),
        }
    }

    fn entry(&self, provider: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(&self.service, provider).map_err(|error| {
            // The most common failure by far is "there is no credential store
            // here at all", which deserves its own wording.
            SecretError::Unavailable(error.to_string())
        })
    }
}

impl SecretStore for KeyringSecrets {
    fn get(&self, provider: &str) -> Result<Option<String>, SecretError> {
        match self.entry(provider)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(SecretError::Backend(error.to_string())),
        }
    }

    fn set(&self, provider: &str, secret: &str) -> Result<(), SecretError> {
        self.entry(provider)?
            .set_password(secret)
            .map_err(|error| SecretError::Backend(error.to_string()))
    }

    fn delete(&self, provider: &str) -> Result<(), SecretError> {
        match self.entry(provider)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(SecretError::Backend(error.to_string())),
        }
    }
}

/// The three tiers, in order.
pub struct Secrets {
    env: EnvSecrets,
    keyring: Option<Box<dyn SecretStore>>,
    session: MemorySecrets,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secrets")
            .field("env", &"<snapshot>")
            .field("keyring", &self.keyring.is_some())
            .field("session", &"<memory>")
            .finish()
    }
}

impl Secrets {
    /// The real environment: capture the variables, and use the OS keyring
    /// unless [`DISABLE_KEYRING_VAR`] says not to.
    pub fn from_process_env() -> Self {
        let env = EnvSecrets::from_process_env();
        let keyring: Option<Box<dyn SecretStore>> = if env.keyring_disabled() {
            None
        } else {
            Some(Box::new(KeyringSecrets::new()))
        };
        Self {
            env,
            keyring,
            session: MemorySecrets::new(),
        }
    }

    /// Build from explicit tiers. `keyring` is `None` when there is no keyring
    /// to use, which is exactly what a headless machine looks like.
    pub fn with_stores(env: EnvSecrets, keyring: Option<Box<dyn SecretStore>>) -> Self {
        Self {
            env,
            keyring,
            session: MemorySecrets::new(),
        }
    }

    /// The key for a provider, and where it came from: environment, then
    /// keyring, then this session. A keyring that is present but broken falls
    /// through rather than blocking the other tiers.
    pub fn resolve(&self, provider: &str) -> Option<ResolvedKey> {
        if let Some(secret) = self.env.get(provider).ok().flatten() {
            return Some(ResolvedKey {
                secret,
                source: KeySource::Environment,
            });
        }
        if let Some(keyring) = &self.keyring
            && let Some(secret) = keyring.get(provider).ok().flatten()
        {
            return Some(ResolvedKey {
                secret,
                source: KeySource::Keyring,
            });
        }
        if let Some(secret) = self.session.get(provider).ok().flatten() {
            return Some(ResolvedKey {
                secret,
                source: KeySource::Session,
            });
        }
        None
    }

    /// Where the key for this provider would come from, without reading it.
    pub fn key_source(&self, provider: &str) -> Option<KeySource> {
        self.resolve(provider).map(|key| key.source)
    }

    /// Store a key as durably as this machine allows.
    pub fn store(&self, provider: &str, secret: &str) -> Stored {
        if let Some(keyring) = &self.keyring
            && keyring.set(provider, secret).is_ok()
        {
            return Stored::Keyring;
        }

        // Session memory it is. Say so plainly: on a one-shot CLI run this
        // means the key was not persisted at all.
        let _ = self.session.set(provider, secret);
        Stored::Session {
            warning: format!(
                "no usable OS keyring: the key was NOT saved. \
                 Export {} for this machine instead.",
                env_var_name(provider)
            ),
        }
    }

    /// Forget a stored key.
    pub fn clear(&self, provider: &str) -> Result<Cleared, SecretError> {
        let _ = self.session.delete(provider);
        match &self.keyring {
            Some(keyring) => {
                keyring.delete(provider)?;
                Ok(Cleared::Keyring)
            }
            None => Ok(Cleared::Nothing),
        }
    }
}
