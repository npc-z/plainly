//! The prompt data: what Plainly asks the model for, and the version derived
//! from the content that is in effect.
//!
//! The factory prompt, the user's appendix and the level descriptors are one
//! body of data (spec §5); these tests pin what reaches the model, what the
//! version covers, and what a user's change can and cannot do.
//!
//! The expected wording is written out here rather than read back from
//! [`factory_descriptor`]: a test that recomputes its expectation from
//! the code cannot disagree with it, and the point of these rows is that they
//! are a decision, not whatever the constant happens to say.

use plainly_core::prompt::{FACTORY_PROMPT, PROMPT_LABEL, Prompt};
use plainly_core::retry::FailureKind;
use plainly_core::{Config, Level, Prompts, wire_schema};

/// The source contract's wording for A1–A2, and therefore for both levels.
const A2_MEANING: &str = "very common words, short sentences, concrete";

#[test]
fn the_prompt_fills_in_the_level_and_the_native_language() {
    let prompt = Prompt::factory().system_prompt(Level::A2, "Japanese");

    assert!(prompt.contains("A2"), "{prompt}");
    assert!(prompt.contains("Japanese"), "{prompt}");
    assert!(!prompt.contains("{{"), "a placeholder was left unfilled");
}

#[test]
fn the_prompt_asks_for_json_and_names_every_field_in_words() {
    // Providers that cannot enforce a schema need the answer asked for in so
    // many words; DeepSeek's JSON mode in particular wants the literal word.
    let prompt = Prompt::factory().system_prompt(Level::B2, "Chinese");

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
    let prompt = Prompt::factory().system_prompt(Level::B2, "Chinese");

    assert!(prompt.contains("never return the passage unchanged"));
}

#[test]
fn the_level_travels_with_its_meaning_and_not_just_its_label() {
    // A bare label is close to sending nothing: two of three levels produced
    // byte-identical paraphrases without a meaning beside them
    // (bench/FINDINGS.md). The four rows are the source contract's wording,
    // except C1 (spec §5).
    for (level, meaning) in [
        (Level::A1, A2_MEANING),
        (Level::A2, A2_MEANING),
        (Level::B2, "English-first; nuance and collocations"),
        (
            Level::C1,
            "subtle meaning, register, idiom and style kept — and still a full \
             restatement, never the passage unchanged",
        ),
    ] {
        let prompt = Prompt::factory().system_prompt(level, "Chinese");

        assert!(
            prompt.contains(&format!("{} ({meaning})", level.as_str())),
            "the {level} label is not sent with its meaning:\n{prompt}"
        );
    }
}

#[test]
fn a_descriptor_is_data_and_never_scanned_for_placeholders() {
    // The descriptor is text a person wrote. Substituting `{{LEVEL}}` first and
    // then scanning the result for `{{NATIVE}}` would treat their text as
    // template; both placeholders are filled in one pass instead.
    let mut prompts = Prompts::default();
    prompts.level_descriptors.insert(
        Level::A2.as_str().to_string(),
        "keep {{NATIVE}} simple".to_string(),
    );

    let text = Prompt::from_config(&prompts).system_prompt(Level::A2, "Japanese");

    assert!(text.contains("keep {{NATIVE}} simple"), "{text}");
    assert!(text.contains("Japanese"), "the real placeholder is filled: {text}");
}

#[test]
fn the_c1_descriptor_does_not_contradict_the_restatement_rule() {
    // "keep most of the original structure" was the sentence that fought the
    // prompt's own hard rule and lost — which is why B2 and C1 collapsed into
    // the same output (bench/FINDINGS.md).
    let factory = Prompt::factory();
    let descriptor = factory.descriptor(Level::C1);

    assert!(
        !descriptor.contains("keep most of the original structure"),
        "the C1 descriptor still fights the restatement rule: {descriptor}"
    );
    assert!(
        descriptor.contains("restatement"),
        "C1's own wording should carry the reconciliation: {descriptor}"
    );
}

#[test]
fn the_appendix_is_appended_and_cannot_remove_a_factory_rule() {
    let appendix = "Prefer British spellings.";
    let prompt = Prompt::from_config(&Prompts {
        appendix: appendix.to_string(),
        ..Default::default()
    });

    let text = prompt.system_prompt(Level::B2, "Chinese");

    assert!(text.ends_with(appendix), "{text}");
    assert!(text.contains("Return a single JSON object"), "{text}");
    assert!(text.contains("never return the passage unchanged"), "{text}");
    assert!(
        text.len() > FACTORY_PROMPT.len(),
        "the factory rules are still there in full"
    );
}

#[test]
fn an_empty_appendix_is_no_appendix() {
    // A file with `appendix = ""` is the factory prompt, not a prompt that
    // happens to end in blank lines: the two must have one version.
    let factory = Prompt::factory();
    let prompt = Prompt::from_config(&Prompts {
        appendix: "  \n".to_string(),
        ..Default::default()
    });

    assert_eq!(
        prompt.system_prompt(Level::B2, "Chinese"),
        factory.system_prompt(Level::B2, "Chinese")
    );
    assert_eq!(prompt.version(), factory.version());
    assert_eq!(PROMPT_LABEL, prompt.label());
}

#[test]
fn the_version_is_a_stable_content_hash_of_the_unsubstituted_prompt() {
    let factory = Prompt::factory();
    let hash = factory.version();

    assert_eq!(hash.len(), 64, "{hash}");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "{hash}");
    assert_eq!(hash, factory.version(), "the same data hashes the same way");
    assert!(!PROMPT_LABEL.is_empty());
    assert!(
        FACTORY_PROMPT.contains("{{LEVEL}}"),
        "the placeholders are part of the prompt data the hash covers"
    );
    assert!(factory.is_factory());
}

#[test]
fn the_appendix_changes_the_version() {
    let factory = Prompt::factory();
    let prompt = Prompt::from_config(&Prompts {
        appendix: "Prefer British spellings.".to_string(),
        ..Default::default()
    });

    assert_ne!(prompt.version(), factory.version());
    assert!(!prompt.is_factory());
}

#[test]
fn one_changed_descriptor_changes_the_version() {
    // The hole this closes: a descriptor that changes the model's output but
    // not the version is a stale hit waiting to happen, because the Lookup Key
    // carries the version and not the prompt text (spec §5).
    let factory = Prompt::factory();
    let mut prompts = Prompts::default();
    prompts
        .level_descriptors
        .insert(Level::B2.as_str().to_string(), "English-first; no idioms".to_string());
    let prompt = Prompt::from_config(&prompts);

    assert!(
        prompt
            .system_prompt(Level::B2, "Chinese")
            .contains("English-first; no idioms")
    );
    assert_ne!(prompt.version(), factory.version());
    assert!(!prompt.is_factory());
}

#[test]
fn a_descriptor_written_back_verbatim_is_not_a_change() {
    let factory = Prompt::factory();
    let mut prompts = Prompts::default();
    prompts
        .level_descriptors
        .insert(Level::A2.as_str().to_string(), A2_MEANING.to_string());

    assert_eq!(Prompt::from_config(&prompts).version(), factory.version());
}

#[test]
fn the_level_and_the_native_language_are_not_in_the_version() {
    // They are separate components of the Lookup Key (spec §9), so substituting
    // them into the hash would give one question two identities.
    let factory = Prompt::factory();

    assert_ne!(
        factory.system_prompt(Level::A2, "Japanese"),
        factory.system_prompt(Level::B2, "Chinese")
    );
    assert_eq!(factory.version(), Prompt::factory().version());
}

#[test]
fn the_display_toggles_do_not_touch_the_prompt_the_schema_or_the_version() {
    let showing = Config::default();
    let hidden = Config::parse(
        "[app]\n\
         show_original = false\n\
         show_comprehensible = false\n\
         show_glosses = false\n\
         show_grammar = false\n\
         show_translation = false\n",
    )
    .expect("the fixture is valid TOML");

    assert_eq!(
        Prompt::from_config(&hidden.prompts).version(),
        Prompt::from_config(&showing.prompts).version()
    );
    assert_eq!(
        Prompt::from_config(&hidden.prompts).system_prompt(Level::B2, "Chinese"),
        Prompt::from_config(&showing.prompts).system_prompt(Level::B2, "Chinese")
    );

    // The schema still asks for all four fields: a hidden section is hidden, not
    // ungenerated (spec §5).
    let schema = wire_schema();
    for field in ["comprehensible", "glosses", "grammar", "translation"] {
        assert!(
            schema["properties"].get(field).is_some(),
            "the schema must still ask for {field}: {schema}"
        );
    }
}

#[test]
fn a_prompt_that_keeps_failing_the_contract_offers_the_factory_one() {
    let prompt = Prompt::from_config(&Prompts {
        appendix: "Answer in one line.".to_string(),
        ..Default::default()
    });

    let fallback = prompt
        .fallback_after(FailureKind::Malformed, Level::B2)
        .expect("the user's prompt is what failed, so the factory one is offered");
    assert!(fallback.is_factory());
    assert_ne!(fallback.version(), prompt.version());

    // A provider failure is not the prompt's doing, and neither is an endpoint
    // that answered without a chat completion at all — `Empty` covers those too,
    // so offering a fallback would blame the prompt for the endpoint.
    assert!(
        prompt
            .fallback_after(FailureKind::Unavailable, Level::B2)
            .is_none()
    );
    assert!(
        prompt
            .fallback_after(FailureKind::Empty, Level::B2)
            .is_none()
    );

    // And a run already sending the factory text has nothing to fall back from.
    assert!(
        Prompt::factory()
            .fallback_after(FailureKind::Malformed, Level::B2)
            .is_none()
    );
}

#[test]
fn a_descriptor_reworded_for_another_level_is_not_a_fallback_for_this_one() {
    // The retry has to change the text this run sends. A row that is never
    // substituted at this level does not, so the offered retry could only fail
    // the same way.
    let mut prompts = Prompts::default();
    prompts
        .level_descriptors
        .insert(Level::A2.as_str().to_string(), "the simplest words".to_string());
    let prompt = Prompt::from_config(&prompts);

    assert!(
        prompt
            .fallback_after(FailureKind::Malformed, Level::B2)
            .is_none()
    );
    assert!(
        prompt
            .fallback_after(FailureKind::Malformed, Level::A2)
            .is_some()
    );
}
