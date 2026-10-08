//! What the panel is going to show, decided from what the clipboard held.
//!
//! The judgement is core's ([`Clipboard::reading`], [`split::panel_chunks`]);
//! this names the outcome so the surface can word each one differently, and so
//! the rule is reachable from a unit test without a display.

use plainly_core::clipboard::{Clipboard, Reading};
use plainly_core::split::{self, TooManyChunks};

/// What one trigger of the panel amounts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// The owner marked the content secret. Nothing is sent, nothing is stored,
    /// and there is no "explain anyway" (spec §11).
    Sensitive,
    /// The marker was published and its value could not be read. The same
    /// asymmetry applies, and the surface says which half is missing.
    Unread,
    /// There is no Passage to explain.
    Nothing,
    /// More Passages than one panel request may carry: the CLI or a manual split
    /// is the way out, and the refusal carries how far over it is.
    TooLong(TooManyChunks),
    /// The Passages to explain, in the clipboard's order.
    Passages(Vec<String>),
}

/// The panel's own decision, from the clipboard as data.
pub fn plan(clipboard: Clipboard) -> Plan {
    match clipboard.reading() {
        Reading::Sensitive => Plan::Sensitive,
        Reading::Unread => Plan::Unread,
        Reading::Passage(passage) => match split::panel_chunks(&passage) {
            Err(too_many) => Plan::TooLong(too_many),
            Ok(chunks) if chunks.is_empty() => Plan::Nothing,
            Ok(chunks) => Plan::Passages(chunks),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plainly_core::clipboard::{MimeType, PASSWORD_HINT};
    use plainly_core::split::{CHUNK_WORDS, PANEL_CHUNK_LIMIT};

    const PASSAGE: &str = "The committee had already gone to ground.";

    fn clipboard(types: Vec<MimeType>) -> Clipboard {
        Clipboard {
            passage: PASSAGE.to_string(),
            types,
        }
    }

    fn hint(value: Option<&str>) -> MimeType {
        MimeType {
            mime: PASSWORD_HINT.to_string(),
            value: value.map(|value| value.as_bytes().to_vec()),
        }
    }

    /// A paragraph of exactly `count` words, so a test can count Passages.
    fn paragraph(tag: &str, count: usize) -> String {
        (0..count)
            .map(|word| format!("{tag}{word}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn a_clipboard_marked_secret_is_planned_as_a_refusal() {
        assert_eq!(plan(clipboard(vec![hint(Some("secret"))])), Plan::Sensitive);
    }

    #[test]
    fn a_clipboard_whose_marker_was_not_read_is_planned_as_unread() {
        assert_eq!(plan(clipboard(vec![hint(None)])), Plan::Unread);
    }

    #[test]
    fn an_unmarked_clipboard_is_one_passage_to_explain() {
        assert_eq!(
            plan(clipboard(vec![hint(Some("public"))])),
            Plan::Passages(vec![PASSAGE.to_string()])
        );
    }

    #[test]
    fn an_empty_clipboard_is_nothing_to_explain() {
        assert_eq!(
            plan(Clipboard {
                passage: "  \n".to_string(),
                types: vec![],
            }),
            Plan::Nothing
        );
    }

    /// The panel's hard limit is core's, not a number the surface keeps: over it
    /// the plan is a refusal that carries how far over.
    #[test]
    fn more_than_the_panel_limit_is_planned_as_too_long() {
        let passage = (0..PANEL_CHUNK_LIMIT + 1)
            .map(|_| paragraph("w", CHUNK_WORDS))
            .collect::<Vec<_>>()
            .join("\n\n");

        assert_eq!(
            plan(Clipboard {
                passage,
                types: vec![],
            }),
            Plan::TooLong(TooManyChunks {
                chunks: PANEL_CHUNK_LIMIT + 1,
                limit: PANEL_CHUNK_LIMIT,
            })
        );
    }

    /// Up to the limit the Passages arrive in the clipboard's order.
    #[test]
    fn several_passages_arrive_in_order() {
        let paragraphs: Vec<String> = (0..3)
            .map(|n| paragraph(&format!("p{n}w"), CHUNK_WORDS))
            .collect();
        let passage = paragraphs.join("\n\n");

        assert_eq!(
            plan(Clipboard {
                passage,
                types: vec![],
            }),
            Plan::Passages(paragraphs)
        );
    }
}
