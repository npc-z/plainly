//! Splitting a long input into Passages.
//!
//! The threshold is the promise the completion budget is sized against, so these
//! tests are about the boundaries: where a split may fall, and what a chunk may
//! never grow past (spec §6).

use plainly_core::split::{
    CHUNK_WORDS, OUTPUT_TOKENS_PER_WORD, PANEL_CHUNK_LIMIT, TooManyChunks, chunks, panel_chunks,
};

/// `count` distinct words, so a test can tell not only how many survived but in
/// which order.
fn words(count: usize) -> String {
    (0..count)
        .map(|word| format!("w{word}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The same, closed like a sentence.
fn sentence(count: usize) -> String {
    format!("{}.", words(count))
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// The spec's numbers, pinned: the threshold, the measured output rate and the
/// panel limit are choices a change has to make deliberately, and the budget has
/// to cover the chunk.
#[test]
fn the_threshold_the_rate_the_panel_limit_and_the_budget_are_the_spec_numbers() {
    assert_eq!(CHUNK_WORDS, 150);
    assert_eq!(OUTPUT_TOKENS_PER_WORD, 6);
    assert_eq!(PANEL_CHUNK_LIMIT, 5);

    // Spec §6 pairs the threshold with the completion budget at the measured
    // rate; `chat.rs` holds the same pairing, against the spec's 1200 floor, as a
    // compile-time assert.
    let needed = CHUNK_WORDS * OUTPUT_TOKENS_PER_WORD;
    assert!(
        plainly_core::MAX_TOKENS as usize >= needed,
        "{CHUNK_WORDS} words need about {needed} output tokens"
    );
}

/// Whitespace with no words in it is no Passage at all: the surface refuses it
/// before asking, and the splitter does not invent an empty Passage for it.
#[test]
fn whitespace_only_text_is_no_passage_at_all() {
    assert_eq!(chunks(""), Vec::<String>::new());
    assert_eq!(chunks(" \n\n\t \n"), Vec::<String>::new());
}

/// A Passage that is already one Passage is handed over exactly as it came:
/// splitting is the only thing that rewrites the text.
#[test]
fn an_input_within_the_threshold_is_one_chunk_verbatim() {
    let passage = "  The committee went to ground.\n\nNobody could find it.  \n";

    assert_eq!(chunks(passage), vec![passage.to_string()]);
}

/// No chunk grows past the threshold, and a paragraph that fits is preferred
/// whole over any boundary inside it.
#[test]
fn paragraphs_are_packed_up_to_the_threshold_and_no_further() {
    let first = words(CHUNK_WORDS * 2 / 3);
    let second = words(CHUNK_WORDS * 2 / 3);
    let third = words(CHUNK_WORDS / 3);
    let passage = format!("{first}\n\n{second}\n\n{third}");

    let chunks = chunks(&passage);

    assert_eq!(chunks.len(), 2, "{chunks:?}");
    assert_eq!(chunks[0], first, "the second paragraph would not fit");
    assert_eq!(chunks[1], format!("{second}\n\n{third}"));
    for chunk in &chunks {
        assert!(word_count(chunk) <= CHUNK_WORDS, "{chunk:?}");
    }
}

/// A paragraph that is over the threshold on its own has no boundary inside it
/// to prefer, so the split drops to the sentences.
#[test]
fn a_paragraph_over_the_threshold_drops_to_sentence_boundaries() {
    let first = sentence(CHUNK_WORDS * 2 / 3);
    let second = sentence(CHUNK_WORDS * 2 / 3);
    let third = sentence(CHUNK_WORDS * 2 / 3);
    let passage = format!("{first} {second} {third}");

    let chunks = chunks(&passage);

    assert_eq!(chunks.len(), 3, "{chunks:?}");
    assert_eq!(chunks[0], first);
    assert_eq!(chunks[1], second);
    assert_eq!(chunks[2], third);
    for chunk in &chunks {
        assert!(
            chunk.ends_with('.'),
            "a split fell inside a sentence: {chunk:?}"
        );
        assert!(word_count(chunk) <= CHUNK_WORDS, "{chunk:?}");
    }
}

/// A terminator inside a word is not a sentence end: `3.14` stays inside the
/// sentence it is part of, even when that sentence shares a chunk with another.
#[test]
fn a_terminator_inside_a_word_is_not_a_sentence_boundary() {
    let first = sentence(CHUNK_WORDS * 2 / 3);
    let middle = "See 3.14 for the details.";
    let last = sentence(CHUNK_WORDS * 2 / 3);
    let passage = format!("{first} {middle} {last}");

    let chunks = chunks(&passage);

    assert_eq!(chunks.len(), 2, "{chunks:?}");
    assert_eq!(chunks[0], format!("{first} {middle}"));
    assert_eq!(chunks[1], last);
}

/// A sentence that is itself over the threshold has no sentence boundary below
/// it, and the threshold is a promise about the request's size, so the last
/// resort is a word boundary: a fragment is better than a truncated answer.
#[test]
fn a_sentence_over_the_threshold_is_split_at_a_word_boundary() {
    let passage = words(CHUNK_WORDS * 2 + 10);

    let chunks = chunks(&passage);

    assert_eq!(chunks.len(), 3, "{chunks:?}");
    assert_eq!(word_count(&chunks[0]), CHUNK_WORDS);
    assert_eq!(word_count(&chunks[1]), CHUNK_WORDS);
    assert_eq!(word_count(&chunks[2]), 10);

    // Every word survives, once, in order.
    let rejoined: Vec<&str> = chunks
        .iter()
        .flat_map(|chunk| chunk.split_whitespace())
        .collect();
    assert_eq!(rejoined, passage.split_whitespace().collect::<Vec<_>>());
}

/// The panel's limit is a judgement a surface can reuse rather than a rule the
/// splitter applies: the CLI takes the same input without being refused.
#[test]
fn the_panel_refuses_more_than_five_chunks_while_the_splitter_does_not() {
    let long = (0..PANEL_CHUNK_LIMIT + 1)
        .map(|_| words(CHUNK_WORDS))
        .collect::<Vec<_>>()
        .join("\n\n");

    let error = panel_chunks(&long).expect_err("six chunks is over the limit");

    assert_eq!(
        error,
        TooManyChunks {
            chunks: PANEL_CHUNK_LIMIT + 1,
            limit: PANEL_CHUNK_LIMIT,
        }
    );
    assert_eq!(
        chunks(&long).len(),
        PANEL_CHUNK_LIMIT + 1,
        "the CLI path is not refused"
    );

    let fits = (0..PANEL_CHUNK_LIMIT)
        .map(|_| words(CHUNK_WORDS))
        .collect::<Vec<_>>()
        .join("\n\n");
    assert_eq!(
        panel_chunks(&fits).expect("five fits").len(),
        PANEL_CHUNK_LIMIT
    );
}
