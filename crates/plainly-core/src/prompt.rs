//! What Plainly asks the model for, and the version that names it.
//!
//! The prompt is content, not code, and it has three parts (spec §5):
//!
//! - **The factory prompt**, shipped inside the application and immutable within
//!   a build. It is the calibrated `v6-synthesis` with one change: the level
//!   site carries the level's *meaning* as well as its label, because a bare
//!   label is close to sending nothing — two of three levels produced
//!   byte-identical paraphrases without a meaning beside them
//!   (`bench/FINDINGS.md`). The C1 wording no longer says "keep most of the
//!   original structure": that sentence fought the prompt's own rule ("it must
//!   be a real restatement") and lost, which is the likely reason B2 and C1 were
//!   indistinguishable.
//! - **The user's appendix**, appended after every factory rule. Append-only is
//!   structural, not a promise in a document: nothing here prepends it or merges
//!   it into the factory text, so no appendix can delete a factory rule. The
//!   factory rules are what keep two people's histories comparable; a user who
//!   wants to counteract one writes the opposite rule after it, which is the
//!   weaker capability the spec records rather than hides.
//! - **The level descriptors**, the wording handed to the model beside a level
//!   label. They ship with the application, a user may override a row, and an
//!   empty row means the shipped wording.
//!
//! [`Prompt::version`] is the SHA-256 of all three, taken over the *unsubstituted*
//! factory text: the Level and the Native Language are already separate
//! components of the Lookup Key (spec §9), so substituting them here would give
//! one question two identities. Because the descriptor table is part of the
//! hashed data, "change a descriptor and forget to bump the version" is not a
//! discipline anyone has to keep — it cannot happen (issue 09).

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use crate::config::{Level, Prompts};
use crate::retry::FailureKind;

/// The name a Record carries so a person can tell which prompt produced it.
///
/// The hash from [`Prompt::version`] is the real version; this is the label for
/// humans, and the two are stored together. It names the *factory* prompt: an
/// appendix changes the hash without changing which factory prompt it extends.
pub const PROMPT_LABEL: &str = "v7-descriptors";

/// The shipped system prompt, with its placeholders unreplaced.
///
/// JSON mode on providers that cannot enforce a schema requires the answer to
/// be asked for in so many words, which is why the word JSON and the shape of
/// every field are in here rather than left to the schema alone.
pub const FACTORY_PROMPT: &str = r#"You explain hard English to a learner whose reading level is {{LEVEL}} and whose native language is {{NATIVE}}.

Return a single JSON object with exactly these four keys.

- "comprehensible": the passage's meaning restated in more frequent, familiar English. Keep the original's meaning, tense and aspect, modality, negation and logical links: nothing added, nothing dropped. Keep subordinate and relative clauses, conditionals, passive voice and discourse links; simplify vocabulary first, and reach for syntax only when vocabulary alone leaves the meaning out of reach. The result must read as natural adult English pitched at {{LEVEL}}. It must be a real restatement: never return the passage unchanged.
- "glosses": one plain-English line for each expression that actually blocks comprehension: an unusual meaning, a phrasal verb, an idiom, a meaning-bearing collocation. Each entry is {"expression": "<the expression, quoted from the passage>", "gloss": "<plain-English explanation>"}. Gloss chunks, not single tokens: "go to ground" is one unit. Use an empty array when nothing blocks it. Never write a gloss that is harder than the expression it explains.
- "grammar": a plain-English explanation of a structural obstacle. Use this whenever the shape of the sentence is part of what blocks comprehension: inversion, a clause whose attachment is unclear, a construction whose difficulty is structural rather than lexical. If an expression is hard because of where it sits in the sentence rather than because of its own meaning, explain it here rather than under "glosses". Use null only when the sentence's shape is genuinely not an obstacle.
- "translation": the passage's meaning in natural {{NATIVE}}. Carry the meaning; do not map word for word.

Accuracy rules, which outrank brevity:

- Never add detail the passage does not contain. Do not invent a cause, an agent, or an object that is not on the page.
- If you are not confident what an idiom, a title, or a technical term means, describe what it does in this sentence instead of asserting a meaning. A hedged gloss is better than a confident wrong one.
- Keep the passage's register and its proper nouns: "the Continent" is not "a continent".
- Understatement and irony block comprehension as much as an idiom does. When a phrase signals that the reality is stronger than the words ("to put it mildly", "not best pleased"), gloss that phrase itself and carry its force into the paraphrase, instead of smoothing it away.

The passage is authoritative: never correct it, never simplify it in place, and quote its expressions verbatim in "expression". Do not annotate every word, only the blockers."#;

/// The meaning that ships beside `level`.
///
/// Three of the four rows are the source contract's own wording, unchanged:
/// A1 and A2 share one meaning ("very common words, short sentences, concrete")
/// because that is how the contract states it, and B2 is its B2 sentence. Only
/// C1 is reworded, and that reword is the one change spec §5 asks for: "keep
/// most of the original structure" fought the prompt's own rule that the result
/// must be a real restatement, and the rule won — so the descriptor says what it
/// means instead of contradicting it.
///
/// A `match` rather than a table to be searched: a new [`Level`] is then a
/// compile error here, not a panic inside [`Prompt::descriptor`].
pub fn factory_descriptor(level: Level) -> &'static str {
    match level {
        Level::A1 => "very common words, short sentences, concrete",
        Level::A2 => "very common words, short sentences, concrete",
        Level::B2 => "English-first; nuance and collocations",
        Level::C1 => {
            "subtle meaning, register, idiom and style kept — and still a full \
             restatement, never the passage unchanged"
        }
    }
}

/// The prompt data in effect: the factory text, the user's appendix and the
/// level descriptors.
///
/// The factory text is a constant rather than a field: it cannot be edited by a
/// user, and an application update replacing it is the point of the split. What
/// varies is held here, and only here — the CLI, the panel and the settings page
/// all ask this one value what to send and what version to stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    appendix: String,
    /// The rows the user reworded. A level absent here uses the shipped wording,
    /// so the shipped table never has to be copied into a field.
    descriptors: BTreeMap<Level, String>,
}

impl Prompt {
    /// The shipped prompt, with no appendix and the shipped descriptors.
    pub fn factory() -> Self {
        Self::from_config(&Prompts::default())
    }

    /// The prompt data a configuration describes.
    pub fn from_config(prompts: &Prompts) -> Self {
        let mut descriptors = BTreeMap::new();

        for (label, meaning) in &prompts.level_descriptors {
            // A key that is not a level is a typo in a hand-edited file. It is
            // ignored rather than refused, because unknown keys in general load
            // fine so that a newer file still works with an older Plainly
            // (config.rs); `plainly config set` is where a bad level is caught
            // loudly.
            let Ok(level) = label.parse::<Level>() else {
                continue;
            };
            let meaning = meaning.trim();
            if !meaning.is_empty() {
                descriptors.insert(level, meaning.to_string());
            }
        }

        Self {
            // Trailing whitespace is not a change to the question: `appendix = ""`
            // and an appendix of newlines are the same prompt, and so they have
            // one version.
            appendix: prompts.appendix.trim().to_string(),
            descriptors,
        }
    }

    /// The meaning handed to the model beside `level`'s label.
    pub fn descriptor(&self, level: Level) -> &str {
        match self.descriptors.get(&level) {
            Some(meaning) => meaning,
            None => factory_descriptor(level),
        }
    }

    /// The human-readable name of the factory prompt this extends.
    pub fn label(&self) -> &'static str {
        PROMPT_LABEL
    }

    /// Whether this is the factory prompt: no appendix and no descriptor
    /// overridden.
    pub fn is_factory(&self) -> bool {
        self.appendix.is_empty()
            && Level::ALL
                .into_iter()
                .all(|level| self.descriptor(level) == factory_descriptor(level))
    }

    /// The system prompt for one run: the factory prompt with both parameters
    /// set, and the appendix after it.
    pub fn system_prompt(&self, level: Level, native_language: &str) -> String {
        let filled = fill(level, self.descriptor(level), native_language);

        if self.appendix.is_empty() {
            return filled;
        }
        format!("{filled}\n\n{}", self.appendix)
    }

    /// The SHA-256 of the prompt data in effect, as lowercase hex.
    ///
    /// SHA-256 rather than something cheaper, for the reason the Lookup Key
    /// gives: a collision means silently reusing an answer to a different
    /// question.
    pub fn version(&self) -> String {
        hex(&Sha256::digest(self.canonical().as_bytes()))
    }

    /// The factory prompt to retry with when this prompt is what failed the
    /// contract at `level`, or `None` when there is nothing to fall back to.
    ///
    /// The test is about *this run's* prompt text, not about the prompt data in
    /// general: a row reworded for another level is never substituted into a run
    /// at this level, so falling back would send byte-identical text and could
    /// only fail the same way. The version still covers the whole table, because
    /// the table is prompt data (spec §5) — this is about what a retry could
    /// change, not about identity.
    ///
    /// Two cases are deliberately not a fallback. A provider failure is not the
    /// prompt's doing (only [`FailureKind::is_contract`] failures are), and a run
    /// already sending the factory text has nothing to fall back from — telling
    /// the user to use what they are using would be noise. The run does not apply
    /// this itself: spec §5 refuses to silently undo the user's prompt, so the
    /// retry happens on the user's say-so, and the Artifact it produces carries
    /// *that* prompt's version because the request was built from it.
    pub fn fallback_after(&self, kind: FailureKind, level: Level) -> Option<Prompt> {
        let changed_this_run =
            !self.appendix.is_empty() || self.descriptor(level) != factory_descriptor(level);

        (kind.is_contract() && changed_this_run).then(Prompt::factory)
    }

    /// The bytes the version covers.
    ///
    /// Every field is length-prefixed, so an appendix containing whatever it
    /// likes cannot shift one field into another: two different bodies of prompt
    /// data always produce different bytes, and therefore different versions.
    fn canonical(&self) -> String {
        let mut out = String::new();
        push_field(&mut out, FACTORY_PROMPT);
        push_field(&mut out, &self.appendix);
        for level in Level::ALL {
            push_field(&mut out, level.as_str());
            push_field(&mut out, self.descriptor(level));
        }
        out
    }
}

/// A level label with its meaning beside it: what `{{LEVEL}}` becomes.
fn labelled(level: Level, meaning: &str) -> String {
    format!("{} ({meaning})", level.as_str())
}

/// The factory prompt with both parameters filled in, in one pass.
///
/// One pass rather than two `.replace` calls: the descriptor is text a person
/// wrote, and text that has just been substituted must not be scanned again for
/// placeholders — otherwise `{{NATIVE}}` inside a descriptor would be treated as
/// template instead of as data.
fn fill(level: Level, meaning: &str, native_language: &str) -> String {
    let mut out = String::with_capacity(FACTORY_PROMPT.len() + meaning.len() * 2);
    let mut rest = FACTORY_PROMPT;

    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];

        if let Some(after) = tail.strip_prefix("{{LEVEL}}") {
            out.push_str(&labelled(level, meaning));
            rest = after;
        } else if let Some(after) = tail.strip_prefix("{{NATIVE}}") {
            out.push_str(native_language);
            rest = after;
        } else {
            // A brace run that is not one of the two placeholders: copy it and
            // carry on, so the scan cannot stall on it.
            out.push_str("{{");
            rest = &tail[2..];
        }
    }

    out.push_str(rest);
    out
}

/// Append a field with its byte length in front.
fn push_field(out: &mut String, field: &str) {
    out.push_str(&field.len().to_string());
    out.push(':');
    out.push_str(field);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
