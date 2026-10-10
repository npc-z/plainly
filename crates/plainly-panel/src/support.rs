//! Fixtures the panel's unit tests share.
//!
//! Three test modules need the same two things — an `Artifact` to draw sections
//! from and a configuration that names a model on this machine — and building them
//! three times would mean three skeletons to keep in step with core's `Artifact`.
//! The shape is a module rather than a crate because only this crate's tests use
//! it, and it is behind `#[cfg(test)]` in `main.rs`, so a release panel does not
//! carry a builder it never calls.
//!
//! Same arrangement as `plainly-core/tests/support`, which is pulled in with
//! `mod support;` for the same reason.

use plainly_core::{Artifact, Explanation, Gloss, Level, Thinking, Timestamp};

/// An Artifact with the passage and provider a test cares about, and everything
/// else fixed.
///
/// `local` is not stored on an Artifact — locality is a fact about the endpoint the
/// configuration resolves to — so callers that need a `Provenance` build it with
/// [`plainly_core::ProviderSetup`] or pass the flag themselves.
pub fn artifact(passage: &str, provider: &str, model: &str) -> Artifact {
    Artifact {
        passage: passage.to_string(),
        level: Level::B2,
        source_language: "English".to_string(),
        native_language: "Chinese".to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        thinking: Thinking::Off,
        artifact_version: plainly_core::ARTIFACT_VERSION,
        prompt_version: "abc".to_string(),
        prompt_label: "Plainly v0".to_string(),
        created_at: Timestamp::from_unix_seconds(0).expect("the epoch is in range"),
        generated_at: Timestamp::from_unix_seconds(0).expect("the epoch is in range"),
        explanation: explanation("c", vec![], None, "t"),
    }
}

/// An Explanation, with its four fields spelled out.
pub fn explanation(
    comprehensible: &str,
    glosses: Vec<Gloss>,
    grammar: Option<&str>,
    translation: &str,
) -> Explanation {
    Explanation {
        comprehensible: comprehensible.to_string(),
        glosses,
        grammar: grammar.map(str::to_string),
        translation: translation.to_string(),
    }
}

/// A person whose provider is a model on this machine — and who has, as they must
/// have, named the model, because the shipped local presets name none
/// (tickets/06).
pub fn local_config() -> plainly_core::Config {
    plainly_core::Config {
        app: plainly_core::App {
            provider: "ollama".to_string(),
            ..plainly_core::App::default()
        },
        providers: [(
            "ollama".to_string(),
            plainly_core::ProviderProfile {
                endpoint: None,
                model: Some("llama3.2".to_string()),
                thinking: Thinking::Off,
            },
        )]
        .into_iter()
        .collect(),
        ..plainly_core::Config::default()
    }
}
