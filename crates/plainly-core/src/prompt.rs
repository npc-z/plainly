//! What Plainly asks the model for.
//!
//! The prompt is content, not code: it is the calibrated `v6-synthesis` from
//! `.scratch/plainly/bench`, whose two evidenced levers are the accuracy rules
//! (no invented detail, proper nouns kept, understatement glossed) and the
//! explicit Grammar wording, plus "it must be a real restatement" as insurance
//! against the staged variant that returned the Passage verbatim.
//!
//! It has exactly two structured parameters, and both are already part of the
//! Lookup Key: the Level and the Native Language. They are substituted here so
//! the model sees "B2" and "Chinese"; the version below is computed from the
//! *unsubstituted* prompt, because a changed substitution is a different
//! Lookup Key rather than a different prompt.
//!
//! v0 ships the factory prompt alone. tickets/07 adds the append-only user
//! appendix and the level descriptor table, and folds both into the hash; that
//! is why [`version`] takes no arguments and is documented as "the prompt data
//! in effect".

use sha2::{Digest, Sha256};

use crate::Level;

/// The name a Record carries so a person can tell which prompt produced it.
///
/// The hash from [`version`] is the real version; this is the label for humans,
/// and the two are stored together.
pub const PROMPT_LABEL: &str = "v6-synthesis";

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

/// The system prompt for one run: the shipped prompt with both parameters set.
pub fn system_prompt(level: Level, native_language: &str) -> String {
    FACTORY_PROMPT
        .replace("{{LEVEL}}", level.as_str())
        .replace("{{NATIVE}}", native_language)
}

/// The SHA-256 of the prompt data in effect, as lowercase hex.
///
/// The placeholders are deliberately not substituted: the Level and the Native
/// Language are separate components of the Lookup Key, so replacing them here
/// would give the same question two identities. SHA-256 rather than something
/// cheaper, for the reason the Lookup Key gives: a collision means silently
/// reusing an answer to a different question.
pub fn version() -> String {
    hex(&Sha256::digest(FACTORY_PROMPT.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
