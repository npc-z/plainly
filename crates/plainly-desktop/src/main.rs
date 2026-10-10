//! The Plainly main window process.
//!
//! `plainly-desktop` is spawned by the panel's ⤢ and by whatever the user puts on
//! their desktop. It takes no arguments except the one that would make it
//! resident (spec §12, §13).

use plainly_desktop::window::Run;

fn main() -> std::process::ExitCode {
    let mut watch = false;

    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            // The switch that would keep a process listening for copies. It is
            // accepted so that the configuration the settings screen writes has
            // a consumer to point at, and it says out loud that the listener is
            // not built yet rather than pretending otherwise.
            "--watch" => watch = true,
            other => {
                eprintln!("usage: plainly-desktop [--watch] (got {other:?})");
                return std::process::ExitCode::from(2);
            }
        }
    }

    match plainly_desktop::window::run(watch) {
        // A handover is a successful launch: the window the person asked for is
        // in front of them, it is just the one that was already open.
        Ok(Run::Served | Run::Handed) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("plainly-desktop: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
