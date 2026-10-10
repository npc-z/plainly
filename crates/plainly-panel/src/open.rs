//! The panel's ⤢: open the main window, on the history screen.
//!
//! The panel does not open a window of its own. It spawns `plainly-desktop`,
//! which takes the session's window lock and either becomes the window or asks the
//! one that already exists to come forward (spec §13, tickets/14). Running the
//! process is therefore the whole handover protocol, and the panel is not a second
//! implementation of it — it is one more caller, like a desktop shortcut.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;

/// The program's name, as `PATH` knows it and as the compositor's own key binding
/// spends it (`spawn "plainly-panel" "--clipboard"`).
const PROGRAM: &str = "plainly-desktop";

/// Open the main window.
///
/// Both sentences this can return are interface copy and live in
/// [`crate::strings`] (`window_not_found` and `window_would_not_start`), because
/// the panel's status line shows them verbatim — this module names what went
/// wrong, the strings module words it.
///
/// A working directory is set to the root **on purpose**: the window and the
/// panel are started from the same shell or key binding, and a spawned child that
/// inherited a deleted directory would fail in a way nobody could read.
pub fn main_window() -> Result<(), String> {
    // Which arm this is decides how the child is spawned, and the two arms are a
    // type rather than a guess: the `PATH` arm is spawned by **name**, so the
    // kernel walks the `PATH` and skips a candidate it cannot run (a half-installed
    // copy, a file that lost its mode) instead of the panel stopping at it — the
    // ownership and group rules `access(2)` applies are not something a check here
    // could reproduce. The beside-the-binary arm is spawned by its path, because
    // that directory is not on the `PATH` the kernel would walk.
    //
    // A named `Found` rather than "does the path look like the program's name": a
    // later change that returned the found file on the `PATH` arm — to name it in
    // the error sentence, say — would silently flip this back to spawning by path
    // and reopen exactly the shadowing case the name-spawn closed.
    let (mut command, subject) = match find()? {
        Found::OnPath => (Command::new(PROGRAM), PROGRAM.to_string()),
        Found::Beside(path) => {
            let name = path.display().to_string();
            (Command::new(path), name)
        }
    };

    let child = command.current_dir("/").spawn().map_err(|error| {
        // `impl Display` rather than `&str` arguments: the name and the system's
        // error are both already displayable, and formatting them here would only
        // add two temporary `String`s.
        crate::strings::window_would_not_start(subject, error)
    })?;

    // Reaped on a thread of its own: the window may stay open for hours, and a
    // child nobody waits for would stay a zombie for as long as the panel does.
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
    });

    Ok(())
}

/// Where `plainly-desktop` was found, in the terms that decide how it is run.
///
/// The two arms are not interchangeable and not a path plus a flag: on the `PATH`
/// the child is spawned by name (the kernel picks the runnable candidate), and
/// beside the binary it is spawned by path (that directory is not on the `PATH`).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Found {
    /// A `plainly-desktop` is on the `PATH` somewhere. *Which* one runs is the
    /// kernel's answer, given when the child is spawned by name.
    OnPath,
    /// The one beside this binary: the development build's layout, and the only
    /// arm that needs a path.
    Beside(PathBuf),
}

/// Whether `plainly-desktop` is within reach, and by which of the two routes.
///
/// `PATH` first, because that is how a session finds it and how the panel itself
/// was found when the compositor spawned it by name. The fallback is the
/// directory this binary is in, which is what a development build has and what a
/// `PATH` that lost its way still keeps together.
///
/// This answers *presence*. Both arms look for a **file**, not for something this
/// process may execute; [`on_path`] says why the second question is the OS's, and
/// why its answer must not be used to choose between candidates.
fn find() -> Result<Found, String> {
    if on_path(std::env::var_os("PATH").as_deref()) {
        return Ok(Found::OnPath);
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(directory) = exe.parent()
    {
        let beside = directory.join(PROGRAM);
        if beside.is_file() {
            return Ok(Found::Beside(beside));
        }
    }

    Err(crate::strings::window_not_found(PROGRAM))
}

/// Whether a `plainly-desktop` **file** is on a `PATH`, given as data.
///
/// The `PATH` is an argument rather than read here, for the reason
/// [`plainly_core::Paths`] takes its environment as data: an environment read is
/// process-wide, and a test that changed it to check this would be changing it
/// under every other test in the binary. [`find`] is the one place that reads the
/// process environment.
///
/// A `bool`, and that is the point: this answers *presence*, which is all that is
/// needed to tell 找不到 from 起不来. Returning the file it found would invite
/// spawning **that** one, and a non-executable `plainly-desktop` earlier on the
/// `PATH` (a half-installed copy, a file that lost its mode) would then shadow the
/// runnable one later, so ⤢ would report 起不来 … Permission denied for a setup
/// where the application menu — same program, same `PATH` — starts it fine.
/// Which candidate runs is `execvp`'s question: it applies the per-caller
/// ownership and group rules of `access(2)`, and no check here could reproduce
/// them.
///
/// "A file" and not "a file this process may execute" for the same reason: a cheap
/// `mode & 0o111` would take another class's execute bit for this process's.
fn on_path(path: Option<&OsStr>) -> bool {
    // No `PATH` at all is "nowhere", and it has to be said here: `split_paths("")`
    // yields one *empty* entry, whose `.join(PROGRAM)` is the relative
    // `plainly-desktop` — a probe of whatever directory the panel happens to be in.
    // That is the same current-directory hazard this module's tests refuse to
    // assert, and `unwrap_or_default()` would put it back into the implementation.
    let Some(path) = path else {
        return false;
    };

    std::env::split_paths(path).any(|directory| directory.join(PROGRAM).is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_is_the_one_the_window_is_called() {
        assert_eq!(PROGRAM, "plainly-desktop");
    }

    /// A `PATH` that cannot hold the program finds nothing.
    ///
    /// The first version of this test asked `find().err()`, which is `None` on
    /// any machine that *does* have `plainly-desktop` installed — so it passed by
    /// finding the real binary and asserted nothing about the lookup. Driving
    /// `on_path` with the `PATH` as data is the assertion it looked like.
    ///
    /// Deliberately no `""` case: `split_paths("")` yields one empty path, and
    /// `PathBuf::from("").join(PROGRAM)` is the *relative* `plainly-desktop` —
    /// `is_file()` on that is a fact about the working directory the tests happen
    /// to run in, not about this function. The positive test below is what keeps
    /// these two from passing vacuously.
    #[test]
    fn a_path_that_cannot_hold_the_program_finds_nothing() {
        assert!(!on_path(Some(OsStr::new("/nonexistent/plainly"))));
        assert!(!on_path(None));
    }

    /// A directory that does hold the program is found through the same lookup, so
    /// the test above is not passing because the lookup never finds anything.
    #[test]
    fn a_directory_that_holds_the_program_finds_it() {
        let directory = std::env::temp_dir().join(format!("plainly-open-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("the temp directory is creatable");
        let program = directory.join(PROGRAM);
        std::fs::write(&program, b"#!/bin/sh\n").expect("the file is writable");

        assert!(on_path(Some(directory.as_os_str())));

        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The failure sentence has to name the program: "找不到" without the name
    /// would leave a person guessing which of two binaries is missing.
    #[test]
    fn a_missing_program_is_named_in_the_reason() {
        let reason = crate::strings::window_not_found(PROGRAM);

        assert!(reason.contains(PROGRAM), "{reason}");
    }
}
