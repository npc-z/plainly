//! Shared scaffolding for the integration tests.
//!
//! Each integration test is compiled as its own crate, so anything here that one
//! of them does not use looks dead to the compiler. The module is shared on
//! purpose: the alternatives are copying the scaffolding into every test file or
//! inventing a support crate for it.

#![allow(dead_code)]

pub mod provider;

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use plainly_core::{
    ARTIFACT_VERSION, Artifact, ExplainRequest, Explanation, Level, Thinking, Timestamp,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A throwaway directory for tests that need real files.
///
/// Deliberately hand-rolled: the point of the exercise is the file behaviour,
/// and one small guard is cheaper than a dependency.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("plainly-{label}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the temp directory is creatable");
        Self { path }
    }

    /// A path inside the directory that does not exist yet.
    pub fn join(&self, relative: &str) -> PathBuf {
        self.path.join(relative)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A SHA-256, of the shape tickets/07 will derive from the Effective Prompt.
pub const PROMPT_VERSION: &str = "9f2c1d4b6a8e0f3c5d7b9a1e2f4c6d8b0a2e4f6c8d0b2a4e6f8c0d2b4a6e8f0c";

/// The Passage from the prototype's first example.
pub const PASSAGE: &str = "The committee conducted a thorough investigation into the matter, but the manager had already gone to ground.";

/// The Passage from the prototype's structural example.
pub const PASSAGE_WITH_GRAMMAR: &str =
    "Not until the auditors had gone did the manager admit that the figures had been doctored.";

/// The prototype's first example as the model would have to return it: no
/// `grammar`, because the paraphrase and glosses leave the Passage readable.
pub const ANSWER: &str = r#"{
  "comprehensible": "The committee tried hard to find out what had happened, but the manager had already gone into hiding.",
  "glosses": [
    { "expression": "conduct an investigation", "gloss": "try to find out what happened" },
    { "expression": "the matter", "gloss": "the situation being discussed" },
    { "expression": "go to ground", "gloss": "hide so that nobody can find you" }
  ],
  "grammar": null,
  "translation": "委员会对此事进行了彻底调查，但那位经理已经躲了起来。"
}"#;

/// The prototype's structural example: the obstacle is the shape of the sentence,
/// so `grammar` carries it.
pub const ANSWER_WITH_GRAMMAR: &str = r#"{
  "comprehensible": "The manager admitted that the figures had been changed dishonestly — but only after the auditors had gone.",
  "glosses": [
    { "expression": "doctored", "gloss": "changed dishonestly" },
    { "expression": "auditors", "gloss": "people who check a company's accounts" }
  ],
  "grammar": "`Not until … did the manager admit …` — starting a sentence with `Not until` moves the auxiliary in front of the subject (`did the manager admit`), the same word order as a question. The meaning is that the admission came only after the auditors had gone.",
  "translation": "直到审计人员离开，经理才承认账目被人做了手脚。"
}"#;

/// A request for `passage` at B2 in Chinese, from the default cloud profile.
pub fn request(passage: &str) -> ExplainRequest {
    ExplainRequest {
        passage: passage.to_string(),
        level: Level::B2,
        source_language: "en".to_string(),
        native_language: "Chinese".to_string(),
        provider: "deepseek".to_string(),
        model: "deepseek-flash".to_string(),
        thinking: Thinking::Off,
        prompt_version: PROMPT_VERSION.to_string(),
        prompt_label: "v6-synthesis".to_string(),
    }
}

/// An Artifact whose metadata is unremarkable, for tests about something else.
pub fn artifact(passage: &str, explanation: Explanation) -> Artifact {
    let now = Timestamp::from_unix_seconds(1_760_000_000).expect("the fixture instant is in range");
    Artifact {
        passage: passage.to_string(),
        level: Level::B2,
        source_language: "en".to_string(),
        native_language: "Chinese".to_string(),
        provider: "deepseek".to_string(),
        model: "deepseek-flash".to_string(),
        thinking: Thinking::Off,
        artifact_version: ARTIFACT_VERSION,
        prompt_version: PROMPT_VERSION.to_string(),
        prompt_label: "v6-synthesis".to_string(),
        created_at: now,
        generated_at: now,
        explanation,
    }
}
