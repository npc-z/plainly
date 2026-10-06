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
use std::ffi::{OsStr, OsString};
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
    #[error("{variable} is not valid UTF-8; a secret must be a UTF-8 string")]
    NotUtf8 { variable: String },
}

/// Somewhere a secret can live. Implementations: the environment (read-only),
/// the OS keyring, and process memory.
pub trait SecretStore: Send + Sync {
    fn get(&self, provider: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, provider: &str, secret: &str) -> Result<(), SecretError>;
    fn delete(&self, provider: &str) -> Result<(), SecretError>;
}

/// Keys supplied in the environment, captured as data.
///
/// Values are kept as the OS handed them over. Converting lossily here would
/// rewrite a stray byte as U+FFFD and hand the keyring a *different* secret than
/// the one that was exported — a silent authentication failure with nothing to
/// see. A value that is not valid UTF-8 is reported instead.
#[derive(Debug, Clone, Default)]
pub struct EnvSecrets {
    vars: BTreeMap<String, OsString>,
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
        V: Into<OsString>,
    {
        Self {
            vars: pairs
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    /// The real process environment, raw.
    ///
    /// `std::env::vars_os` rather than `vars`: the latter panics on a variable
    /// that is not valid UTF-8, and one stray byte somewhere in the environment
    /// is not a reason to bring down the command. Validity is judged later, and
    /// only for the variable actually being asked for.
    pub fn from_process_env() -> Self {
        Self {
            vars: std::env::vars_os()
                // Only the name is converted lossily, and only because an
                // environment variable's name has to be a `String` to be looked
                // up. A name that is not UTF-8 cannot be `PLAINLY_*_API_KEY`,
                // so it can never be mistaken for one of ours.
                .map(|(name, value)| (name.to_string_lossy().into_owned(), value))
                .collect(),
        }
    }

    /// The raw value of a provider's variable, if it is set and not empty.
    fn raw(&self, provider: &str) -> Option<&OsStr> {
        self.vars
            .get(&env_var_name(provider))
            .map(OsString::as_os_str)
            .filter(|value| !value.is_empty())
    }

    /// Whether this environment supplies a key for a provider, without
    /// interpreting the value.
    pub fn provides(&self, provider: &str) -> bool {
        self.raw(provider).is_some()
    }

    /// Whether the keyring has been switched off for this environment.
    pub fn keyring_disabled(&self) -> bool {
        self.vars
            .get(DISABLE_KEYRING_VAR)
            .and_then(|value| value.to_str())
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
        let Some(raw) = self.raw(provider) else {
            return Ok(None);
        };
        match raw.to_str() {
            Some(value) => Ok(Some(value.to_string())),
            None => Err(SecretError::NotUtf8 {
                variable: env_var_name(provider),
            }),
        }
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
    /// keyring, then this session.
    ///
    /// Every tier is asked in turn, and the first one with an answer wins.
    ///
    /// A tier that fails to answer is an **error**, not a skip: "nowhere has a
    /// key for this provider" and "the keyring would not open" call for
    /// different things from the user, and a caller cannot tell them apart if
    /// failures are swallowed here. The error says which store failed —
    /// [`SecretError::NotUtf8`] for an exported value we cannot use,
    /// [`SecretError::Unavailable`] or [`SecretError::Backend`] for the keyring.
    ///
    /// Note the consequence: a locked keyring stops the search rather than
    /// falling through to a key held in memory. Using a different credential
    /// than the stored one, without saying so, is the worse failure.
    pub fn resolve(&self, provider: &str) -> Result<Option<ResolvedKey>, SecretError> {
        if let Some(secret) = self.env.get(provider)? {
            return Ok(Some(ResolvedKey {
                secret,
                source: KeySource::Environment,
            }));
        }
        if let Some(keyring) = &self.keyring
            && let Some(secret) = keyring.get(provider)?
        {
            return Ok(Some(ResolvedKey {
                secret,
                source: KeySource::Keyring,
            }));
        }
        if let Some(secret) = self.session.get(provider)? {
            return Ok(Some(ResolvedKey {
                secret,
                source: KeySource::Session,
            }));
        }
        Ok(None)
    }

    /// Where the key for this provider would come from.
    ///
    /// This asks the stores, keyring included, but never returns the secret
    /// itself. Prefer [`Secrets::environment_provides`] when all you need is
    /// the precedence note: that one stays off the D-Bus.
    pub fn key_source(&self, provider: &str) -> Result<Option<KeySource>, SecretError> {
        Ok(self.resolve(provider)?.map(|key| key.source))
    }

    /// Whether the environment supplies a key for this provider.
    ///
    /// Checks only the environment tier, and only whether it is set, so callers
    /// that merely want to warn "your exported key outranks anything I store" do
    /// not pay for a keyring round trip or trip over an unreadable value.
    pub fn environment_provides(&self, provider: &str) -> bool {
        self.env.provides(provider)
    }

    /// Store a key as durably as this machine allows.
    pub fn store(&self, provider: &str, secret: &str) -> Stored {
        // A keyring that is present but broken is worth distinguishing from no
        // keyring at all: "locked collection" and "no session bus" send the
        // user to different places. Both end up in the same fallback, but the
        // reason travels with it.
        let reason = match &self.keyring {
            Some(keyring) => match keyring.set(provider, secret) {
                Ok(()) => return Stored::Keyring,
                Err(error) => error.to_string(),
            },
            None => "no OS keyring is in use".to_string(),
        };

        // Session memory it is. On a one-shot CLI run that means the key dies
        // with the process, so the warning has to say so outright.
        let _ = self.session.set(provider, secret);
        Stored::Session {
            warning: format!(
                "{reason}: the key was NOT saved and lives only in this process. \
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
