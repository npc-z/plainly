//! Where Plainly keeps its files.
//!
//! These paths mirror what Tauri's `appConfigDir`, `appDataDir` and
//! `appCacheDir` resolve to, so the desktop crate can adopt them later without
//! moving anyone's files.
//!
//! Resolution takes its inputs — the home directory and the environment — as
//! data rather than reading the process environment itself. That keeps the
//! platform rules testable on any one platform, and it keeps `unsafe
//! { std::env::set_var }` out of the test suite.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The application identifier. It names the config, data and cache
/// directories, so changing it moves everyone's files.
pub const IDENTIFIER: &str = "dev.plainly.app";

/// The configuration file's name inside the config directory.
pub const CONFIG_FILE: &str = "config.toml";

/// The history store's file name inside the data directory.
pub const HISTORY_FILE: &str = "history.db";

/// The config, data and cache directories Plainly uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    config_dir: PathBuf,
    data_dir: PathBuf,
    cache_dir: PathBuf,
}

/// The environment [`Paths`] resolves against.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathInputs {
    home: Option<PathBuf>,
    env: BTreeMap<String, String>,
}

/// Why a set of paths could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// No home directory in the environment, and no platform variable that
    /// stands in for one.
    #[error("no home directory: neither HOME nor USERPROFILE is set")]
    NoHome,
}

impl PathInputs {
    /// Inputs with a home directory and no environment variables set.
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self {
            home: Some(home.into()),
            env: BTreeMap::new(),
        }
    }

    /// Inputs with no home directory at all, for the platform variables that
    /// can stand in for one.
    pub fn without_home() -> Self {
        Self::default()
    }

    /// Add one environment variable.
    pub fn with_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Read the real process environment.
    pub fn from_env() -> Self {
        let mut inputs = Self::default();
        for key in [
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
        ] {
            if let Some(value) = std::env::var_os(key) {
                inputs
                    .env
                    .insert(key.to_string(), value.to_string_lossy().into_owned());
            }
        }
        inputs.home = inputs
            .var("HOME")
            .or_else(|| inputs.var("USERPROFILE"))
            .map(PathBuf::from);
        inputs
    }

    /// A variable's value, treating empty as unset.
    fn var(&self, key: &str) -> Option<&str> {
        self.env
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    fn home(&self) -> Result<&Path, PathError> {
        self.home.as_deref().ok_or(PathError::NoHome)
    }
}

impl Paths {
    /// Resolve the three directories from explicit inputs.
    pub fn resolve(inputs: &PathInputs) -> Result<Self, PathError> {
        let identifier = Path::new(IDENTIFIER);

        // macOS keeps everything under the home directory and ignores XDG.
        #[cfg(target_os = "macos")]
        let (config_dir, data_dir, cache_dir) = {
            let home = inputs.home()?;
            let support = home.join("Library/Application Support").join(identifier);
            (
                support.clone(),
                support,
                home.join("Library/Caches").join(identifier),
            )
        };

        // Windows uses the roaming profile for configuration and the local one
        // for data, which is what Tauri does there too.
        #[cfg(target_os = "windows")]
        let (config_dir, data_dir, cache_dir) = {
            let roaming = inputs
                .var("APPDATA")
                .map(PathBuf::from)
                .or_else(|| inputs.home().ok().map(|home| home.join("AppData/Roaming")))
                .ok_or(PathError::NoHome)?;
            let local = inputs
                .var("LOCALAPPDATA")
                .map(PathBuf::from)
                .or_else(|| inputs.home().ok().map(|home| home.join("AppData/Local")))
                .ok_or(PathError::NoHome)?;
            (
                roaming.join(identifier),
                local.join(identifier),
                local.join(identifier).join("cache"),
            )
        };

        // Everything else follows the XDG base directory spec, which is what
        // Tauri does on Linux.
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let (config_dir, data_dir, cache_dir) = {
            let home = inputs.home()?;
            (
                xdg_dir(inputs.var("XDG_CONFIG_HOME"), home, ".config").join(identifier),
                xdg_dir(inputs.var("XDG_DATA_HOME"), home, ".local/share").join(identifier),
                xdg_dir(inputs.var("XDG_CACHE_HOME"), home, ".cache").join(identifier),
            )
        };

        Ok(Self {
            config_dir,
            data_dir,
            cache_dir,
        })
    }

    /// Resolve the three directories from the real process environment.
    pub fn discover() -> Result<Self, PathError> {
        Self::resolve(&PathInputs::from_env())
    }

    /// Where hand-editable configuration lives.
    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    /// Where the history store and other durable records live.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Where recomputable things (capability probes) live. Clearing this must
    /// never lose a choice the user made.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// The configuration file itself.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join(CONFIG_FILE)
    }

    /// The history store itself — durable records, unlike the cache.
    pub fn history_file(&self) -> PathBuf {
        self.data_dir.join(HISTORY_FILE)
    }
}

/// An XDG variable is only honoured when it is absolute; a relative one is
/// treated as unset, per the base directory spec.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn xdg_dir(var: Option<&str>, home: &Path, fallback: &str) -> PathBuf {
    match var {
        Some(value) if Path::new(value).is_absolute() => PathBuf::from(value),
        _ => home.join(fallback),
    }
}
