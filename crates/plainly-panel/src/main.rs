//! The Plainly panel: an independent GTK + gtk-layer-shell process.
//!
//! It is a separate binary because Tauri can create a layer surface but cannot
//! draw into one — measured on the host, with a plain GTK C prototype rendering
//! correctly in the same session. See ticket 13 for the minimum loop (compositor
//! keybinding → data-control reads the clipboard → core → five sections) and
//! ticket 15 for the panel's actions and states.

fn main() -> std::process::ExitCode {
    eprintln!(
        "plainly-panel is not implemented yet: the layer-shell panel lands with \
         ticket 13 in .scratch/plainly/tickets/."
    );
    std::process::ExitCode::from(1)
}
