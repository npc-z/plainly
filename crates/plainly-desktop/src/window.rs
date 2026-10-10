//! The process: one window, one socket, and the two commands that are not about
//! the window's own state.
//!
//! Startup order matters here and is the whole point of this file:
//!
//! 1. the window's state is opened — the history store and the configuration —
//!    because a failure there is something the window has to *show*, not
//!    something to exit over;
//! 2. the single-instance lock is claimed, and a second launch hands over and
//!    stops here;
//! 3. the window is built and the socket begins to listen.
//!
//! (1) deliberately happens before (2), because between (2) and (3) this process
//! is a window that nobody can reach: a second launch while the store is being
//! opened would wait on a handover that is minutes away on a cold machine.
//! Opening the store first costs a handed-over launch a little work it discards,
//! and buys every handover a socket that is already there.

use std::sync::mpsc;

use tauri::{Manager, WebviewWindow};

use crate::Window;
use crate::app::{Desktop, Environment};
use crate::single::Role;

/// What one launch of `plainly-desktop` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// This process held the window until it closed.
    Served,
    /// Another window was already open, and it has been asked to come forward.
    Handed,
}

/// Build and run the window.
pub fn run(watch: bool) -> Result<Run, String> {
    let (activate, activations) = mpsc::channel::<()>();

    // Opened first, and a failure is carried rather than fatal: a store that
    // will not open is exactly the thing a person needs the window to tell them
    // about, and a process that exits with a line on stderr tells nobody who
    // started it from a desktop launcher. `commands::Window` holds the `Result`
    // and every screen draws it.
    //
    // Before the lock, so that the lock is only held by a process that is ready
    // to answer a handover.
    let desktop = match Desktop::open(Environment::observe()) {
        Ok(desktop) => Ok(desktop),
        Err(error) => Err(crate::strings::cannot_open(&error.to_string())),
    };

    // A second launch hands over from here and pays for the state it will not
    // use — a store read and possibly a keyring round trip, no webview.
    let role = crate::single::claim(activate)?;
    if role == Role::Handed {
        return Ok(Run::Handed);
    }

    let application = tauri::Builder::default()
        .manage(Window(std::sync::Mutex::new(desktop)))
        .invoke_handler(crate::handlers!())
        .setup(move |app| {
            // The window comes forward when this is where the activation is
            // acted on, so that "the panel opened it" and "a second launch asked
            // for it" cannot drift apart.
            let window = app
                .get_webview_window("main")
                .ok_or_else(|| "the window is missing from the configuration".to_string())?;
            forward(activations, window);

            if watch {
                // The flag the watcher will be started with, and the window is
                // still opened: a launch that showed nothing at all would look
                // broken to whoever typed it. The listener itself is the ticket
                // that adds it, and until then the sentence says so rather than
                // letting the process sit there pretending.
                eprintln!("{}", crate::strings::WATCH_NOT_IMPLEMENTED);
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .map_err(|error| crate::strings::cannot_start(&error.to_string()))?;

    application.run(|_, _| {});

    Ok(Run::Served)
}

/// Hand every activation to the window.
///
/// The socket's listener runs on a thread of its own, and Tauri's window
/// operations are safe to call from there: each one is posted to the event loop
/// and performed on the main thread, so this thread is a messenger rather than a
/// second owner of the window.
fn forward(activations: mpsc::Receiver<()>, window: WebviewWindow) {
    std::thread::spawn(move || {
        while activations.recv().is_ok() {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    });
}
