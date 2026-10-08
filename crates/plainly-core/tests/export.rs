//! The three exports: one markdown document and two CSVs.
//!
//! An export is a function of Records — the history in its own order — so the
//! fixtures come through a real (in-memory) store rather than being built by
//! hand: what a learner exports is what the history holds.
//!
//! The two CSVs are read back with a small quote-aware reader, because their
//! cells are allowed to hold commas and line breaks: counting lines would be
//! wrong the moment a Passage has two paragraphs, and asserting the escaping by
//! eye is how a CSV ends up broken in a spreadsheet.

mod support;

use plainly_core::store::Lookup;
use plainly_core::{Artifact, Explanation, Gloss, Level, Record, Store, Thinking, export};

/// A Passage and an Explanation whose fields hold no comma, quote or line break,
/// so a row can be read in the test exactly as a spreadsheet would read it.
const BOTTLED: &str = "He bottled it at the last minute.";
const BOTTLED_ANSWER: &str = r#"{
  "comprehensible": "He lost his nerve right at the end.",
  "glosses": [
    { "expression": "bottle it", "gloss": "lose your nerve" },
    { "expression": "at the last minute", "gloss": "at the very end" }
  ],
  "grammar": null,
  "translation": "他在最后一刻怯场了。"
}"#;

/// The day the fixture instants fall on, from `date -u -d @… +%F`: an answer the
/// tests did not compute the way the code computes it.
const FIRST_DAY: &str = "2025-10-09";
const SECOND_DAY: &str = "2025-10-10";

fn lookup(passage: &str) -> Lookup<'_> {
    Lookup {
        passage,
        level: Level::B2,
        source_language: "en",
        native_language: "Chinese",
        prompt_version: support::PROMPT_VERSION,
        provider: "deepseek",
        model: "deepseek-flash",
        thinking: Thinking::Off,
    }
}

fn bottled() -> Explanation {
    Explanation::parse(BOTTLED_ANSWER).expect("the fixture holds the contract")
}

fn committee() -> Explanation {
    Explanation::parse(support::ANSWER).expect("the fixture holds the contract")
}

fn with_grammar() -> Explanation {
    Explanation::parse(support::ANSWER_WITH_GRAMMAR).expect("the fixture holds the contract")
}

/// Store an Artifact and hand back the Record the store made of it.
fn kept(store: &mut Store, artifact: &Artifact) -> Record {
    store
        .remember(&lookup(&artifact.passage).key(), artifact)
        .expect("the store accepts it")
}

/// A store holding one record, for the tests that need a real one.
fn one(artifact: &Artifact) -> Record {
    let mut store = Store::in_memory().expect("an in-memory store opens");
    kept(&mut store, artifact)
}

/// Split a CSV document into rows of fields, the way a spreadsheet would:
/// quotes protect commas and line breaks, and a doubled quote is one quote.
///
/// Small on purpose. The point is to read the product through the interface its
/// consumer uses, not to reimplement a CSV library — and the escaping itself is
/// pinned by a worked example elsewhere in this file.
fn rows(document: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = document.chars().peekable();

    while let Some(character) = chars.next() {
        match (quoted, character) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, character) => field.push(character),
            (false, '"') => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut field)),
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, '\r') => {}
            (false, character) => field.push(character),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }

    rows
}

// ---------------------------------------------------------------------------
// Markdown: one document, one section per Record
// ---------------------------------------------------------------------------

#[test]
fn the_markdown_export_gives_every_record_a_heading_and_the_five_sections() {
    let first = one(&support::artifact_at(BOTTLED, bottled(), 1_760_000_000));
    let second = one(&support::artifact_at(
        support::PASSAGE_WITH_GRAMMAR,
        with_grammar(),
        1_760_086_400,
    ));

    let document = export::markdown(&[first.clone(), second.clone()]);

    assert!(
        document.contains(&format!(
            "## {FIRST_DAY} · B2 · deepseek/deepseek-flash · v7-descriptors\n"
        )),
        "the heading carries the date, the Level, the profile and the prompt label:\n{document}"
    );
    assert!(
        document.contains(&format!(
            "## {SECOND_DAY} · B2 · deepseek/deepseek-flash · v7-descriptors\n"
        )),
        "{document}"
    );
    assert_eq!(
        document.matches("\n## ").count() + usize::from(document.starts_with("## ")),
        2,
        "one section per Record:\n{document}"
    );
}

#[test]
fn a_markdown_export_shows_the_five_sections_in_order() {
    // Content, not composition: the export is read by a person, so what has to
    // hold is that every part of the Explanation is in the document under its
    // heading, in the prototype's order.
    let record = one(&support::artifact_at(support::PASSAGE, committee(), 1_760_000_000));

    let document = export::markdown(std::slice::from_ref(&record));

    let mut cursor = 0;
    for part in [
        "### Original",
        support::PASSAGE,
        "### Comprehensible English",
        "The committee tried hard to find out what had happened",
        "### Key Help",
        "- `go to ground` → hide so that nobody can find you",
        "### Translation",
        "委员会对此事进行了彻底调查",
    ] {
        let at = document[cursor..]
            .find(part)
            .unwrap_or_else(|| panic!("{part:?} is missing, or out of order, in:\n{document}"));
        cursor += at + part.len();
    }
}

#[test]
fn a_record_with_no_grammar_note_has_no_grammar_section() {
    let plain = one(&support::artifact_at(BOTTLED, bottled(), 1_760_000_000));
    let structural = one(&support::artifact_at(
        support::PASSAGE_WITH_GRAMMAR,
        with_grammar(),
        1_760_086_400,
    ));

    let document = export::markdown(&[plain, structural]);

    assert_eq!(
        document.matches("### Grammar").count(),
        1,
        "the model left one out, and an absent section is the model's answer:\n{document}"
    );
}

#[test]
fn the_records_of_a_markdown_export_keep_the_order_they_were_given() {
    let first = one(&support::artifact_at(BOTTLED, bottled(), 1_760_000_000));
    let second = one(&support::artifact_at(
        support::PASSAGE_WITH_GRAMMAR,
        with_grammar(),
        1_760_086_400,
    ));

    let document = export::markdown(&[second.clone(), first.clone()]);

    let later = document.find("Not until the auditors").expect("the second record");
    let earlier = document.find("He bottled it").expect("the first record");
    assert!(
        later < earlier,
        "the export does not re-sort what it is handed:\n{document}"
    );
}

#[test]
fn the_markdown_export_of_an_empty_history_is_empty() {
    assert_eq!(export::markdown(&[]), "");
}

// ---------------------------------------------------------------------------
// Anki CSV: one card per Gloss
// ---------------------------------------------------------------------------

/// The columns an Anki import maps onto its fields, as the spec lists them.
const ANKI_HEADER: &str =
    "expression,gloss,passage,level,native_language,provider,model,prompt_label";

/// A Record that found nothing to gloss.
const NOTHING_TO_GLOSS: &str = "The cat sat on the mat.";
const NOTHING_TO_GLOSS_ANSWER: &str = r#"{
  "comprehensible": "The cat sat on the mat.",
  "glosses": [],
  "grammar": null,
  "translation": "猫坐在垫子上。"
}"#;

/// An Expression and a Passage holding every character CSV has to protect.
const AWKWARD: &str = "First line.\nSecond line.";
const AWKWARD_ANSWER: &str = r#"{
  "comprehensible": "Two lines, said again.",
  "glosses": [
    { "expression": "well, \"yes\"", "gloss": "line one\nline two" }
  ],
  "grammar": null,
  "translation": "两行。"
}"#;

#[test]
fn the_anki_export_gives_every_gloss_its_own_row() {
    let record = one(&support::artifact_at(BOTTLED, bottled(), 1_760_000_000));

    let document = export::anki_csv(std::slice::from_ref(&record));

    assert_eq!(
        document,
        format!(
            "{ANKI_HEADER}\n\
             bottle it,lose your nerve,He bottled it at the last minute.,B2,Chinese,\
             deepseek,deepseek-flash,v7-descriptors\n\
             at the last minute,at the very end,He bottled it at the last minute.,B2,Chinese,\
             deepseek,deepseek-flash,v7-descriptors\n"
        ),
        "front, back, context, then the provenance"
    );
}

#[test]
fn a_record_with_no_glosses_makes_no_cards() {
    // The unit of review is the Gloss. A row with nothing on its front would
    // import as a card that cannot be answered, so such a Record is in the
    // markdown and raw exports instead.
    let bare = one(&support::artifact_at(
        NOTHING_TO_GLOSS,
        Explanation::parse(NOTHING_TO_GLOSS_ANSWER).expect("the fixture holds the contract"),
        1_760_000_000,
    ));
    let glossed = one(&support::artifact_at(BOTTLED, bottled(), 1_760_086_400));

    let document = export::anki_csv(&[bare, glossed]);

    assert_eq!(rows(&document).len(), 3, "the header and the two cards:\n{document}");
}

#[test]
fn a_gloss_with_a_blank_expression_makes_no_card() {
    // The schema cannot forbid an empty expression (ADR-0001: no length bounds),
    // so a model may return one; the export is where a card that cannot be
    // answered is left out.
    let mut explanation = bottled();
    explanation.glosses.push(Gloss {
        expression: "   ".to_string(),
        gloss: "nothing to put a question on".to_string(),
    });
    let record = one(&support::artifact_at(BOTTLED, explanation, 1_760_000_000));

    let document = export::anki_csv(std::slice::from_ref(&record));

    assert_eq!(
        rows(&document).len(),
        3,
        "the header and the two real cards:\n{document}"
    );
}

#[test]
fn a_csv_field_with_a_comma_a_quote_or_a_line_break_is_quoted_and_escaped() {
    let record = one(&support::artifact_at(
        AWKWARD,
        Explanation::parse(AWKWARD_ANSWER).expect("the fixture holds the contract"),
        1_760_000_000,
    ));

    let document = export::anki_csv(std::slice::from_ref(&record));

    assert_eq!(
        document,
        format!(
            "{ANKI_HEADER}\n\
             \"well, \"\"yes\"\"\",\"line one\nline two\",\"First line.\nSecond line.\",\
             B2,Chinese,deepseek,deepseek-flash,v7-descriptors\n"
        )
    );
    let read = rows(&document);
    assert_eq!(read[1][0], "well, \"yes\"");
    assert_eq!(read[1][1], "line one\nline two");
    assert_eq!(read[1][2], AWKWARD);
}

#[test]
fn the_anki_export_of_an_empty_history_is_the_header_alone() {
    assert_eq!(export::anki_csv(&[]), format!("{ANKI_HEADER}\n"));
}

// ---------------------------------------------------------------------------
// Raw CSV: one row per Record, for a spreadsheet
// ---------------------------------------------------------------------------

/// Every column of the raw export, in the order it writes them.
const RAW_HEADER: &str = "id,created_at,generated_at,last_seen,level,source_language,\
                          native_language,provider,model,thinking,prompt_label,prompt_version,\
                          artifact_version,tags,passage,comprehensible,glosses,grammar,translation";

fn nothing_to_gloss() -> Explanation {
    Explanation::parse(NOTHING_TO_GLOSS_ANSWER).expect("the fixture holds the contract")
}

#[test]
fn the_raw_export_gives_every_record_one_row_and_every_column() {
    let mut store = Store::in_memory().expect("an in-memory store opens");
    let first = kept(&mut store, &support::artifact_at(BOTTLED, bottled(), 1_760_000_000));
    store.tag(first.id, "work").expect("the record exists");
    let first = store
        .show(first.id)
        .expect("the store is readable")
        .expect("the record is there");
    let second = kept(
        &mut store,
        &support::artifact_at(NOTHING_TO_GLOSS, nothing_to_gloss(), 1_760_086_400),
    );

    let document = export::raw_csv(&[first.clone(), second.clone()]);
    let table = rows(&document);

    assert_eq!(table[0].join(","), RAW_HEADER, "the header is the columns");
    assert_eq!(table.len(), 3, "the header and one row per Record:\n{document}");

    let column = |name: &str| {
        table[0]
            .iter()
            .position(|heading| heading == name)
            .unwrap_or_else(|| panic!("no {name} column in {:?}", table[0]))
    };
    assert_eq!(table[1][column("id")], first.id.to_string());
    assert_eq!(table[1][column("level")], "B2");
    assert_eq!(table[1][column("source_language")], "en");
    assert_eq!(table[1][column("native_language")], "Chinese");
    assert_eq!(table[1][column("provider")], "deepseek");
    assert_eq!(table[1][column("model")], "deepseek-flash");
    assert_eq!(table[1][column("thinking")], "off");
    assert_eq!(table[1][column("prompt_label")], "v7-descriptors");
    assert_eq!(table[1][column("prompt_version")], support::PROMPT_VERSION);
    assert_eq!(table[1][column("tags")], "work");
    assert_eq!(table[1][column("passage")], BOTTLED);
    assert_eq!(
        table[1][column("comprehensible")],
        "He lost his nerve right at the end."
    );
    assert_eq!(table[1][column("translation")], "他在最后一刻怯场了。");
    assert_eq!(
        table[1][column("grammar")],
        "",
        "a Grammar note the model left out is an empty cell, not the word null"
    );
    assert_eq!(table[2][column("passage")], NOTHING_TO_GLOSS);
    assert_eq!(table[2][column("tags")], "", "a Record with no Tags");
    assert_eq!(table[2][column("glosses")], "");
}

#[test]
fn the_raw_export_puts_the_glosses_in_one_cell_as_several_lines() {
    let record = one(&support::artifact_at(BOTTLED, bottled(), 1_760_000_000));

    let document = export::raw_csv(std::slice::from_ref(&record));
    let table = rows(&document);
    let glosses = table[0]
        .iter()
        .position(|heading| heading == "glosses")
        .expect("a glosses column");

    assert_eq!(
        table[1][glosses], "bottle it → lose your nerve\nat the last minute → at the very end",
        "one cell, one line per Gloss"
    );
}

#[test]
fn the_raw_export_of_an_empty_history_is_the_header_alone() {
    assert_eq!(export::raw_csv(&[]), format!("{RAW_HEADER}\n"));
}
