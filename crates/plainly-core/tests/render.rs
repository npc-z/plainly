//! Rendering: the five sections, byte for byte as the prototype documents them.

mod support;

use plainly_core::{Explanation, SectionKind, render};

/// The panel's copy for a Grammar section the model left out. It is interface
/// text, which tickets/15 keeps with the rest of the strings, so the renderer is
/// handed it rather than knowing it.
const NOT_NEEDED: &str = "本段不需要";

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
- `conduct an investigation` → try to find out what happened
- `the matter` → the situation being discussed
- `go to ground` → hide so that nobody can find you

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
- `doctored` → changed dishonestly
- `auditors` → people who check a company's accounts

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
    let sections = render::panel(support::PASSAGE, &common(), NOT_NEEDED);

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
    assert_eq!(grammar.body, NOT_NEEDED);
    assert_eq!(grammar.body, "本段不需要");
}

#[test]
fn the_panel_shows_the_grammar_note_when_there_is_one() {
    let explanation = structural();
    let sections = render::panel(support::PASSAGE_WITH_GRAMMAR, &explanation, NOT_NEEDED);

    let grammar = sections
        .iter()
        .find(|section| section.kind == SectionKind::Grammar)
        .expect("the panel has the section");
    assert_eq!(grammar.body, explanation.grammar.clone().unwrap());
    assert_ne!(grammar.body, NOT_NEEDED);
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
fn a_key_help_with_no_glosses_is_left_out_rather_than_headed_blank() {
    // Zero Glosses is a legitimate answer — the contract has no lower bound, and
    // "nothing in this Passage blocks comprehension" is a real thing for the
    // model to say. A heading with nothing under it would read like a defect.
    let explanation = Explanation::parse(
        r#"{ "comprehensible": "c", "glosses": [], "grammar": null, "translation": "t" }"#,
    )
    .expect("the fixture holds the contract");

    assert_eq!(
        render::markdown("Original text.", &explanation),
        "### Original\nOriginal text.\n\n### Comprehensible English\nc\n\n### Translation\nt\n"
    );

    let sections = render::panel("Original text.", &explanation, NOT_NEEDED);
    let kinds: Vec<SectionKind> = sections.iter().map(|section| section.kind).collect();
    assert_eq!(
        kinds,
        [
            SectionKind::Original,
            SectionKind::ComprehensibleEnglish,
            SectionKind::Grammar,
            SectionKind::Translation,
        ],
        "the panel keeps the Grammar it must label and drops the empty Key Help"
    );
}

#[test]
fn the_panel_and_the_document_carry_the_same_bodies() {
    // Two surfaces, one renderer: the panel differs in what it does with a
    // Grammar section that is not there, and in nothing else.
    let explanation = structural();
    let document = render::markdown(support::PASSAGE_WITH_GRAMMAR, &explanation);
    let sections = render::panel(support::PASSAGE_WITH_GRAMMAR, &explanation, NOT_NEEDED);

    for section in sections {
        assert!(
            document.contains(&section.body),
            "{:?} is missing from the document",
            section.kind
        );
    }
}
