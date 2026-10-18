//! Shared scaffolding for the desktop crate's tests.
//!
//! The window's state takes every input as a value ([`plainly_desktop::app::Desktop::open_at`]),
//! so the scaffolding here is only about producing those values: a temporary
//! home, a compositor that publishes data-control, and a Record whose metadata
//! is unremarkable.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use plainly_core::{
    ARTIFACT_VERSION, Artifact, Explanation, Gloss, Level, PathInputs, Paths, SecretError,
    SecretStore, Thinking, Timestamp,
};
use plainly_desktop::app::Environment;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A throwaway directory for tests that need real files.
///
/// Deliberately hand-rolled, the way the core crate's is: the point of the
/// exercise is the file behaviour, and one small guard is cheaper than a
/// dependency.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "plainly-desktop-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the temp directory is creatable");
        Self { path }
    }

    /// The three XDG directories Plainly resolves, all inside this directory.
    pub fn paths(&self) -> Paths {
        Paths::resolve(&PathInputs::new(&self.path)).expect("a temporary home resolves")
    }

    /// A path inside the directory.
    pub fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The compositor facts for a session that publishes data-control: niri and the
/// other wlroots compositors, where every switch in the settings screen can be
/// offered.
pub fn desktop_with_wayland() -> Environment {
    Environment {
        desktop: plainly_core::Desktop::new(["wl_compositor", "ext_data_control_manager_v1"]),
        outputs: vec![
            ("eDP-1".to_string(), "Built-in display".to_string()),
            ("DP-2".to_string(), "External".to_string()),
        ],
    }
}

/// A keyring that cannot answer: no session bus, or a locked collection.
///
/// Readiness tells this apart from "nothing is stored", and so does the settings
/// screen — the two facts send a person to different places. A desktop test
/// needs a store that fails without a real keyring to fail.
pub struct BrokenKeyring;

impl SecretStore for BrokenKeyring {
    fn get(&self, _provider: &str) -> Result<Option<String>, SecretError> {
        Err(SecretError::Unavailable("no session bus".to_string()))
    }

    fn set(&self, _provider: &str, _secret: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable("no session bus".to_string()))
    }

    fn delete(&self, _provider: &str) -> Result<(), SecretError> {
        Err(SecretError::Unavailable("no session bus".to_string()))
    }
}

/// An Artifact whose metadata is unremarkable, for a test about something else.
pub fn artifact(seconds: i64, passage: &str) -> Artifact {
    let now = Timestamp::from_unix_seconds(seconds).expect("the fixture instant is in range");

    Artifact {
        passage: passage.to_string(),
        level: Level::B2,
        source_language: "en".to_string(),
        native_language: "Chinese".to_string(),
        provider: "deepseek".to_string(),
        model: "deepseek-flash".to_string(),
        thinking: Thinking::Off,
        artifact_version: ARTIFACT_VERSION,
        prompt_version:
            "9f2c1d4b6a8e0f3c5d7b9a1e2f4c6d8b0a2e4f6c8d0b2a4e6f8c0d2b4a6e8f0c".to_string(),
        prompt_label: "v8-examples".to_string(),
        created_at: now,
        generated_at: now,
        explanation: Explanation {
            comprehensible: "The committee tried hard to find out what had happened, but the manager had already gone into hiding.".to_string(),
            glosses: vec![
                Gloss {
                    expression: "go to ground".to_string(),
                    gloss: "hide so that nobody can find you".to_string(),
                    example: "The manager went to ground when the reporters arrived.".to_string(),
                },
            ],
            grammar: None,
            translation: "委员会对此事进行了彻底调查，但那位经理已经躲了起来。".to_string(),
        },
    }
}
