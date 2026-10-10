//! Handing a path to the desktop.
//!
//! The window's one action that leaves the process without a conversation: it
//! opens the directory the history and the exports live in. `xdg-open` is in the
//! development shell and on any desktop, and a compositor's session is the only
//! place this window runs.

use std::path::Path;
use std::process::Command;

/// Ask the desktop to open a directory.
///
/// Reported rather than ignored: opening the data directory is an answer to "I
/// want my files", and a user who is told nothing would conclude the button is
/// broken. The caller names the path either way, so a session with no
/// `xdg-open` still learns where the files are.
pub fn directory(path: &Path) -> Result<(), String> {
    let command = match std::env::consts::OS {
        "macos" => "open",
        "windows" => "explorer",
        _ => "xdg-open",
    };

    // Not waited on by the caller: a file manager stays open for as long as the
    // person wants it, and a window that blocked on it would freeze. It is
    // reaped on a thread of its own all the same, because this window is
    // long-lived and a child nobody reaps stays a zombie for as long as the
    // window does.
    let child = Command::new(command)
        .arg(path)
        .spawn()
        .map_err(|error| crate::strings::open_failed(&format!("{command}: {error}")))?;

    reap(child);

    Ok(())
}

/// Wait for a child nobody else waits for, on a thread that outlives this call.
///
/// The child is moved in whichever way the spawn went — there is a process to
/// reap or there is not — so this is the single place that owns the handle
/// rather than one branch that remembers to.
fn reap(child: std::process::Child) {
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });
}
