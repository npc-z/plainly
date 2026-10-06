//! Plainly's domain logic: where its files live, what its settings are, and how
//! API keys are resolved.
//!
//! Nothing here touches GTK, WebKit or Wayland. The desktop surfaces pass what
//! they observe in as data (a clipboard's MIME hints, a compositor's protocol
//! list), so every decision stays reachable from a headless test — and so the
//! `plainly` CLI can be built and run on a machine with no graphics stack at all.

pub mod config;
pub mod paths;
pub mod secrets;

pub use config::{
    App, Config, ConfigError, ConfigFile, DEFAULT_CONFIG, ExportFormat, Level, PanelCorner,
    Prompts, Provider, Thinking,
};
pub use paths::{CONFIG_FILE, IDENTIFIER, PathError, PathInputs, Paths};
pub use secrets::{
    Cleared, EnvSecrets, KeySource, MemorySecrets, ResolvedKey, SecretError, SecretStore, Secrets,
    Stored, env_var_name,
};
