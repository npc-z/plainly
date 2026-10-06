//! The `plainly` command line: a thin shell over `plainly-core`.
//!
//! This binary must not depend on GTK or WebKit. It is the one surface that has
//! to work on a headless machine, and the whole reason the workspace is split
//! the way it is.

mod cli;
mod commands;
mod exit;

use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(commands::dispatch())
}
