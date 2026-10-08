//! Reading the clipboard, as data.
//!
//! Two ways, because the panel has two shapes (spec §12): a layer surface never
//! has focus, so GTK's own clipboard API does not work on it and the read goes
//! through the data-control protocol; the fallback window does have focus, and
//! there GTK's clipboard is the one that works. Both hand core the same data —
//! the Passage, and the MIME types its owner published with the value of the
//! password hint when it published one.

use std::io::Read;

use plainly_core::clipboard::{Clipboard, MimeType, PASSWORD_HINT};
use wl_clipboard_rs::paste::{self, ClipboardType, MimeType as Paste, Seat};

/// Why the clipboard could not be read.
#[derive(Debug)]
pub enum Error {
    /// There is no Passage to read: an empty clipboard, or nothing Plainly can
    /// read as text.
    NoPassage(String),
    /// The clipboard could not be read at all.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoPassage(reason) | Error::Failed(reason) => f.write_str(reason),
        }
    }
}

/// The clipboard through the data-control protocol: what a layer surface, which
/// never takes focus, has to use.
pub fn data_control() -> Result<Clipboard, Error> {
    let types = paste::get_mime_types_ordered(ClipboardType::Regular, Seat::Unspecified)
        .map_err(unreadable)?;

    // A value that will not come is handed over as "not read" rather than
    // guessed at: what half a criterion means is core's call.
    let hint = if types.iter().any(|mime| mime == PASSWORD_HINT) {
        bytes(Paste::Specific(PASSWORD_HINT)).ok()
    } else {
        None
    };

    let passage = text(Paste::Text)?;
    Ok(assemble(passage, types, hint))
}

/// The clipboard through GTK: what the focused-window fallback has to use.
pub fn focused_window(clipboard: &gtk::Clipboard) -> Result<Clipboard, Error> {
    // The Passage is read first so that an empty clipboard is told apart from a
    // type list that will not come; neither is a reason to send anything.
    let passage = clipboard
        .wait_for_text()
        .map(|text| text.to_string())
        .ok_or_else(|| Error::NoPassage("the clipboard holds no text".to_string()))?;

    // A target list that is not delivered is not "no marker": a secret-marked
    // owner that answers the text but hides its own types would otherwise be
    // explained, which is the inverse of the asymmetry (spec §11).
    let types: Vec<String> = clipboard
        .wait_for_targets()
        .ok_or_else(|| Error::Failed("cannot read the clipboard's type list".to_string()))?
        .iter()
        .map(|target| target.name().to_string())
        .collect();

    let hint = if types.iter().any(|mime| mime == PASSWORD_HINT) {
        clipboard
            .wait_for_contents(&gtk::gdk::Atom::intern(PASSWORD_HINT))
            .map(|data| data.data().to_vec())
    } else {
        None
    };

    Ok(assemble(passage, types, hint))
}

/// The Passage and the type list as core wants them: only the hint carries a
/// value, because it is the only type a rule reads.
fn assemble(passage: String, types: Vec<String>, hint: Option<Vec<u8>>) -> Clipboard {
    Clipboard {
        passage,
        types: types
            .into_iter()
            .map(|mime| MimeType {
                value: if mime == PASSWORD_HINT {
                    hint.clone()
                } else {
                    None
                },
                mime,
            })
            .collect(),
    }
}

/// One type's bytes, through data-control.
fn bytes(mime: Paste<'_>) -> Result<Vec<u8>, Error> {
    let (mut pipe, _mime) =
        paste::get_contents(ClipboardType::Regular, Seat::Unspecified, mime).map_err(unreadable)?;

    let mut value = Vec::new();
    pipe.read_to_end(&mut value)
        .map_err(|error| Error::Failed(format!("cannot read the clipboard: {error}")))?;
    Ok(value)
}

/// The clipboard as text, through data-control.
fn text(mime: Paste<'_>) -> Result<String, Error> {
    Ok(String::from_utf8_lossy(&bytes(mime)?).into_owned())
}

/// An empty clipboard and an unreadable one are different states: the first has
/// nothing to explain, the second is a session to fix. A compositor with no seat
/// at all is the second, not an empty clipboard: it is the clipboard that is
/// missing, not the Passage.
fn unreadable(error: paste::Error) -> Error {
    use paste::Error::*;

    match error {
        ClipboardEmpty | NoMimeType => Error::NoPassage(error.to_string()),
        error => Error::Failed(error.to_string()),
    }
}
