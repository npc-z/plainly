//! What a history row and a history detail are, as the window draws them.
//!
//! The frontend is a thin renderer: it is handed one value per row, already
//! carrying every fact the spec says a row has to show — provider, model,
//! Level, Tags, the moment, and the **prompt version** (spec §13). The prompt
//! version is the reason this module exists rather than a `Record` traveling
//! whole: the store's `Record` is the wrong shape for a list — it carries the
//! Explanation, which no row shows — and a row that lacked the version would
//! read as a duplicate the moment the factory prompt changed for the same
//! Passage, which is exactly what ticket 09 warned about.

use serde::Serialize;

use crate::strings::STRINGS;
use plainly_core::export;
use plainly_core::render;
use plainly_core::{Config, Record, Timestamp, local_of};

/// How much of the Passage a list row shows: enough to recognise a paragraph,
/// and the same width the CLI's listing uses, so the two surfaces read alike.
const SNIPPET: usize = 72;

/// How many characters of the prompt hash a row shows. Eight is what the CLI
/// prints and more than a person needs to see that two versions differ; the
/// detail shows the whole hash, because that is the one place it can be read.
const SHORT_HASH: usize = 8;

/// One row of the history list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecordView {
    pub id: i64,
    /// The Passage on one line, cut to about [`SNIPPET`] characters.
    pub snippet: String,
    pub level: String,
    pub provider: String,
    pub model: String,
    /// The human-readable name of the prompt that produced this: what a person
    /// filters by.
    pub prompt_label: String,
    /// The whole prompt hash: what tells two prompt versions apart.
    pub prompt_version: String,
    /// The first [`SHORT_HASH`] characters, for the row.
    pub prompt_short: String,
    pub tags: Vec<String>,
    /// When this question was last asked, in seconds since the epoch.
    ///
    /// Seconds rather than a rendered string: the date groups in the sidebar
    /// are **local** (今天 / 昨天 / 更早), and only the webview knows this
    /// machine's offset. A UTC-rendered moment grouped into UTC days would put
    /// an evening Passage under tomorrow's heading.
    pub last_seen: i64,
    /// The same instant in UTC, for a tooltip.
    pub last_seen_utc: String,
    /// Whether the model that answered runs on this machine. It is what makes
    /// the detail's "this may be confidently wrong" note apply, and it is a
    /// fact about the *endpoint* rather than about the provider's name
    /// (spec §12).
    pub local: bool,
}

impl RecordView {
    /// The row for one Record, with the configuration that says whether the
    /// model that answered is on this machine.
    pub fn of(record: &Record, config: &Config) -> Self {
        let artifact = &record.artifact;

        Self {
            id: record.id,
            snippet: snippet(&artifact.passage),
            level: artifact.level.to_string(),
            provider: artifact.provider.clone(),
            model: artifact.model.clone(),
            prompt_label: artifact.prompt_label.clone(),
            prompt_short: short_hash(&artifact.prompt_version),
            prompt_version: artifact.prompt_version.clone(),
            tags: record.tags.clone(),
            last_seen: record.last_seen.unix_seconds(),
            last_seen_utc: record.last_seen.to_rfc3339(),
            local: local_of(&artifact.provider, config),
        }
    }
}

/// One Record, in full: the detail pane's content, and the input to its two
/// export actions.
///
/// The Explanation arrives as [`SectionView`]s, not as five flat fields: the
/// card renders the list it is given, so presentation and copy stay with the
/// surface while the section rules stay in core (ADR-0005).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DetailView {
    pub id: i64,
    pub level: String,
    pub source_language: String,
    pub native_language: String,
    pub provider: String,
    pub model: String,
    pub thinking: String,
    pub prompt_label: String,
    pub prompt_version: String,
    /// The same hash, abbreviated the one way this crate abbreviates it.
    pub prompt_short: String,
    pub artifact_version: u32,
    pub created_at: String,
    pub generated_at: String,
    pub last_seen: String,
    /// The same moment in seconds, for the sidebar's local grouping.
    pub last_seen_unix: i64,
    pub local: bool,
    pub tags: Vec<String>,
    /// The card's content as the section list core renders: the order, the
    /// empty-section policy and every Gloss's settled shape come from there, so
    /// the window draws values instead of re-deriving them (ADR-0005).
    pub sections: Vec<SectionView>,
    /// The whole Record as the five-section markdown document, which is what
    /// "复制 markdown" puts on the clipboard and what "导出这一条" writes.
    ///
    /// Rendered here rather than in the frontend because
    /// [`plainly_core::render`] is the one renderer of an Explanation, and a
    /// second renderer in JavaScript would be a second thing to keep in step.
    pub markdown: String,
}

impl DetailView {
    /// The detail for one Record, with the configuration that says whether the
    /// model that answered is on this machine.
    pub fn of(record: &Record, config: &Config) -> Self {
        let artifact = &record.artifact;
        let explanation = &artifact.explanation;

        Self {
            id: record.id,
            level: artifact.level.to_string(),
            source_language: artifact.source_language.clone(),
            native_language: artifact.native_language.clone(),
            provider: artifact.provider.clone(),
            model: artifact.model.clone(),
            thinking: artifact.thinking.as_str().to_string(),
            prompt_label: artifact.prompt_label.clone(),
            prompt_version: artifact.prompt_version.clone(),
            prompt_short: short_hash(&artifact.prompt_version),
            artifact_version: artifact.artifact_version,
            created_at: artifact.created_at.to_rfc3339(),
            generated_at: artifact.generated_at.to_rfc3339(),
            last_seen: record.last_seen.to_rfc3339(),
            last_seen_unix: record.last_seen.unix_seconds(),
            local: local_of(&artifact.provider, config),
            tags: record.tags.clone(),
            sections: render::sections(
                &artifact.passage,
                explanation,
                &render::EmptyNotes {
                    grammar: Some(STRINGS.grammar_not_needed),
                    key_help: Some(STRINGS.glosses_empty),
                },
            )
            .into_iter()
            .map(section_view)
            .collect(),
            markdown: render::markdown(&artifact.passage, explanation),
        }
    }
}

/// One section of the detail card, as the window draws it.
///
/// `kind` is the section's identity for the stylesheet (`data-kind`), `title` is
/// the interface copy, and `body` says which of the three drawings the section
/// wants. The list this comes in is core's order: the frontend renders the array
/// it is handed and decides none of those things (ADR-0005).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SectionView {
    pub kind: &'static str,
    pub title: &'static str,
    pub body: SectionBodyView,
}

/// A section's body, in the terms the window's template draws it in.
///
/// Internally tagged for the wire: `{ "type": "prose", "text": … }`,
/// `{ "type": "note", "text": … }` and `{ "type": "glosses", "glosses": […] }`.
/// `Note` is the sentence a surface says where the model left a section empty —
/// not the Explanation talking — which is what lets the card keep its quieter
/// material for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SectionBodyView {
    Prose { text: String },
    Note { text: String },
    Glosses { glosses: Vec<GlossView> },
}

/// One Gloss, as the wire carries it: the expression, its explanation, and the
/// Example only when there is one.
///
/// The presence and the text are core's decision ([`plainly_core::render::GlossView`]);
/// this type exists because the wire is the desktop crate's contract and core's
/// render types stay free of serde (ADR-0005).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GlossView {
    pub expression: String,
    pub gloss: String,
    /// `null` for a Record generated before Examples existed (ADR 0002), for one
    /// whose model left the Example empty, and for one that answered only
    /// whitespace; otherwise the trimmed sentence.
    pub example: Option<String>,
}

impl From<render::GlossView> for GlossView {
    fn from(gloss: render::GlossView) -> Self {
        Self {
            expression: gloss.expression,
            gloss: gloss.gloss,
            example: gloss.example,
        }
    }
}

/// One core section as the wire form, with the window's own title attached.
fn section_view(section: render::Section) -> SectionView {
    let (kind, title) = section_identity(section.kind);
    SectionView {
        kind,
        title,
        body: match section.body {
            render::Body::Prose(text) => SectionBodyView::Prose { text },
            render::Body::Note(text) => SectionBodyView::Note { text },
            render::Body::Glosses(glosses) => SectionBodyView::Glosses {
                glosses: glosses.into_iter().map(GlossView::from).collect(),
            },
        },
    }
}

/// The two names a section needs on the wire: its identity for the stylesheet
/// (and the wire test) and its heading in the interface's own copy.
///
/// One table rather than two matches, because a new section has to appear in
/// both and a forgotten arm should fail where the section is built.
fn section_identity(kind: render::SectionKind) -> (&'static str, &'static str) {
    match kind {
        render::SectionKind::Original => ("original", STRINGS.detail_original),
        render::SectionKind::ComprehensibleEnglish => {
            ("comprehensible-english", STRINGS.detail_comprehensible)
        }
        render::SectionKind::KeyHelp => ("key-help", STRINGS.detail_glosses),
        render::SectionKind::Grammar => ("grammar", STRINGS.detail_grammar),
        render::SectionKind::Translation => ("translation", STRINGS.detail_translation),
    }
}

/// Where an export was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportOffer {
    /// The file the export is in.
    pub path: String,
    /// What is in it, for the sentence the window says afterwards.
    pub summary: String,
}

/// The one renderer of an export, so that the two global exports and the
/// single-record export cannot drift from the CLI's (spec §9, tickets/10).
pub fn export_text(format: plainly_core::ExportFormat, records: &[Record]) -> String {
    match format {
        plainly_core::ExportFormat::Markdown => export::markdown(records),
        plainly_core::ExportFormat::Raw => export::raw_csv(records),
    }
}

/// The Passage on one line, cut to [`SNIPPET`] characters.
///
/// Whitespace is collapsed first: a Passage is a paragraph, and a row that
/// showed its line breaks would be a row of varying height.
pub fn snippet(passage: &str) -> String {
    let line = passage.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut snippet: String = line.chars().take(SNIPPET).collect();
    if line.chars().count() > SNIPPET {
        snippet.push('…');
    }
    snippet
}

/// The first [`SHORT_HASH`] characters of a prompt hash, taken by character so
/// that a hand-edited file cannot split one.
pub fn short_hash(hash: &str) -> String {
    hash.chars().take(SHORT_HASH).collect()
}

/// A moment as `YYYY-MM-DD HH:MM:SS` in **UTC**, with the trailing `Z` dropped.
///
/// The window groups its sidebar by local days, which it does itself; this is
/// the tooltip form of the same instant, where the offset would only be
/// confusing. Deriving it here rather than parsing RFC 3339 in JavaScript keeps
/// one spelling of a moment in the process.
pub fn utc_text(moment: Timestamp) -> String {
    let text = moment.to_rfc3339();
    text.trim_end_matches('Z').replace('T', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snippet_collapses_whitespace_and_cuts_to_the_limit() {
        assert_eq!(snippet("a\n  b\tc"), "a b c");

        let long: String = "word ".repeat(40);
        let cut = snippet(&long);
        assert!(cut.ends_with('…'), "{cut:?} should be marked as cut");
        assert_eq!(cut.chars().count(), SNIPPET + 1);
    }

    #[test]
    fn a_short_hash_is_a_prefix_of_the_whole_one() {
        let hash = "9f2c1d4b6a8e0f3c5d7b9a1e2f4c6d8b0a2e4f6c8d0b2a4e6f8c0d2b4a6e8f0c";
        assert_eq!(short_hash(hash), "9f2c1d4b");
    }

    #[test]
    fn a_utc_text_drops_the_designator_and_keeps_the_shape() {
        let moment = Timestamp::from_unix_seconds(1_760_000_000).expect("in range");
        assert_eq!(utc_text(moment), "2025-10-09 08:53:20");
    }
}
