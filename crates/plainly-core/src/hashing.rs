//! Hashing the two versions Plainly derives from content.
//!
//! [`crate::prompt::Prompt::version`] and [`crate::store::Lookup::key`] both turn
//! a handful of strings into one identity, and both need the same two
//! properties: SHA-256, because a collision means silently answering a different
//! question, and an unambiguously framed material string, because a field must
//! not be able to shift itself into the next one.
//!
//! The framing lives here rather than in either caller so there is one argument
//! for why it is safe, and one place to change if it ever has to be.

use sha2::{Digest, Sha256};

/// Append a field with its byte length in front.
///
/// `["ab", "c"]` and `["a", "bc"]` must not produce the same material, or two
/// different prompts (or two different questions) could share one version. The
/// length is the field's own, so no separator can be smuggled in as content.
pub(crate) fn push_field(out: &mut String, text: &str) {
    out.push_str(&text.len().to_string());
    out.push(':');
    out.push_str(text);
}

/// The SHA-256 of `material`, as lowercase hex.
pub(crate) fn sha256_hex(material: &str) -> String {
    Sha256::digest(material.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
