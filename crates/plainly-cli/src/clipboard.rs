//! Reading the session clipboard, as data, for `explain --clipboard`.
//!
//! The CLI keeps its distance from GTK and from the Wayland client libraries: it
//! asks `wl-paste` what the clipboard holds and hands the answer to core as
//! data, where the sensitive-content rule lives (spec §11). The panel reads the
//! same clipboard through the data-control protocol directly (tickets/13);
//! `wl-paste` is what a headless surface has instead, and it is the fallback the
//! spec already names.
//!
//! Only the types something reads are read: `wl-paste` answers one type per run,
//! so the Passage costs one round trip and the password hint costs another only
//! when the owner published it. A value the owner will not give is handed over
//! as "not read" rather than guessed at — what half a criterion means is core's
//! call, not this module's.

use std::process::Command;

use plainly_core::DisplaySession;
use plainly_core::SessionRefusal;
use plainly_core::clipboard::{Clipboard, MimeType, PASSWORD_HINT};

/// The program that reaches the clipboard from a client that has no window.
const WL_PASTE: &str = "wl-paste";

/// What `wl-paste` says when there is no selection at all.
///
/// It is the command's own wording, matched because there is no exit code that
/// distinguishes "nothing is copied" from "the session cannot be reached". A
/// version that words it differently costs an exit code of 1 instead of 2, not
/// a wrong answer.
const NOTHING_COPIED: &str = "Nothing is copied";

/// Why the clipboard could not be read.
#[derive(Debug)]
pub enum Error {
    /// `wl-paste` is not installed.
    Missing,
    /// There is no selection to read: the clipboard is empty.
    Empty,
    /// There is a selection, and none of it is text Plainly can read as a
    /// Passage — a copied image, say. Asking for the generic `text` type is what
    /// makes this a decision rather than a roll of the dice: without it
    /// `wl-paste` falls back to any offered type and would hand over an image's
    /// bytes as if they were a Passage.
    NotText,
    /// The session refuses this read before it is attempted: X11, or a machine
    /// with no Wayland session at all (spec §1). Reported as that rather than as
    /// `wl-paste`'s own complaint about a socket, because the person's next step
    /// is different — not "fix the clipboard" but "use Wayland, or use the
    /// command line, which needs no display".
    ///
    /// The refusal carries its own sentence and cannot be built for a Wayland
    /// session, so this arm cannot be reached with "nothing is wrong".
    Refused(SessionRefusal),
    /// `wl-paste` ran and failed: no data-control, no session to talk to.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Missing => {
                f.write_str("cannot read the clipboard: `wl-paste` (wl-clipboard) is not on PATH")
            }
            Error::Empty => f.write_str("the clipboard is empty"),
            Error::NotText => f.write_str("the clipboard holds no text"),
            Error::Refused(refusal) => refusal.fmt(f),
            Error::Failed(reason) => write!(f, "cannot read the clipboard: {reason}"),
        }
    }
}

/// The clipboard as data: the Passage it holds, and the MIME types its owner
/// published with the value of the password hint when it published one.
///
/// The session is settled first and the read is not attempted on one v0 refuses,
/// because the answer is already known: `wl-paste` reaches the clipboard through
/// Wayland, so on X11 it can only fail, and its failure would report a socket
/// rather than the exclusion (spec §1).
pub fn read() -> Result<Clipboard, Error> {
    if let Some(refusal) = DisplaySession::from_process_env().refusal("reading the clipboard") {
        return Err(Error::Refused(refusal));
    }

    let mut types = Vec::new();
    for mime in list_types()? {
        // The hint is the only type a rule reads a value from. Every other type
        // is kept as the fact that it was published, without paying a round trip
        // for bytes nothing looks at; a hint whose value will not come is left
        // unread, and core decides what that means.
        let value = if mime == PASSWORD_HINT {
            ask(&["--no-newline", "--type", &mime], Error::Failed).ok()
        } else {
            None
        };
        types.push(MimeType { mime, value });
    }

    // `text` is `wl-paste`'s generic name for "pick an offered text type"; an
    // explicit one is what keeps a copied image from being read as a Passage.
    let passage = text(&["--no-newline", "--type", "text"], text_refused)?;
    Ok(Clipboard { passage, types })
}

/// The types the clipboard published, in the order `wl-paste` lists them.
fn list_types() -> Result<Vec<String>, Error> {
    Ok(text(&["--list-types"], listing_refused)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// Why a listing failed: an empty clipboard is the ordinary case and is told
/// apart from a session that cannot be reached at all.
fn listing_refused(reason: String) -> Error {
    if reason.contains(NOTHING_COPIED) {
        Error::Empty
    } else {
        Error::Failed(reason)
    }
}

/// Why a text request failed: the session cannot be reached, the clipboard
/// emptied itself under us, or it never held text in the first place.
///
/// Unlike a listing, this request speaks about one type — a session that cannot
/// be reached and a clipboard that offers no text fail it with different
/// meanings, and only the second is a usage error.
fn text_refused(reason: String) -> Error {
    if reason.contains(NOTHING_COPIED) {
        Error::Empty
    } else if is_session_failure(&reason) {
        Error::Failed(reason)
    } else {
        // `wl-paste` ran and spoke about the selection rather than about the
        // session: an image on the clipboard, say, rather than a broken one.
        Error::NotText
    }
}

/// Whether `wl-paste` is complaining about the session rather than about the
/// clipboard's contents.
///
/// The phrases are the ones `wl-clipboard` itself uses when it cannot reach a
/// compositor — "Failed to connect to a Wayland server", "XDG_RUNTIME_DIR is not
/// set", "compositor does not support the data-control protocol". A version that
/// words it differently costs a less precise message, not a wrong answer: either
/// way the read failed and the run stops.
fn is_session_failure(reason: &str) -> bool {
    ["wayland", "compositor", "XDG_RUNTIME_DIR", "data-control"]
        .iter()
        .any(|phrase| reason.to_lowercase().contains(&phrase.to_lowercase()))
}

/// One `wl-paste` run whose answer is text.
fn text(args: &[&str], refused: fn(String) -> Error) -> Result<String, Error> {
    Ok(String::from_utf8_lossy(&ask(args, refused)?).into_owned())
}

/// Ask `wl-paste` for something and give back the bytes it printed, or why it
/// would not. `refused` names the failure the caller has a use for.
fn ask(args: &[&str], refused: fn(String) -> Error) -> Result<Vec<u8>, Error> {
    let output = Command::new(WL_PASTE)
        .args(args)
        .output()
        .map_err(start_error)?;

    if !output.status.success() {
        return Err(refused(reason(&output)));
    }
    Ok(output.stdout)
}

/// Why `wl-paste` could not even be started.
fn start_error(error: std::io::Error) -> Error {
    match error.kind() {
        std::io::ErrorKind::NotFound => Error::Missing,
        _ => Error::Failed(error.to_string()),
    }
}

/// What `wl-paste` said when it failed. An empty complaint is still a reason:
/// the status is what is left of it.
fn reason(output: &std::process::Output) -> String {
    let said = String::from_utf8_lossy(&output.stderr);
    let said = said.trim();
    if said.is_empty() {
        format!("wl-paste exited with {}", output.status)
    } else {
        said.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exclusion is stated, not echoed: a person on X11 reads what is
    /// unsupported and what still works, not a socket error.
    #[test]
    fn the_x11_refusal_names_the_session_and_the_way_out() {
        let refusal = DisplaySession::X11
            .refusal("reading the clipboard")
            .expect("X11 is refused");
        let message = Error::Refused(refusal).to_string();

        assert!(message.contains("X11"), "{message}");
        assert!(message.contains("Wayland"), "{message}");
        assert!(message.contains("clipboard"), "{message}");
        assert!(message.contains("plainly"), "{message}");
    }

    /// Both refusals read as a read that did not happen, and neither of them is
    /// mistaken for a clipboard that held no text: those send a person to
    /// different places.
    #[test]
    fn a_refused_session_is_told_apart_from_a_clipboard_without_text() {
        for session in [DisplaySession::X11, DisplaySession::None] {
            let refusal = session
                .refusal("reading the clipboard")
                .expect("neither of these is Wayland");
            let refused = Error::Refused(refusal).to_string();
            let text = Error::NotText.to_string();

            assert_ne!(refused, text);
            assert!(
                refused.contains("X11") || refused.contains("no Wayland session"),
                "{refused}"
            );
            assert!(text.contains("no text"), "{text}");
        }
    }

    /// `wl-clipboard`'s own words for "there is no compositor" are recognized as
    /// a session failure rather than as a selection that offers no text.
    #[test]
    fn the_readers_own_words_for_a_missing_session_are_recognized() {
        for reason in [
            "Failed to connect to a Wayland server",
            "XDG_RUNTIME_DIR is not set",
            "the compositor does not support the data-control protocol",
        ] {
            assert!(
                is_session_failure(reason),
                "{reason:?} is about the session"
            );
        }

        assert!(
            !is_session_failure("no suitable type"),
            "a selection with no text is about the clipboard, not the session"
        );
    }
}
