//! Reading the clipboard, as data — and writing it, for the panel's 复制.
//!
//! Two ways, because the panel has two shapes (spec §12): a layer surface never
//! has focus, so GTK's own clipboard API does not work on it and both the read
//! and the write go through the data-control protocol; the fallback window does
//! have focus, and there GTK's clipboard is the one that works. The two reads hand
//! core the same data — the Passage, and the MIME types its owner published with
//! the value of the password hint when it published one.
//!
//! A copy is not the mirror image of a read, and the difference is worth stating:
//! writing to the clipboard makes this process the **owner** of the selection, so
//! the text has to be served for as long as it is on the clipboard. That service
//! is a thread which outlives the button press — see [`copy`].

use std::io::Read;

use gtk::glib;
use plainly_core::clipboard::{Clipboard, MimeType, PASSWORD_HINT};
use wl_clipboard_rs::copy::{MimeType as Copy, Options, Source};
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

/// Put text on the clipboard, from whichever shape this panel has.
///
/// `published` is called with whether the text reached the clipboard at all, and
/// it is called on the **main loop** — from `copy_gtk` directly, from
/// `copy_data_control` through a channel — because the caller's next move is to
/// set a label. A `bool` rather than a reason: the status line shows the same
/// short phrase either way, and a diagnostic nobody displays is a promise the type
/// would not keep.
///
/// The two paths differ in *when* they can answer, and that is why they are
/// written differently rather than because they fail differently. GTK's `set_text`
/// answers on the spot; the data-control arm is a Wayland connection that is not
/// `Send`, so it has to be prepared on the thread that will serve it.
pub fn copy(text: &str, data_control: bool, published: impl FnOnce(bool) + 'static) {
    match data_control {
        true => copy_data_control(text, published),
        false => published(copy_gtk(text).is_ok()),
    }
}

/// Serve `text` as the regular selection over data-control.
///
/// The one subtlety is the lifetime: a selection is not a value held by the
/// compositor but a promise held by its owner, so the thread that publishes the
/// text has to outlive the button press and keep answering paste requests. It is
/// a detached thread of this process, and it ends when the panel does — the
/// window between "copied" and "pasted" is what it has to cover, and the panel is
/// on screen for exactly that window.
///
/// The whole operation happens **on that thread**, including preparing it, which
/// is why nothing about owning a selection can cross a thread boundary after it is
/// made. What does cross is the answer to "did it get published at all", which is
/// all a single press can know: whether another client ever *takes* the selection
/// is answered by them, later, and a failure of that service is not a failure of
/// the copy.
fn copy_data_control(text: &str, published: impl FnOnce(bool) + 'static) {
    let (sender, receiver) = async_channel::bounded(1);
    // The bytes travel with the thread: a `&str` cannot outlive the button press,
    // and the thread that owns the selection has to hold the text for as long as
    // the selection lives.
    let text = text.to_string();

    std::thread::spawn(move || {
        // `prepare_copy` **asserts** `foreground`, so this is not a preference:
        // leaving it false panics the serving thread before anything is published,
        // and the caller sees a closed channel rather than a copy. Serving on our
        // own thread is what this function does anyway — `serve()` blocks, and the
        // thread is here to block.
        let mut options = Options::new();
        options.foreground(true);

        let prepared = match options.prepare_copy(
            Source::Bytes(text.as_bytes().to_vec().into_boxed_slice()),
            Copy::Text,
        ) {
            Ok(prepared) => {
                let _ = sender.send_blocking(true);
                prepared
            }
            Err(_) => {
                let _ = sender.send_blocking(false);
                return;
            }
        };

        // Serving blocks until the selection is taken over by another client, or
        // until this process exits. A service failure here is not reported: the
        // text is on the clipboard from the person's side, and there is no
        // remaining surface on which to tell them otherwise.
        let _ = prepared.serve();
    });

    // Back on the main loop, where a label may be touched.
    glib::spawn_future_local(async move {
        if let Ok(published_ok) = receiver.recv().await {
            published(published_ok);
        }
    });
}

/// Put text on GTK's own clipboard: the focused-window path, where the panel has
/// the focus a selection needs.
///
/// The store is chosen rather than left to GTK (`set_text` picks for one), because
/// `CLIPBOARD` is the selection a person's paste reads: `set_text` would not be
/// wrong, it would just be a second way to say the same thing.
fn copy_gtk(text: &str) -> Result<(), String> {
    let display =
        gtk::gdk::Display::default().ok_or_else(|| "no display to put the text on".to_string())?;
    let clipboard = gtk::Clipboard::for_display(&display, &gtk::gdk::SELECTION_CLIPBOARD);

    // `gtk_clipboard_set_text` has no failure of its own: whether the selection
    // was actually taken is answered later, by the owner of the selection, which
    // is not something a button press can wait for.
    clipboard.set_text(text);
    Ok(())
}
