//! The clipboard as data, and the one place sensitive content is judged.
//!
//! The rule is tiny and the reason it is tested hard is the cost asymmetry: a
//! password sent to a cloud provider and written into the history cannot be
//! recalled, while a Passage Plainly declines to explain costs almost nothing
//! (spec §11, issues/18).

use plainly_core::clipboard::{Clipboard, MimeType, PASSWORD_HINT, Reading};

/// The Passage the fixture clipboard holds, whichever way the test expects it to
/// be judged.
const PASSAGE: &str = "The manager had already gone to ground.";

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

fn read(types: Vec<MimeType>) -> Reading {
    clipboard(types).reading()
}

/// The whole point: a password manager marking its content secret stops the run.
#[test]
fn the_password_hint_set_to_secret_is_refused() {
    assert_eq!(read(vec![hint(Some("secret"))]), Reading::Sensitive);
}

/// The hint is a two-step signal: the type alone says nothing. KeePassXC's own
/// non-secret writes publish the same type with the value `public`.
#[test]
fn the_same_hint_set_to_public_is_explained() {
    assert_eq!(
        read(vec![hint(Some("public"))]),
        Reading::Passage(PASSAGE.to_string())
    );
}

/// The type published without its value read is only half the criterion, and
/// half is not a reason to send: a platform that could not read the value says
/// so rather than looking public.
#[test]
fn the_hint_published_without_a_value_is_unread_rather_than_public() {
    assert_eq!(read(vec![hint(None)]), Reading::Unread);
}

/// The value is matched exactly: another type carrying `secret` is another
/// type, and `Secret` is another value.
#[test]
fn a_different_type_or_value_carrying_secret_is_not_the_hint() {
    let other_type = MimeType {
        mime: "text/plain;charset=utf-8".to_string(),
        value: Some(b"secret".to_vec()),
    };
    assert_eq!(
        read(vec![other_type]),
        Reading::Passage(PASSAGE.to_string())
    );

    assert_eq!(
        read(vec![hint(Some("Secret"))]),
        Reading::Passage(PASSAGE.to_string()),
        "the convention's value is lowercase"
    );
}

/// A clipboard that never published the hint is an ordinary Passage, and it
/// comes back byte for byte.
#[test]
fn a_clipboard_without_the_hint_is_an_ordinary_passage() {
    let passage = "  Not until the auditors had gone did he admit it.  \n";

    assert_eq!(
        Clipboard {
            passage: passage.to_string(),
            types: vec![MimeType {
                mime: "text/plain;charset=utf-8".to_string(),
                value: Some(b"anything".to_vec()),
            }],
        }
        .reading(),
        Reading::Passage(passage.to_string())
    );
}

/// The hint is honoured wherever it sits in the list, and a sensitive type
/// beside harmless ones still refuses the whole clipboard.
#[test]
fn the_hint_is_found_among_other_types_in_any_order() {
    let harmless = MimeType {
        mime: "text/html".to_string(),
        value: None,
    };

    assert_eq!(
        read(vec![harmless.clone(), hint(Some("secret"))]),
        Reading::Sensitive
    );
    assert_eq!(
        read(vec![hint(Some("secret")), harmless]),
        Reading::Sensitive
    );
}
