//! Reading the clipboard without taking focus, for the copy listener.
//!
//! The main window never has to read the clipboard: it is a focused window, and
//! everything it does with a selection is *writing* it ([`crate::copy`]). The
//! listener is the other way round — it has no surface at all and no focus to
//! ask for, because taking focus on every copy would be the opposite of what
//! "explain on copy" is for. So this is the panel's reader (spec §12) again, in
//! the one shape the listener needs: the Passage, plus the MIME types its owner
//! published, which is half of the data-control offer and the half that says
//! what was published with it.
//!
//! **The duplication with `plainly-panel` is deliberate and the reason is a
//! boundary, not convenience.** Core must not depend on Wayland — it is the
//! crate that has to compile for a macOS or Windows build (spec §1) — and the
//! panel is a binary-only crate, so sharing this would mean a new crate holding
//! a Wayland client, which is the same trade tickets/14 refused for the
//! twenty-line protocol probe. What is duplicated is the *reading*, not the
//! criterion: the judgement of what the types mean lives in
//! `plainly_core::clipboard` and is written once (spec §11). The listener reads
//! the types only to tell one clipboard state from the next; the panel reads
//! them to decide whether anything may be sent, and that is the one that decides.

use std::io::Read;
use std::time::{Duration, Instant};

use wl_clipboard_rs::paste::Seat;
use wl_clipboard_rs::watch::{ClipboardEvent, ClipboardType, Offer};

/// What one clipboard change published.
///
/// The Passage and the types, and nothing judged: which types matter is core's
/// question, and the listener does not answer it — it only notices that the
/// answer would be different from last time. The types are part of the reading
/// because they are part of the offer: the same words copied from a terminal and
/// from a browser are two different offers, and the panel that the second one
/// opens is the one that explains what the second copy put there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    /// The Passage the clipboard holds, verbatim.
    pub passage: String,
    /// Every MIME type its owner published, in the order it published them.
    pub types: Vec<String>,
}

/// Why a clipboard change could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unreadable {
    /// Nothing to explain: the clipboard was cleared, or it holds something
    /// with no text in it (an image, a file list). Not a failure of the
    /// session, so it is not reported as one.
    Nothing,
    /// The clipboard could not be read at all — no seat, no protocol, a
    /// compositor that stopped answering. This one is worth a line of its own,
    /// and the watch ends on it.
    Failed(String),
}

/// What a clipboard change turned out to be.
pub type Reading = Result<Observation, Unreadable>;

/// How long the clipboard's owner is given to deliver what it offered.
///
/// Reading an offer is a request *to the application that owns the selection*,
/// and nothing in the protocol makes it answer: a hung editor, a browser waiting
/// on a modal, a program that is being debugged all leave the pipe open and
/// silent. Two things behind the listener would wait with it — the panel is
/// spawned only after this read, and the stop channel is not looked at while it
/// is blocked — so a read that never ends is a window that will not close and a
/// `--watch` that will not stop.
///
/// A well-behaved owner answers in well under a millisecond, so this is not a
/// budget anyone can notice; it is the difference between "slow" and "gone".
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// Read the contents of the offer a watcher just reported.
///
/// `receive` is asked for one type rather than a list, because the marker's
/// *value* is read by the surface that acts on it: the panel asks for
/// `x-kde-passwordManagerHint` as well, and the criterion is core's
/// (`Clipboard::reading`). A listener that read the value too would be a second
/// place where "secret" is spelled.
///
/// The read is bounded by [`READ_TIMEOUT`] and it is done on a thread of its own
/// because the pipe has no timeout of its own. A read that outlives the deadline
/// is **detached rather than killed** — there is no way to interrupt it from
/// here — and the wait that owns it moves on; the thread ends when its owner
/// closes the pipe, or with the process. The cost of that is one blocked thread
/// per stalled owner, which is the price of not blocking the listener on one.
pub fn observe(offer: &mut Offer<'_>, mime_types: &[String]) -> Reading {
    let Some(mime) = text_type(mime_types) else {
        return Err(Unreadable::Nothing);
    };

    let mut pipe = offer
        .receive(mime)
        .map_err(|error| unreadable(&error.to_string()))?;
    let (read, contents) = std::sync::mpsc::channel::<Result<Vec<u8>, String>>();

    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = read.send(
            pipe.read_to_end(&mut bytes)
                .map(|_| bytes)
                .map_err(|error| format!("cannot read the clipboard: {error}")),
        );
    });

    match contents.recv_timeout(READ_TIMEOUT) {
        Ok(Ok(bytes)) => Ok(Observation {
            passage: String::from_utf8_lossy(&bytes).into_owned(),
            types: mime_types.to_vec(),
        }),
        Ok(Err(reason)) => Err(Unreadable::Failed(reason)),
        Err(_) => Err(Unreadable::Failed(format!(
            "the clipboard's owner did not deliver within {} seconds",
            READ_TIMEOUT.as_secs()
        ))),
    }
}

/// Which of an offer's types to read as the Passage.
///
/// The first text-ish one, by the same rule `wl-clipboard` uses: `wl-paste` with
/// the generic name `text` asks for exactly this, and a reader that hard-coded
/// `text/plain` would miss the applications that publish only
/// `text/plain;charset=utf-8` or `UTF8_STRING` (tickets/12).
///
/// An offer with no text type is the "nothing to explain" answer, which is why
/// an image on the clipboard produces no panel rather than an error.
fn text_type(mime_types: &[String]) -> Option<&str> {
    mime_types
        .iter()
        .find(|mime| is_text(mime))
        .map(String::as_str)
}

/// Whether a MIME type names text, as `wl-clipboard` decides it.
///
/// The list is `wl-clipboard`'s own `is_text`, because this is a question about
/// what applications copy rather than about Plainly: `STRING`, `UTF8_STRING` and
/// `TEXT` are the X11-era names GTK and terminals still publish, and the markup
/// and script types are what an editor offers when it copies a whole file.
fn is_text(mime: &str) -> bool {
    match mime {
        "TEXT" | "STRING" | "UTF8_STRING" => true,
        _ => {
            mime.starts_with("text/")
                || ["json", "script", "xml", "yaml", "csv", "ini"]
                    .iter()
                    .any(|name| mime.contains(name))
        }
    }
}

/// A failure of `receive`, told apart from an empty clipboard.
///
/// The two are different states with different sentences, and it is the same
/// split the panel's reader makes: an empty clipboard, or a type list with no
/// text in it, is "nothing to explain", while a compositor that will not answer
/// is something worth a line on stderr.
///
/// The library's messages are matched loosely on purpose. It words them, and a
/// reworded one must not turn "the clipboard is empty" into a failure worth
/// printing — but it also must not turn a broken session into silence, so the
/// fallback is the loud one.
fn unreadable(reason: &str) -> Unreadable {
    if reason.contains("empty") || reason.contains("No suitable type") {
        Unreadable::Nothing
    } else {
        Unreadable::Failed(reason.to_string())
    }
}

/// Watch the regular clipboard, one selection change at a time.
///
/// Only the regular clipboard is watched. The primary selection is what a text
/// *selection* is, not what a copy is: popping a panel every time somebody
/// highlights a word while reading would be a different feature, and not one
/// anybody asked for.
///
/// **A watcher is used for one wait and then dropped.** `wl-clipboard-rs` has
/// exactly one way out of a blocked `next_event`: cancel the watcher — and a
/// cancelled watcher is finished for good, because the pipe its `wait` polls is
/// still readable. Keeping one around would mean every wait after the first
/// returns instantly and a copy that arrived in between is never seen. So each
/// wait opens its own connection and lets it go, which also means the reconnect a
/// compositor restart needs is not a separate path: it is the same one.
pub struct Changes {
    /// The last reading the watch produced, so that "is there anything a panel
    /// could be asked for" can be answered without consuming an event.
    last: Option<Reading>,
}

impl Changes {
    /// Start watching.
    ///
    /// Nothing is read here: the listener's first [`Observer::quiet`] is what
    /// reports what the session holds, and that first reading is the arm.
    pub fn open() -> Result<Self, String> {
        // A connection is opened and dropped so that "there is no session" is
        // reported by `--watch` at startup rather than by the first copy, hours
        // later, when nobody is looking at stderr.
        drop(connect()?);
        Ok(Self { last: None })
    }

    /// Wait until the clipboard changes, or until the deadline.
    ///
    /// `Err` is the watch ending: the compositor stopped answering. It is told
    /// apart from a cancellation because the two want different things — one is a
    /// reason to ask again, and the other is a reason to stop being a listener.
    ///
    /// ## One connection per wait, and why it is not a poll
    ///
    /// `wl-clipboard-rs`'s `Watcher` has two properties that shape this function:
    ///
    /// - its **first** `next_event` reports the selection *already on the
    ///   clipboard* — a level, not an event — and returns at once rather than
    ///   waiting;
    /// - it is finished for good once it is cancelled, so a deadline cannot be put
    ///   on one and then waited on again.
    ///
    /// So this opens one connection, reads its first report as the level, and if
    /// that level is the reading already in hand it **keeps waiting on the same
    /// connection** until something genuinely changes or the deadline cancels it.
    /// Returning the level would make every wait report something, no wait would
    /// ever be quiet, and the settle window the listener counts could never close
    /// — a clipboard that was merely being watched would never open a panel.
    ///
    /// One call is therefore one slice of real time that reports either a change
    /// or silence, and never the clipboard repeating itself.
    fn change(&mut self, deadline: Instant) -> Result<Option<Reading>, String> {
        let mut refused: Option<String> = None;

        loop {
            let mut watcher = match connect() {
                Ok(watcher) => watcher,
                // A compositor that refused twice in a row is not having a blip,
                // and the sentence would be the same one.
                Err(reason) if refused.as_deref() == Some(reason.as_str()) => {
                    return Err(reason);
                }
                // One refusal is not a reason to stop being a listener: a
                // compositor that is restarting turns connections away for a
                // moment and then takes them again. Pause a slice so that a
                // compositor which keeps refusing is not hammered until the
                // deadline.
                Err(reason) => {
                    refused = Some(reason);
                    std::thread::sleep(crate::watch::POLL);
                    continue;
                }
            };

            // The slice starts now, not before the connection: a slow connect
            // must not eat into it.
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return Ok(None);
            };

            let cancel = watcher.cancel_handle();
            let (stop, stopped) = std::sync::mpsc::channel::<()>();

            // The timer is armed **before** the wait, so that a change cannot
            // arrive in the gap and then be lost to a cancellation meant for it.
            let timer = std::thread::spawn(move || {
                // A timeout is the deadline. Anything else is the wait having
                // ended first — the sender dropped, or the stop sent — and then
                // there is nothing to cancel.
                if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = stopped.recv_timeout(left)
                {
                    cancel.cancel();
                }
            });

            // The level first, then whatever the connection has to report until
            // the deadline. Only the *first* report can be the level: everything
            // after it is an event, and an event is a change.
            let mut level = true;
            let outcome: Result<Option<Reading>, String> = loop {
                match event_to_reading(watcher.next_event()) {
                    Ok(Some(reading)) => {
                        let repeating = level && self.last.as_ref() == Some(&reading);
                        level = false;

                        if repeating {
                            continue;
                        }

                        self.last = Some(reading.clone());
                        break Ok(Some(reading));
                    }
                    // Cancelled: the deadline ran out with nothing to report.
                    Ok(None) => break Ok(None),
                    Err(reason) => break Err(reason),
                }
            };

            // The wait is over before the timer is told so, which is what keeps a
            // cancellation from landing on the *next* wait: the handle it holds is
            // this watcher's, and this watcher is about to be dropped.
            let _ = stop.send(());
            let _ = timer.join();

            // A compositor that stopped answering mid-connection is the one
            // ending there is: a later call opens a fresh connection, which is
            // what a restart needs.
            return outcome;
        }
    }
}

/// One event, as the listener reads it.
///
/// `Err` carries the library's own sentence for a watch that ended; a cancelled
/// wait is `Ok(None)`, which is not an error but an empty slice.
fn event_to_reading(
    event: Result<Option<ClipboardEvent<'_>>, wl_clipboard_rs::paste::Error>,
) -> Result<Option<Reading>, String> {
    match event {
        Ok(Some(ClipboardEvent::Changed {
            mime_types,
            mut offer,
            ..
        })) => Ok(Some(observe(&mut offer, &mime_types))),
        // A cleared clipboard is a change like any other: it is what the next
        // copy is measured against, and it is what keeps "copy the same text
        // again" a new copy rather than a repeat.
        Ok(Some(ClipboardEvent::Cleared { .. })) => Ok(Some(Err(Unreadable::Nothing))),
        Ok(None) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

/// A fresh data-control watch.
fn connect() -> Result<wl_clipboard_rs::watch::Watcher, String> {
    wl_clipboard_rs::watch::Watcher::new(ClipboardType::Regular, Seat::Unspecified)
        .map_err(|error| error.to_string())
}

/// The listener watches the clipboard through this, and nothing else does.
impl crate::watch::Observer for Changes {
    /// Wait one slice: a change if one arrives, silence if the deadline does, and
    /// the end of the watch if the compositor stops answering.
    ///
    /// The deadline is honoured even when something was read: the *level* of the
    /// clipboard is re-reported by every connection, so a reading equal to the one
    /// in hand is not a change and the wait continues until the time is spent.
    /// That is what makes a slice cost a slice.
    fn quiet(&mut self, deadline: Instant) -> crate::watch::Waiting {
        use crate::watch::Waiting;

        match self.change(deadline) {
            Ok(Some(reading)) => Waiting::Arrived(reading),
            Ok(None) => Waiting::Quiet,
            Err(reason) => {
                eprintln!("{}", crate::strings::watch_read_failed(&reason));
                Waiting::Ended
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plainly_core::clipboard::PASSWORD_HINT;

    #[test]
    fn the_type_read_is_the_first_text_one_by_wl_clipboards_rule() {
        // What a real offer looks like: the marker first, then whatever the
        // application decided to publish.
        assert_eq!(
            text_type(&[
                PASSWORD_HINT.to_string(),
                "text/plain;charset=utf-8".to_string(),
            ]),
            Some("text/plain;charset=utf-8")
        );
        assert_eq!(
            text_type(&["UTF8_STRING".to_string(), "STRING".to_string()]),
            Some("UTF8_STRING")
        );
        assert_eq!(
            text_type(&["text/html".to_string(), "text/plain".to_string()]),
            Some("text/html"),
            "the offer's own order decides, as it does for wl-paste"
        );
    }

    #[test]
    fn an_offer_with_no_text_at_all_is_nothing_to_explain() {
        // An image on the clipboard: no panel, and not an error either.
        assert_eq!(text_type(&["image/png".to_string()]), None);
        assert_eq!(text_type(&[]), None);
    }

    #[test]
    fn an_empty_clipboard_is_told_apart_from_a_broken_session() {
        // The sentences are the library's, so this asserts the classification
        // both of them have to land in.
        assert_eq!(
            unreadable("The clipboard of the requested seat is empty"),
            Unreadable::Nothing
        );
        assert_eq!(
            unreadable("No suitable type of content copied"),
            Unreadable::Nothing
        );
        assert!(matches!(
            unreadable("Wayland compositor communication error"),
            Unreadable::Failed(_)
        ));
    }
}
