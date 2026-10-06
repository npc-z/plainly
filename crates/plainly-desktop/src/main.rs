//! The Plainly main window: a Tauri window holding history, search, settings
//! and export.
//!
//! The render failure recorded in ticket 12 applies to layer surfaces only; a
//! plain Tauri window is unaffected. Ticket 14 builds this.

fn main() -> std::process::ExitCode {
    eprintln!(
        "plainly-desktop is not implemented yet: the main window lands with \
         ticket 14 in .scratch/plainly/tickets/."
    );
    std::process::ExitCode::from(1)
}
