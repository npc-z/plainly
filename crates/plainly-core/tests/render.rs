//! Rendering: the five sections, byte for byte as the prototype documents them.

mod support;

use plainly_core::{Body, Explanation, SectionKind, render};

/// The prose a body holds, for the tests that are about a sentence rather than
/// about the Key Help's Glosses.
fn prose(body: &Body) -> &str {
    match body {
        Body::Prose(text) => text,
        Body::Glosses(glosses) => panic!("expected prose, got {} Glosses", glosses.len()),
    }
}

/// The panel's copy for a Grammar section the model left out. It is interface
/// text, which tickets/15 keeps with the rest of the strings, so the renderer is
/// handed it rather than knowing it.
const NOT_NEEDED: &str = "本段不需要";

/// The same for a Key Help with no Glosses. Its own sentence rather than the
/// Grammar one: two empty sections saying the same thing would make one of them
/// look like the other's mistake (tickets/15).
const NO_GLOSSES: &str = "本段没有需要解释的表达";

/// The prototype's common example, parsed through the contract so the fixture
/// and the renderer cannot drift apart.
fn common() -> Explanation {
    Explanation::parse(support::ANSWER).expect("the fixture holds the contract")
}

fn structural() -> Explanation {
    Explanation::parse(support::ANSWER_WITH_GRAMMAR).expect("the fixture holds the contract")
}

#[test]
fn the_common_example_renders_exactly_as_the_prototype_documents_it() {
    let expected = "\
### Original
The committee conducted a thorough investigation into the matter, but the manager had already gone to ground.

### Comprehensible English
The committee tried hard to find out what had happened, but the manager had already gone into hiding.

### Key Help
- `conduct an investigation`
  try to find out what happened

  > The police conducted a thorough investigation into the theft.

- `the matter`
  the situation being discussed

  > The manager refused to discuss the matter with reporters.

- `go to ground`
  hide so that nobody can find you

  > The manager went to ground when the reporters arrived.

### Translation
委员会对此事进行了彻底调查，但那位经理已经躲了起来。
";

    assert_eq!(render::markdown(support::PASSAGE, &common()), expected);
}

#[test]
fn the_structural_example_renders_exactly_as_the_prototype_documents_it() {
    let expected = "\
### Original
Not until the auditors had gone did the manager admit that the figures had been doctored.

### Comprehensible English
The manager admitted that the figures had been changed dishonestly — but only after the auditors had gone.

### Key Help
- `doctored`
  changed dishonestly

  > The accounts had been doctored to hide the missing money.

- `auditors`
  people who check a company's accounts

  > The auditors found that the figures did not add up.

### Grammar
`Not until … did the manager admit …` — starting a sentence with `Not until` moves the auxiliary in front of the subject (`did the manager admit`), the same word order as a question. The meaning is that the admission came only after the auditors had gone.

### Translation
直到审计人员离开，经理才承认账目被人做了手脚。
";

    assert_eq!(
        render::markdown(support::PASSAGE_WITH_GRAMMAR, &structural()),
        expected
    );
}

#[test]
fn a_gloss_with_no_example_renders_two_lines() {
    // A Record generated before Examples existed (ADR 0002) and a model that left
    // the Example empty are the same picture: no third line, not an empty one.
    let explanation = Explanation::parse(
        r#"{
            "comprehensible": "c",
            "glosses": [
                { "expression": "go to ground", "gloss": "hide", "example": "" },
                { "expression": "the matter", "gloss": "the situation", "example": "The matter is closed." }
            ],
            "grammar": null,
            "translation": "t"
        }"#,
    )
    .expect("the fixture holds the contract");

    assert_eq!(
        render::markdown("Original text.", &explanation),
        "### Original\nOriginal text.\n\n### Comprehensible English\nc\n\n### Key Help\n\
         - `go to ground`\n  hide\n\n- `the matter`\n  the situation\n\n  > The matter is closed.\n\n### Translation\nt\n"
    );
}

#[test]
fn an_example_the_model_wrapped_over_two_lines_stays_one_quote() {
    // Model output is not trusted to be one line. A second line left at column
    // zero would end the list item, and the rest of the Key Help would fall out
    // of the list as a paragraph of its own.
    let explanation = Explanation::parse(
        r#"{
            "comprehensible": "c",
            "glosses": [
                { "expression": "go to ground", "gloss": "hide", "example": "He went to ground.\nNobody found him." }
            ],
            "grammar": null,
            "translation": "t"
        }"#,
    )
    .expect("the fixture holds the contract");

    assert_eq!(
        render::markdown("Original text.", &explanation),
        "### Original\nOriginal text.\n\n### Comprehensible English\nc\n\n### Key Help\n\
         - `go to ground`\n  hide\n\n  > He went to ground.\n  > Nobody found him.\n\n### Translation\nt\n"
    );
}

#[test]
fn the_document_omits_a_grammar_section_there_is_none_of() {
    let document = render::markdown(support::PASSAGE, &common());

    assert!(!document.contains("### Grammar"), "{document}");
    // The Translation is still last, and the document still ends in one newline.
    assert!(
        document
            .ends_with("### Translation\n委员会对此事进行了彻底调查，但那位经理已经躲了起来。\n"),
        "{document:?}"
    );
}

#[test]
fn the_panel_keeps_the_grammar_section_and_says_it_is_not_needed() {
    let sections = render::panel(support::PASSAGE, &common(), NOT_NEEDED, NO_GLOSSES);

    let kinds: Vec<SectionKind> = sections.iter().map(|section| section.kind).collect();
    assert_eq!(
        kinds,
        [
            SectionKind::Original,
            SectionKind::ComprehensibleEnglish,
            SectionKind::KeyHelp,
            SectionKind::Grammar,
            SectionKind::Translation,
        ],
        "the panel shows every section, in the order the spec fixes"
    );

    let grammar = sections
        .iter()
        .find(|section| section.kind == SectionKind::Grammar)
        .expect("the panel has the section");
    assert_eq!(prose(&grammar.body), NOT_NEEDED);
    assert_eq!(prose(&grammar.body), "本段不需要");
}

#[test]
fn the_panel_shows_the_grammar_note_when_there_is_one() {
    let explanation = structural();
    let sections = render::panel(
        support::PASSAGE_WITH_GRAMMAR,
        &explanation,
        NOT_NEEDED,
        NO_GLOSSES,
    );

    let grammar = sections
        .iter()
        .find(|section| section.kind == SectionKind::Grammar)
        .expect("the panel has the section");
    let note = explanation.grammar.clone().expect("the fixture has a note");
    assert_eq!(prose(&grammar.body), note);
    assert_ne!(prose(&grammar.body), NOT_NEEDED);
}

#[test]
fn a_grammar_section_sits_above_the_translation() {
    let document = render::markdown(support::PASSAGE_WITH_GRAMMAR, &structural());

    let grammar = document.find("### Grammar").expect("the section is there");
    let translation = document
        .find("### Translation")
        .expect("the section is there");
    assert!(grammar < translation, "{document}");
}

#[test]
fn the_passage_is_rendered_verbatim_even_when_it_is_not_clean_prose() {
    // The Original is authoritative: oddities, trailing space and all. What the
    // model may not do is correct it (spec §4).
    let passage = "  not until the auditors   had gone —  did he admit it.  \n";
    let document = render::markdown(passage, &common());

    assert!(
        document.starts_with(&format!("### Original\n{passage}\n")),
        "{document:?}"
    );
}

#[test]
fn an_empty_key_help_is_left_out_of_the_document_and_said_on_the_panel() {
    // Zero Glosses is a legitimate answer — the contract has no lower bound, and
    // "nothing in this Passage blocks comprehension" is a real thing for the
    // model to say. A heading with nothing under it would read like a defect, so
    // the document drops the section; the panel keeps it with a sentence, because
    // a panel is read once, in place, where a gap cannot be told from a section
    // that was never generated (tickets/15).
    let explanation = Explanation::parse(
        r#"{ "comprehensible": "c", "glosses": [], "grammar": null, "translation": "t" }"#,
    )
    .expect("the fixture holds the contract");

    assert_eq!(
        render::markdown("Original text.", &explanation),
        "### Original\nOriginal text.\n\n### Comprehensible English\nc\n\n### Translation\nt\n"
    );

    let sections = render::panel("Original text.", &explanation, NOT_NEEDED, NO_GLOSSES);
    let kinds: Vec<SectionKind> = sections.iter().map(|section| section.kind).collect();
    assert_eq!(
        kinds,
        [
            SectionKind::Original,
            SectionKind::ComprehensibleEnglish,
            SectionKind::KeyHelp,
            SectionKind::Grammar,
            SectionKind::Translation,
        ],
        "the panel keeps both sections it has to label"
    );

    let key_help = sections
        .iter()
        .find(|section| section.kind == SectionKind::KeyHelp)
        .expect("the panel has the section");
    assert_eq!(prose(&key_help.body), NO_GLOSSES);
    assert_ne!(
        prose(&key_help.body),
        NOT_NEEDED,
        "the two empty sections do not say the same thing"
    );
}

#[test]
fn the_panel_and_the_document_carry_the_same_bodies() {
    // Two surfaces, one renderer: the panel differs in what it does with the two
    // sections a model can leave empty, and in nothing else.
    let explanation = structural();
    let document = render::markdown(support::PASSAGE_WITH_GRAMMAR, &explanation);
    let sections = render::panel(
        support::PASSAGE_WITH_GRAMMAR,
        &explanation,
        NOT_NEEDED,
        NO_GLOSSES,
    );

    for section in sections {
        match &section.body {
            Body::Prose(text) => assert!(
                document.contains(text),
                "{:?} is missing from the document",
                section.kind
            ),
            Body::Glosses(glosses) => {
                for gloss in glosses {
                    assert!(
                        document.contains(&gloss.expression),
                        "a Gloss expression of {:?} is missing from the document",
                        section.kind
                    );
                    assert!(
                        document.contains(&gloss.gloss),
                        "a Gloss meaning of {:?} is missing from the document",
                        section.kind
                    );
                }
            }
        }
    }
}
