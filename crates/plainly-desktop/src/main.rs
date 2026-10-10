//! The Plainly main window process.
//!
//! `plainly-desktop` is spawned by the panel's ⤢ and by whatever the user puts on
//! their desktop. It takes one argument: the one that makes it a resident
//! listener for copies as well as a window (spec §12, §13).

use plainly_desktop::window::{Mode, Run};

fn main() -> std::process::ExitCode {
    let mut mode = Mode::Window;

    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            // The switch that keeps a process listening for copies. It is
            // accepted so that the configuration the settings screen writes has
            // a consumer to point at, and it is the *only* way the listener is
            // started: the window never starts one behind the switch's back,
            // because a window that did would break "nothing is resident by
            // default" from the other side (spec §12, tickets/14).
            "--watch" => mode = Mode::Watch,
            other => {
                eprintln!("usage: plainly-desktop [--watch] (got {other:?})");
                return std::process::ExitCode::from(2);
            }
        }
    }

    match plainly_desktop::window::run(mode) {
        // A handover is a successful launch: the window the person asked for is
        // in front of them, it is just the one that was already open.
        Ok(Run::Served | Run::Handed) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("plainly-desktop: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
