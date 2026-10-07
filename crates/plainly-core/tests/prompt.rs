//! The shipped prompt: what it says, and the version derived from it.

use plainly_core::Level;
use plainly_core::prompt::{FACTORY_PROMPT, PROMPT_LABEL, system_prompt, version};

#[test]
fn the_prompt_fills_in_the_level_and_the_native_language() {
    let prompt = system_prompt(Level::A2, "Japanese");

    assert!(prompt.contains("A2"), "{prompt}");
    assert!(prompt.contains("Japanese"), "{prompt}");
    assert!(!prompt.contains("{{"), "a placeholder was left unfilled");
}

#[test]
fn the_prompt_asks_for_json_and_names_every_field_in_words() {
    // Providers that cannot enforce a schema need the answer asked for in so
    // many words; DeepSeek's JSON mode in particular wants the literal word.
    let prompt = system_prompt(Level::B2, "Chinese");

    for expected in [
        "JSON",
        "comprehensible",
        "glosses",
        "grammar",
        "translation",
    ] {
        assert!(
            prompt.contains(expected),
            "the prompt does not name {expected}"
        );
    }
}

#[test]
fn the_prompt_is_never_returned_unchanged_by_instructions() {
    // The insurance against the staged variant's failure mode: five of twelve
    // paraphrases came back byte-identical to the Passage.
    let prompt = system_prompt(Level::B2, "Chinese");

    assert!(prompt.contains("never return the passage unchanged"));
}

#[test]
fn the_version_is_a_stable_content_hash_of_the_unsubstituted_prompt() {
    let hash = version();

    assert_eq!(hash.len(), 64, "{hash}");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "{hash}");
    assert_eq!(hash, version(), "the same prompt data hashes the same way");
    assert!(!PROMPT_LABEL.is_empty());
    assert!(
        FACTORY_PROMPT.contains("{{LEVEL}}"),
        "the placeholders are part of the prompt data the hash covers"
    );
}
