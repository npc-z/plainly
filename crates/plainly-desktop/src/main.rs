//! The Plainly main window process.
//!
//! `plainly-desktop` is spawned by the panel's ⤢ and by whatever the user puts on
//! their desktop. It takes one argument: `--watch`, which makes it a resident
//! listener for copies as well as a window (spec §12, §13).

use plainly_desktop::window::{Mode, Run};

fn main() -> std::process::ExitCode {
    let mut mode = Mode::Window;

    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            // The only way the listener starts. The window never starts one on
            // its own, so nothing is resident unless this flag was asked for.
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
