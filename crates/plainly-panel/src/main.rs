//! The Plainly panel: an independent GTK + gtk-layer-shell process.
//!
//! It is a separate binary because Tauri can create a layer surface but cannot
//! draw into one — measured on the host, with a plain GTK C prototype rendering
//! correctly in the same session. The compositor's keybinding spawns it, it
//! reads the clipboard itself, and it exits when ✕ is pressed or when it never
//! manages to draw. Ticket 15 is the rest of the panel: its actions, its
//! position, and its complete set of states.

mod app;
mod capability;
mod clipboard;
mod panel;
mod plan;
mod strings;
mod watchdog;
mod worker;

use gtk::glib;

/// The one way the panel is started: the compositor's keybinding asks for the
/// clipboard (spec §12). There is no other input, so anything else is a mistake
/// in the asking rather than a mode to guess at.
const CLIPBOARD: &str = "--clipboard";

fn main() -> glib::ExitCode {
    let mut arguments = std::env::args().skip(1);

    if arguments.next().as_deref() != Some(CLIPBOARD) || arguments.next().is_some() {
        eprintln!("usage: plainly-panel {CLIPBOARD}");
        return glib::ExitCode::from(2);
    }

    app::run()
}
