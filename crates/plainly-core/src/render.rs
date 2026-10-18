//! Rendering an Explanation into sections.
//!
//! The shape is one value (ADR-0005): a list of [`Section`]s whose Key Help body
//! is [`Body::Glosses`], the expression, its meaning and its Example already
//! settled, and whose empty sections are [`Body::Note`]s where a surface asks for
//! one. Every surface consumes that value instead of re-judging the raw fields,
//! and rendering is a pure function of a Passage and an Explanation.
//!
//! The empty-section policy is the one rule a surface chooses between. Grammar
//! with no structural blocker and Key Help with no Glosses disappear from the
//! markdown document. A surface read in place keeps them and says so in its own
//! words, because a reader there cannot tell a missing section from one that was
//! never generated, while a document a person scrolls and re-reads can.

use crate::{Explanation, Gloss};

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
    /// Explanation stays English (spec §4).
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

/// One rendered section: what it is, and what it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub kind: SectionKind,
    pub body: Body,
}

/// One Gloss as a surface draws it: the expression, its meaning, and the
/// Example, settled.
///
/// [`Gloss`]'s `example` is a `String` because the wire schema requires one, and
/// this is where the presence question is decided once, at the only constructor,
/// so no renderer re-derives it by trimming a string. `None` covers both a Record
/// written before Examples existed and a model that left the field blank
/// (ADR-0002).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossView {
    pub expression: String,
    pub gloss: String,
    /// The Example sentence, trimmed, or `None` when there is none.
    pub example: Option<String>,
}

impl GlossView {
    /// The one place a raw [`Gloss`] becomes the rendered shape.
    ///
    /// The text is settled with the presence: surrounding whitespace is not part
    /// of the sentence, so `Some` always carries the trimmed Example and an
    /// Example of only whitespace is `None`.
    pub fn of(gloss: &Gloss) -> Self {
        let example = gloss.example.trim();
        Self {
            expression: gloss.expression.clone(),
            gloss: gloss.gloss.clone(),
            example: (!example.is_empty()).then(|| example.to_string()),
        }
    }
}

/// A section's body, in the terms a surface draws it in.
///
/// The distinction is not decoration. Prose is shown as it stands, while the Key
/// Help's Glosses have three parts each and every surface gives them different
/// treatment: a document makes them a bullet and a block quote, and the panel
/// makes them three widgets with a rule beside the Example. Flattening both into
/// one `String` would force every surface to re-parse markdown or show its
/// syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// Text shown as it stands: the Original, the paraphrases, a Grammar note,
    /// or the Translation.
    Prose(String),
    /// The sentence a surface says where a section is empty, because the model
    /// said nothing there. It is not the Explanation talking, so a surface may
    /// give it a quieter material than prose. A document prints it as text.
    Note(String),
    /// The Key Help: the Glosses themselves, in the model's order and already
    /// settled by [`GlossView::of`].
    Glosses(Vec<GlossView>),
}

/// The sections as markdown, ending in one newline.
///
/// A section the model left nothing for is left out rather than headed and left
/// blank. Empty *prose* is not treated this way: an empty paraphrase is a bad
/// answer, not a missing section.
pub fn markdown(passage: &str, explanation: &Explanation) -> String {
    render_with(
        |kind| kind.title(),
        sections(passage, explanation, &EmptyNotes::silent()),
    )
}

/// The one place a list of [`Section`]s becomes a document.
///
/// The panel copies an Explanation too, under the interface's own headings, and
/// "`### heading`, blank line, body" is a format rather than a surface's choice.
/// Both callers come through here so the two documents cannot drift apart in
/// shape while agreeing about the sections.
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
        .map(|section| format!("### {}\n{}", title_of(section.kind), as_document(&section.body)))
        .collect();

    let mut document = sections.join("\n\n");
    document.push('\n');
    document
}

/// A body as a document's markdown.
fn as_document(body: &Body) -> String {
    match body {
        Body::Prose(text) | Body::Note(text) => text.clone(),
        Body::Glosses(glosses) => glosses_markdown(glosses),
    }
}

/// The sentences a surface says where a section is empty.
///
/// The empty-section decision is core's, but the wording is interface copy and
/// belongs to the surface that says it: the panel has its two sentences, the
/// desktop detail its own. `None` is how a caller says "do not keep this one",
/// which is what a document does with both (see [`markdown`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyNotes<'a> {
    /// What to say where the model gave no Grammar note.
    pub grammar: Option<&'a str>,
    /// What to say where the model gave no Glosses at all.
    pub key_help: Option<&'a str>,
}

impl<'a> EmptyNotes<'a> {
    /// No sentences: a surface that does not show empty sections, which is what
    /// a document is.
    pub const fn silent() -> Self {
        Self {
            grammar: None,
            key_help: None,
        }
    }
}

/// The sections of one Explanation, for a surface read in place.
///
/// The section set, the order and the empty-section policy are this function's;
/// only the wording of the two empty sections is the caller's. A document passes
/// [`EmptyNotes::silent`] and gets only the sections that have content, while the
/// panel and the desktop detail pass their own sentences and get every section.
pub fn sections(passage: &str, explanation: &Explanation, notes: &EmptyNotes) -> Vec<Section> {
    let mut sections = vec![
        Section {
            kind: SectionKind::Original,
            body: Body::Prose(passage.to_string()),
        },
        Section {
            kind: SectionKind::ComprehensibleEnglish,
            body: Body::Prose(explanation.comprehensible.clone()),
        },
    ];

    match (explanation.glosses.is_empty(), notes.key_help) {
        (false, _) => sections.push(Section {
            kind: SectionKind::KeyHelp,
            body: Body::Glosses(explanation.glosses.iter().map(GlossView::of).collect()),
        }),
        (true, Some(note)) => sections.push(Section {
            kind: SectionKind::KeyHelp,
            body: Body::Note(note.to_string()),
        }),
        (true, None) => {}
    }

    match &explanation.grammar {
        Some(grammar) => sections.push(Section {
            kind: SectionKind::Grammar,
            body: Body::Prose(grammar.clone()),
        }),
        None => {
            if let Some(note) = notes.grammar {
                sections.push(Section {
                    kind: SectionKind::Grammar,
                    body: Body::Note(note.to_string()),
                });
            }
        }
    }

    sections.push(Section {
        kind: SectionKind::Translation,
        body: Body::Prose(explanation.translation.clone()),
    });
    sections
}

/// The Key Help as a document: one bullet per Gloss, its meaning on the bullet's
/// own continuation line, and its Example as a block quote under both, in the
/// model's order, the order the prompt asks for, most blocking first.
///
/// A block quote rather than a second continuation line, because the two things
/// are not the same kind of text: the meaning is the Explanation talking, and the
/// Example is a sample of prose it is quoting. A markdown reader sets a quote
/// apart, while it draws two plain lines as one paragraph. A Gloss with no
/// Example is the bullet and its meaning, with no quote.
///
/// Items are separated by a blank line so the list is loose: with a quote in an
/// item, two Glosses flush against each other read as one.
fn glosses_markdown(glosses: &[GlossView]) -> String {
    glosses
        .iter()
        .map(|gloss| {
            let mut block = format!("- `{}`\n  {}", gloss.expression, gloss.gloss);
            if let Some(example) = &gloss.example {
                // One `>` per line. A model that wrapped its sentence over two
                // lines would otherwise spill the second one out of the quote at
                // column zero, which ends the list item and leaves the rest of
                // the Key Help as a paragraph of its own.
                let quoted: Vec<String> = example
                    .lines()
                    .map(|line| format!("  > {line}"))
                    .collect();
                block.push_str("\n\n");
                block.push_str(&quoted.join("\n"));
            }
            block
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
