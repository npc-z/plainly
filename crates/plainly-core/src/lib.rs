//! Plainly's domain logic: the Explanation contract and its rendering, where its
//! files live, what its settings are, and how API keys are resolved.
//!
//! Nothing here touches GTK, WebKit or Wayland. The desktop surfaces pass what
//! they observe in as data (a clipboard's MIME hints, a compositor's protocol
//! list), so every decision stays reachable from a headless test — and so the
//! `plainly` CLI can be built and run on a machine with no graphics stack at all.

pub mod artifact;
pub mod chat;
pub mod config;
pub mod explain;
pub mod explanation;
pub mod paths;
pub mod presets;
pub mod prompt;
pub mod provider;
pub mod render;
pub mod retry;
pub mod secrets;
pub mod setup;

pub use artifact::{ARTIFACT_VERSION, Artifact, Timestamp};
pub use chat::{ChatCompletions, MAX_TOKENS, MAX_TOKENS_THINKING};
pub use config::{
    App, Config, ConfigError, ConfigFile, DEFAULT_CONFIG, ExportFormat, Level, PanelCorner,
    Prompts, ProviderProfile, Thinking,
};
pub use explain::{ExplainError, SOURCE_LANGUAGE, explain};
pub use explanation::{ContractError, Explanation, Gloss, wire_schema};
pub use paths::{CONFIG_FILE, IDENTIFIER, PathError, PathInputs, Paths};
pub use presets::{KeyRequirement, PRESETS, Preset, SchemaSupport, ThinkingSwitch};
pub use prompt::{FACTORY_PROMPT, PROMPT_LABEL};
pub use provider::{ExplainRequest, Provider, ProviderError, ProviderErrorKind};
pub use render::{Section, SectionKind};
pub use retry::{Failure, FailureKind, Stopped};
pub use secrets::{
    Cleared, EnvSecrets, KeySource, KeyringSecrets, MemorySecrets, ResolvedKey, SecretError,
    SecretStore, Secrets, Stored, env_var_name,
};
pub use setup::{ProviderSetup, SetupError};
