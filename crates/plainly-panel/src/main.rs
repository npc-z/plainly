//! The Plainly panel: an independent GTK + gtk-layer-shell process.
//!
//! It is a separate binary because Tauri can create a layer surface but cannot
//! draw into one — measured on the host, with a plain GTK C prototype rendering
//! correctly in the same session. The compositor's keybinding spawns it, it
//! reads the clipboard itself, and it exits when ✕ is pressed, when its
//! auto-dismiss timer runs out, or when it never manages to draw.
//!
//! The modules are split by the question each one answers, which is also what
//! makes the panel testable at all: `state.rs` is what it is showing, `position.rs`
//! is where it goes, `prefs.rs` is what the configuration says, and `panel.rs` is
//! the widgets that draw the answer. Tickets/15 is where that shape settled.

mod app;
mod capability;
mod clipboard;
mod css;
mod document;
mod open;
mod panel;
mod position;
mod prefs;
mod provenance;
mod provider;
mod state;
mod strings;
#[cfg(test)]
mod support;
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
