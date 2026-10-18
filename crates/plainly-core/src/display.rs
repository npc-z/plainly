//! Which display session Plainly is running under, as data.
//!
//! v0 is a Wayland application: niri and the wlroots family get the whole path,
//! GNOME degrades within Wayland, and X11 is excluded outright (spec §1). The
//! exclusion is a sentence a person has to be able to read, so the session is
//! decided once, here, and both surfaces that would otherwise print a Wayland
//! client library's connection error use it: `plainly explain --clipboard` and
//! `plainly-panel`.
//!
//! The environment is an argument rather than a global read for the same reason
//! every other platform observation in this crate is: the decision has to be
//! reachable from a test, and a test cannot set another test's environment
//! safely. [`DisplaySession::from_process_env`] is the two-line wrapper the
//! binaries call.
//!
//! `DisplaySession` and not `Session`: [`crate::secrets`] already uses `Session`
//! for the tier a secret is held for.
//!
//! Nothing here speaks Wayland. The answer is a fact about the environment, and
//! what to do with it stays with the caller. [`DisplaySession::None`] is a refusal
//! for anything that needs a display, whether a GUI surface with nothing to open
//! or `plainly --clipboard`, whose reader has no session to reach. It is no
//! refusal at all for a Passage from a pipe or a file, which never asks the
//! question and works on a machine with no display whatsoever (spec §14).

use std::ffi::OsString;
use std::fmt;

/// The variable a Wayland client connects with, and the one that decides.
///
/// `XDG_SESSION_TYPE` is deliberately not the decider: it names the login
/// session, while `WAYLAND_DISPLAY` is what a client actually connects to. The
/// two disagree in exactly the case this module exists for, a Wayland session
/// started from an X11 login, and a client that trusts the name over the socket
/// connects nowhere.
const WAYLAND_DISPLAY: &str = "WAYLAND_DISPLAY";

/// The X11 server variable. Its presence with no [`WAYLAND_DISPLAY`] is what
/// makes a session X11.
const DISPLAY: &str = "DISPLAY";

/// What the environment says about the display session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplaySession {
    /// The session publishes a Wayland display: the full v0 path.
    Wayland,
    /// A display is there and it is not Wayland. v0 does not support it (spec
    /// §1). It is the answer to report, not a failure to retry.
    X11,
    /// Neither variable is set: no display session at all. The command line
    /// works here and a GUI surface has nothing to open.
    None,
}

impl DisplaySession {
    /// Classify the environment a caller observed.
    ///
    /// The variables are compared as `OsStr` and never converted to `String`:
    /// `std::env::vars()` panics on a value that is not valid UTF-8, and a stray
    /// byte in somebody's environment is not a reason to bring down the command
    /// (the same rule as [`crate::secrets::EnvSecrets::from_process_env`]).
    pub fn from_env(vars: impl IntoIterator<Item = (OsString, OsString)>) -> Self {
        let mut wayland = false;
        let mut x11 = false;
        for (name, value) in vars {
            // An empty value is how a launcher clears a variable, and
            // connecting to "" is not connecting to anything.
            if value.is_empty() {
                continue;
            }
            if name == WAYLAND_DISPLAY {
                wayland = true;
            } else if name == DISPLAY {
                x11 = true;
            }
        }

        // Wayland first: `DISPLAY` is commonly set alongside it, so the reverse
        // order would make every XWayland-capable session look like X11.
        match (wayland, x11) {
            (true, _) => DisplaySession::Wayland,
            (false, true) => DisplaySession::X11,
            (false, false) => DisplaySession::None,
        }
    }

    /// The process's own environment, classified.
    pub fn from_process_env() -> Self {
        Self::from_env(std::env::vars_os())
    }

    /// The refusal for a surface that cannot work here, saying what it could not
    /// offer, or `None` on the session v0 is built for.
    ///
    /// The check and the sentence are one call on purpose: a surface cannot test
    /// the session without having the sentence to report.
    pub fn refusal(&self, offering: &str) -> Option<SessionRefusal> {
        let message = match self {
            DisplaySession::Wayland => return None,
            DisplaySession::X11 => format!(
                "X11 is not supported in v0: {offering} needs Wayland \
                 (niri, sway, or another wlroots-family compositor). \
                 The command line works on X11 — explain a file or a pipe with `plainly`."
            ),
            DisplaySession::None => format!("no Wayland session: {offering} needs one"),
        };
        Some(SessionRefusal { message })
    }
}

/// A session v0 refuses, with the sentence for it.
///
/// [`DisplaySession::refusal`] is the only way to make one, and it cannot make
/// one for Wayland, so a surface cannot hold "refused" and "this session is
/// fine" at the same time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRefusal {
    message: String,
}

impl fmt::Display for SessionRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
