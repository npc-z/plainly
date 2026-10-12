//! Rendering an Explanation into sections.
//!
//! The shape is the prototype's, settled in `issues/01-output-contract.md`:
//! `### Original` / `### Comprehensible English` / `### Key Help` /
//! `### Grammar` (only when there is one) / `### Translation`, with the
//! Translation last. A reader of the markdown cannot tell it was rendered from
//! JSON, which is the point.
//!
//! Rendering is a pure function of a Passage and an Explanation, and the CLI,
//! the markdown export and the panel all go through it. The raw CSV does not:
//! a spreadsheet cell is not a document. What differs between surfaces is
//! one rule: the two sections that can be empty *because the model said nothing
//! is here* — Grammar with no structural blocker, Key Help with no Glosses —
//! disappear from the document but are *said* on the panel, in the caller's own
//! words. A panel is read once, in place, where a missing section cannot be told
//! from a section that was never generated; a document a person scrolls and
//! re-reads does not have that problem, so it keeps the silence.

use crate::Explanation;

/// Which part of an Explanation a section is.
///
/// The order the sections are shown in is the order `sections` renders them in;
/// this enum deliberately does not carry a second copy of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    Original,
    ComprehensibleEnglish,
    KeyHelp,
    Grammar,
    Translation,
}

impl SectionKind {
    /// The section's heading *in the markdown document* — `### Original` and so
    /// on.
    ///
    /// The content of an Explanation is English, so the document's headings are
    /// too. The panel does not use this: it takes a [`SectionKind`] and words the
    /// heading itself, which is how an interface can be Chinese while the
    /// Explanation stays English (spec §4; the strings live in tickets/15).
    pub fn title(self) -> &'static str {
        match self {
            SectionKind::Original => "Original",
            SectionKind::ComprehensibleEnglish => "Comprehensible English",
            SectionKind::KeyHelp => "Key Help",
            SectionKind::Grammar => "Grammar",
            SectionKind::Translation => "Translation",
        }
    }
}

/// One rendered section: what it is, and its body as the surface will show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub kind: SectionKind,
    pub body: String,
}

/// The sections as markdown, in the prototype's shape, ending in one newline.
///
/// A section the model left nothing for is left out rather than headed and left
/// blank: `Grammar` when there is no structural blocker, and `Key Help` when
/// there are no Glosses at all. In both cases the emptiness is the model saying
/// "nothing here", and a reader of the markdown loses nothing by its absence.
/// Empty *prose* is not treated this way — an empty paraphrase is a bad answer,
/// not a missing section (tickets/07).
pub fn markdown(passage: &str, explanation: &Explanation) -> String {
    render_with(
        |kind| kind.title(),
        sections(passage, explanation, None, None),
    )
}

/// The one place a list of [`Section`]s becomes a document.
///
/// The panel copies an Explanation too (tickets/15), under the interface's own
/// headings, and "`### heading`, blank line, body" is a format rather than a
/// surface's choice — so both callers come through here and the two documents
/// cannot drift apart in shape while agreeing about the sections.
///
/// `title` is a function rather than a `&str` because the markdown headings are
/// the section's own English name and the panel's are not: the caller that words
/// them is the caller that knows the language.
pub fn render_with(
    title_of: impl Fn(SectionKind) -> &'static str,
    sections: Vec<Section>,
) -> String {
    let sections: Vec<String> = sections
        .into_iter()
        .map(|section| format!("### {}\n{}", title_of(section.kind), section.body))
        .collect();

    let mut document = sections.join("\n\n");
    document.push('\n');
    document
}

/// The same sections for the panel.
///
/// Two differences from [`markdown`], and they are the same difference twice: a
/// Grammar section the model left out is *kept* and labelled with
/// `grammar_not_needed`, and a Key Help with no Glosses is *kept* and labelled
/// with `key_help_not_needed`. A panel is read once, in place, so a learner who
/// sees no Key Help section there cannot tell "nothing in this Passage blocked
/// comprehension" from "nothing was generated for this section" — the reason
/// that keeps the Grammar section is true of the empty Key Help word for word
/// (tickets/15). Every other rule, including the section order, is the
/// document's rule too, stated on [`markdown`].
///
/// The two wordings belong to the caller: they are interface copy, and
/// tickets/15 keeps them with the rest of the interface strings rather than in
/// the domain. They are separate sentences because they are separate facts, and
/// one empty section repeating the other's words would make one of them look
/// like the other's mistake. A `None` note is how a surface says "do not keep
/// this one", which is what the document does with both.
pub fn panel(
    passage: &str,
    explanation: &Explanation,
    grammar_not_needed: &str,
    key_help_not_needed: &str,
) -> Vec<Section> {
    sections(
        passage,
        explanation,
        Some(grammar_not_needed),
        Some(key_help_not_needed),
    )
}

/// Render the sections, with the panel's labels for the sections a model left
/// empty if there is a surface to show them on.
fn sections(
    passage: &str,
    explanation: &Explanation,
    grammar_not_needed: Option<&str>,
    key_help_not_needed: Option<&str>,
) -> Vec<Section> {
    let mut sections = vec![
        Section {
            kind: SectionKind::Original,
            body: passage.to_string(),
        },
        Section {
            kind: SectionKind::ComprehensibleEnglish,
            body: explanation.comprehensible.clone(),
        },
    ];

    let key_help = key_help(explanation);
    match (key_help.is_empty(), key_help_not_needed) {
        (false, _) => sections.push(Section {
            kind: SectionKind::KeyHelp,
            body: key_help,
        }),
        // Nothing to explain is a legitimate answer, not a missing one, and the
        // panel says so rather than leaving a gap where a section was expected.
        (true, Some(note)) => sections.push(Section {
            kind: SectionKind::KeyHelp,
            body: note.to_string(),
        }),
        (true, None) => {}
    }

    match &explanation.grammar {
        Some(grammar) => sections.push(Section {
            kind: SectionKind::Grammar,
            body: grammar.clone(),
        }),
        None => {
            if let Some(grammar_not_needed) = grammar_not_needed {
                sections.push(Section {
                    kind: SectionKind::Grammar,
                    body: grammar_not_needed.to_string(),
                });
            }
        }
    }

    sections.push(Section {
        kind: SectionKind::Translation,
        body: explanation.translation.clone(),
    });
    sections
}

/// One `- \`expression\` → gloss` line per Gloss, in the model's order — the
/// order the prompt asks for, most blocking first. Empty when there are no
/// Glosses, which is a legitimate answer: the model found nothing to explain.
fn key_help(explanation: &Explanation) -> String {
    explanation
        .glosses
        .iter()
        .map(|gloss| format!("- `{}` → {}", gloss.expression, gloss.gloss))
        .collect::<Vec<_>>()
        .join("\n")
}
