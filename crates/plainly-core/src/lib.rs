//! Plainly's domain logic: the Explanation contract and its rendering, where its
//! files live, what its settings are, and how API keys are resolved.
//!
//! Nothing here touches GTK, WebKit or Wayland. The desktop surfaces pass what
//! they observe in as data (a clipboard's MIME hints, a compositor's protocol
//! list), so every decision stays reachable from a headless test — and so the
//! `plainly` CLI can be built and run on a machine with no graphics stack at all.

pub mod artifact;
pub mod config;
pub mod explain;
pub mod explanation;
pub mod paths;
pub mod provider;
pub mod render;
pub mod secrets;

pub use artifact::{ARTIFACT_VERSION, Artifact, Timestamp};
pub use config::{
    App, Config, ConfigError, ConfigFile, DEFAULT_CONFIG, ExportFormat, Level, PanelCorner,
    Prompts, ProviderProfile, Thinking,
};
pub use explain::{ExplainError, explain};
pub use explanation::{ContractError, Explanation, Gloss, wire_schema};
pub use paths::{CONFIG_FILE, IDENTIFIER, PathError, PathInputs, Paths};
pub use provider::{ExplainRequest, Provider, ProviderError};
pub use render::{Section, SectionKind};
pub use secrets::{
    Cleared, EnvSecrets, KeySource, KeyringSecrets, MemorySecrets, ResolvedKey, SecretError,
    SecretStore, Secrets, Stored, env_var_name,
};
