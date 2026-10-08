//! Splitting a long input into Passages.
//!
//! One Explanation is one Passage (spec §4), so an input longer than a Passage
//! may carry is *cut* into Passages rather than grown into a single request. The
//! threshold is a time budget, not a size budget: the output side is what hurts
//! first, and a cut-off answer is malformed JSON, which reads as a model fault
//! while the root cause is the request.
//!
//! The rule, in order (spec §6):
//!
//! 1. paragraphs, packed up to [`CHUNK_WORDS`];
//! 2. a paragraph over the threshold on its own drops to sentence boundaries;
//! 3. a sentence over the threshold on its own drops to word boundaries.
//!
//! Step 3 is not in the spec's wording, but it is what keeps the threshold a
//! promise: a single sentence can be any length, and the alternative to a
//! fragment is a request the completion budget may not cover.
//!
//! The panel's [`PANEL_CHUNK_LIMIT`] is a judgement a surface asks for with
//! [`panel_chunks`] rather than a rule [`chunks`] applies: the CLI is the long
//! input's proper home and takes the same input without being refused.

/// The most words one Passage may carry, by whitespace count.
///
/// 150 words is roughly 900 output tokens and 4.5 seconds (spec §6), and it is
/// **paired** with the completion budget: raising this without raising
/// `chat::MAX_TOKENS` gets the chunk truncated, which surfaces as a JSON failure
/// rather than as a budget failure. `chat.rs` holds the compile-time assert that
/// keeps the pair from drifting apart.
pub const CHUNK_WORDS: usize = 150;

/// The output tokens one word of a chunk costs to answer, measured (spec §6:
/// 输出 token ≈ 6 × 输入词数).
///
/// The completion budget's floor is sized from this, and `chat.rs` asserts the
/// floor against it, so the rate lives in one place rather than in each of them.
pub const OUTPUT_TOKENS_PER_WORD: usize = 6;

/// The most Passages a panel request may carry: five, about 750 words.
///
/// What this cuts off is the failure mode of pasting far more than one Passage
/// into the panel — dozens of requests, dozens of Records, dozens of seconds — so
/// the panel refuses and points at the CLI. The CLI does not inherit the limit
/// (spec §6).
pub const PANEL_CHUNK_LIMIT: usize = 5;

/// The input splits into more Passages than a panel request may carry.
///
/// The count travels with the refusal so the surface can say how far over it is;
/// the wording a learner reads belongs to that surface, not here, so this is a
/// statement of the fact rather than a message to paste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{chunks} Passages is more than the {limit} a panel request carries")]
pub struct TooManyChunks {
    /// How many Passages the input splits into.
    pub chunks: usize,
    /// The most a panel request may carry: [`PANEL_CHUNK_LIMIT`].
    pub limit: usize,
}

/// The text as Passages, in the order it was written.
///
/// Text that is already one Passage comes back byte for byte — including the
/// whitespace around it — because only a split rewrites it. Every other chunk is
/// the original span it came from, so a Passage always reads as it was written.
///
/// Whitespace with no words in it is no Passage at all, so it comes back as an
/// empty list rather than as one empty Passage; what to say about that belongs to
/// the surface that asked.
pub fn chunks(passage: &str) -> Vec<String> {
    let spans = pack(atoms(passage));

    match spans.as_slice() {
        [] => Vec::new(),
        [_] => vec![passage.to_string()],
        _ => spans
            .iter()
            .map(|&(start, end)| passage[start..end].to_string())
            .collect(),
    }
}

/// The same split, refused when it is more than a panel request may carry.
pub fn panel_chunks(passage: &str) -> Result<Vec<String>, TooManyChunks> {
    let chunks = chunks(passage);

    if chunks.len() > PANEL_CHUNK_LIMIT {
        return Err(TooManyChunks {
            chunks: chunks.len(),
            limit: PANEL_CHUNK_LIMIT,
        });
    }
    Ok(chunks)
}

/// One piece of the input that may stand as a Passage on its own, as a byte
/// range into the original text.
struct Atom {
    start: usize,
    end: usize,
    words: usize,
}

/// The input as the smallest pieces the rule allows a split to fall between.
fn atoms(passage: &str) -> Vec<Atom> {
    let mut atoms = Vec::new();

    for (paragraph_start, paragraph_end) in paragraphs(passage) {
        let paragraph = &passage[paragraph_start..paragraph_end];
        let count = word_count(paragraph);
        if count <= CHUNK_WORDS {
            atoms.push(Atom {
                start: paragraph_start,
                end: paragraph_end,
                words: count,
            });
            continue;
        }

        for (sentence_start, sentence_end) in sentences(paragraph) {
            let start = paragraph_start + sentence_start;
            let end = paragraph_start + sentence_end;
            let count = word_count(&passage[start..end]);
            if count <= CHUNK_WORDS {
                atoms.push(Atom {
                    start,
                    end,
                    words: count,
                });
            } else {
                atoms.extend(by_word(passage, start, end));
            }
        }
    }

    atoms
}

/// Every `(start, end)` byte range that may be a chunk, greedy in order: the
/// next atom joins the open chunk while it fits, and starts a new one when it
/// does not. Every atom is at most [`CHUNK_WORDS`] long, so no chunk is.
fn pack(atoms: Vec<Atom>) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut open: Option<(usize, usize, usize)> = None;

    for atom in atoms {
        match open {
            Some((start, _, words)) if words + atom.words <= CHUNK_WORDS => {
                open = Some((start, atom.end, words + atom.words));
            }
            Some((start, end, _)) => {
                spans.push((start, end));
                open = Some((atom.start, atom.end, atom.words));
            }
            None => open = Some((atom.start, atom.end, atom.words)),
        }
    }

    if let Some((start, end, _)) = open {
        spans.push((start, end));
    }
    spans
}

/// The paragraphs of `text`, as byte ranges with their surrounding whitespace
/// trimmed: a paragraph break is a blank line, and the lines between two of them
/// are one paragraph however they are wrapped.
fn paragraphs(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start: Option<usize> = None;
    let mut end = 0;
    let mut cursor = 0;

    while cursor < text.len() {
        let line_end = text[cursor..]
            .find('\n')
            .map_or(text.len(), |offset| cursor + offset);
        if text[cursor..line_end].trim().is_empty() {
            if let Some(start) = start.take() {
                spans.push(trimmed(text, start, end));
            }
        } else {
            start.get_or_insert(cursor);
            end = line_end;
        }
        cursor = line_end + 1;
    }

    if let Some(start) = start {
        spans.push(trimmed(text, start, end));
    }
    spans
}

/// The sentences of one paragraph, as byte ranges into it.
///
/// A boundary is a `.`, `!` or `?`, with any closing quote or bracket after it,
/// followed by whitespace or the paragraph's end. A terminator followed directly
/// by another character — `3.14` — is inside a word and is not a boundary; an
/// abbreviation that ends a word, `e.g.` or `Mr.`, looks exactly like the end of
/// a sentence and is treated as one. Nothing here is a parser: a paragraph only
/// reaches this when it is over the threshold on its own, and a split at the
/// wrong full stop is a smaller error than a request the budget may not cover.
fn sentences(text: &str) -> Vec<(usize, usize)> {
    const TERMINATORS: [char; 3] = ['.', '!', '?'];
    const CLOSERS: [char; 5] = ['"', '\'', ')', ']', '”'];

    let mut spans = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();

    while let Some((at, ch)) = chars.next() {
        if !TERMINATORS.contains(&ch) {
            continue;
        }

        let mut end = at + ch.len_utf8();
        while let Some(&(next_at, next_ch)) = chars.peek() {
            if !CLOSERS.contains(&next_ch) {
                break;
            }
            end = next_at + next_ch.len_utf8();
            chars.next();
        }

        match chars.peek() {
            // A terminator inside a word ("3.14") is not a boundary.
            Some(&(_, next_ch)) if !next_ch.is_whitespace() => {}
            Some(&(next_at, _)) => {
                spans.push(trimmed(text, start, end));
                start = next_at;
            }
            None => {
                spans.push(trimmed(text, start, end));
                start = text.len();
            }
        }
    }

    if start < text.len() {
        spans.push(trimmed(text, start, text.len()));
    }
    spans
}

/// One atom per word: the last resort, for a sentence that is itself over the
/// threshold and has no boundary below it that still leaves English.
fn by_word(text: &str, start: usize, end: usize) -> Vec<Atom> {
    let mut atoms = Vec::new();
    let mut word_start: Option<usize> = None;

    for (offset, ch) in text[start..end].char_indices() {
        let at = start + offset;
        if ch.is_whitespace() {
            if let Some(word_start) = word_start.take() {
                atoms.push(Atom {
                    start: word_start,
                    end: at,
                    words: 1,
                });
            }
        } else {
            word_start.get_or_insert(at);
        }
    }

    if let Some(word_start) = word_start {
        atoms.push(Atom {
            start: word_start,
            end,
            words: 1,
        });
    }
    atoms
}

/// `text[start..end]` with its surrounding whitespace excluded, still as offsets
/// into `text`.
fn trimmed(text: &str, start: usize, end: usize) -> (usize, usize) {
    let slice = &text[start..end];
    let lead = slice.len() - slice.trim_start().len();
    (start + lead, start + slice.trim_end().len())
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}
