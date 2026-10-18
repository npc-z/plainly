//! The two exports: one markdown document and one CSV.
//!
//! The spec (§9) fixes what each one is for, and the two are different shapes
//! rather than two spellings of one shape:
//!
//! - **Markdown** is for reading and for pasting into notes: one document, one
//!   section per Record, headed with what tells two Records apart, and the
//!   Explanation below in the five sections [`crate::render`] draws. A Record
//!   whose Grammar note is missing has no Grammar section, because a document
//!   that says nothing about grammar reads as "the model found none".
//! - **Raw CSV** is for a spreadsheet: one row per Record, with the Glosses as
//!   multi-line text in a single cell.
//!
//! Both are pure functions of `&[Record]`, and they keep the order they are
//! handed. The caller decides whether that is the history's own order
//! (`last_seen`, most recent first) or a single Record from the detail view.
//!
//! [`crate::render`] is the one renderer of an Explanation: the markdown export
//! wraps it rather than drawing a second copy of the five sections.

use crate::render;
use crate::store::Record;
use crate::{Explanation, Timestamp};

/// The columns of the raw export, in the order it writes them.
///
/// Every fact a learner can use: this is the export someone hands to another
/// tool, so a column left out here is a column they do not have. `lookup_key` is
/// the deliberate exception — it is the store's cache identity, a hash of the
/// Passage with the whole provider profile, which is the store's business and
/// noise in a spreadsheet. `id` is here because `history show` takes it.
const RAW_HEADER: &str = "id,created_at,generated_at,last_seen,level,source_language,\
                          native_language,provider,model,thinking,prompt_label,prompt_version,\
                          artifact_version,tags,passage,comprehensible,glosses,grammar,\
                          translation";

/// The whole history as one markdown document.
///
/// Ends in a newline when there is anything to export, and is empty when there is
/// not: a learner who has explained nothing gets an empty file rather than a file
/// that looks broken.
pub fn markdown(records: &[Record]) -> String {
    let sections: Vec<String> = records
        .iter()
        .map(|record| {
            format!(
                "## {} · {} · {}/{} · {}\n\n{}",
                day(record.artifact.created_at),
                record.artifact.level,
                record.artifact.provider,
                record.artifact.model,
                record.artifact.prompt_label,
                render::markdown(&record.artifact.passage, &record.artifact.explanation),
            )
        })
        .collect();

    sections.join("\n")
}

/// An instant's date, as `YYYY-MM-DD`.
///
/// The instant's one RFC 3339 shape starts with the date, so the date is the
/// first ten characters of it rather than a second calendar implementation. It is
/// the **UTC** date, which is the clock every other timestamp Plainly prints
/// reads; the interface's today/yesterday grouping reads the same instant in the
/// reader's own zone, so the two differ only for Records made within the reader's
/// offset of their midnight.
fn day(at: Timestamp) -> String {
    at.to_rfc3339().chars().take(10).collect()
}

/// One CSV line from its fields, without the line break.
fn csv_line(fields: &[&str]) -> String {
    fields
        .iter()
        .map(|field| csv_field(field))
        .collect::<Vec<_>>()
        .join(",")
}

/// One CSV field, quoted as far as it has to be.
///
/// A comma, a quote or a line break would otherwise end the field or the row —
/// Passages have commas and paragraphs, and a Gloss may quote the Passage — so
/// such a field is wrapped in quotes with every quote inside it doubled, which is
/// RFC 4180 and every spreadsheet read.
fn csv_field(text: &str) -> String {
    match text.contains([',', '"', '\n', '\r']) {
        true => format!("\"{}\"", text.replace('"', "\"\"")),
        false => text.to_string(),
    }
}

/// The history as one CSV row per Record, for a spreadsheet.
///
/// Everything a Record holds, minus `lookup_key`: this export exists so that a
/// learner can take their history somewhere else, and a column left out here is a
/// column they do not have. The Glosses share one cell as multi-line text rather
/// than spreading the Record over several rows, because the row is the unit a
/// spreadsheet sorts and filters by. The Tags share a cell as prose — `, `
/// between them — for the same reason: that cell is there to be read, and a Tag
/// whose own text holds `, ` reads as two there. The store is what a program
/// should ask which Tags a Record carries.
pub fn raw_csv(records: &[Record]) -> String {
    let mut document = String::from(RAW_HEADER);
    document.push('\n');

    for record in records {
        let artifact = &record.artifact;
        let explanation = &artifact.explanation;
        document.push_str(&csv_line(&[
            &record.id.to_string(),
            &artifact.created_at.to_rfc3339(),
            &artifact.generated_at.to_rfc3339(),
            &record.last_seen.to_rfc3339(),
            artifact.level.as_str(),
            &artifact.source_language,
            &artifact.native_language,
            &artifact.provider,
            &artifact.model,
            artifact.thinking.as_str(),
            &artifact.prompt_label,
            &artifact.prompt_version,
            &artifact.artifact_version.to_string(),
            &record.tags.join(", "),
            &artifact.passage,
            &explanation.comprehensible,
            &glosses_text(explanation),
            explanation.grammar.as_deref().unwrap_or(""),
            &explanation.translation,
        ]));
        document.push('\n');
    }

    document
}

/// The Glosses as the raw export's caller sees them in one cell: the
/// expression, its explanation and its Example, one per line and the Glosses one
/// after another, in the model's order.
///
/// The same three lines the document and the card draw, and deliberately without
/// the document's block quote around the Example: a spreadsheet cell is not a
/// document (see [`crate::render`]), and a `>` in a data cell is noise. A Gloss
/// with no Example is two lines. The Example's presence and text are settled by
/// [`render::GlossView::of`], the same value the document and the card draw.
fn glosses_text(explanation: &Explanation) -> String {
    explanation
        .glosses
        .iter()
        .map(render::GlossView::of)
        .map(|gloss| {
            let mut block = format!("{}\n  {}", gloss.expression, gloss.gloss);
            if let Some(example) = &gloss.example {
                block.push_str(&format!("\n  {example}"));
            }
            block
        })
        .collect::<Vec<_>>()
        .join("\n")
}
