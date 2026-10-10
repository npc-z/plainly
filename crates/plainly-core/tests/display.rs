//! Which display session Plainly is running under, decided from the variables
//! the session publishes.
//!
//! v0 is a Wayland application with one documented degradation (GNOME, which
//! still runs Wayland) and one documented exclusion: X11 (spec §1). The
//! exclusion is explicit rather than left blank, so somebody on an X11 session
//! has to read that sentence rather than a Wayland client library's complaint.
//! Nothing here can be discovered reliably except from the environment, so the
//! environment is an argument and this is a pure function of it.

use std::ffi::OsString;

use plainly_core::display::DisplaySession;

/// A session as a test can hand it over.
fn session(vars: &[(&str, &str)]) -> DisplaySession {
    DisplaySession::from_env(
        vars.iter()
            .map(|(name, value)| (OsString::from(*name), OsString::from(*value))),
    )
}

/// The compositor's own display variable is what makes a session Wayland, and it
/// outranks `DISPLAY` for clients that support both: a Wayland session started
/// from an X11 login commonly has `DISPLAY` set too.
#[test]
fn a_wayland_display_is_a_wayland_session_even_with_display_set() {
    let session = session(&[("WAYLAND_DISPLAY", "wayland-1"), ("DISPLAY", ":0")]);

    assert_eq!(session, DisplaySession::Wayland);
}

/// No `WAYLAND_DISPLAY` and a `DISPLAY` is an X11 session, whatever
/// `XDG_SESSION_TYPE` happens to say: the variable a client connects with is the
/// one that decides.
#[test]
fn a_display_without_a_wayland_display_is_an_x11_session() {
    assert_eq!(session(&[("DISPLAY", ":0")]), DisplaySession::X11);
    assert_eq!(
        session(&[("DISPLAY", ":0"), ("XDG_SESSION_TYPE", "wayland")]),
        DisplaySession::X11,
        "the name the session type carries is not what a client connects with"
    );
}

/// A client with neither variable has no display session to speak of, which is
/// not the same fact as an X11 session: the CLI has to keep working over a pipe
/// on a machine with no display at all.
#[test]
fn neither_variable_is_no_session_rather_than_x11() {
    assert_eq!(session(&[]), DisplaySession::None);
    assert_eq!(session(&[("TERM", "dumb")]), DisplaySession::None);
}

/// An empty value is not a session: a variable set to nothing is how a launcher
/// sometimes clears one, and connecting to "" is not connecting to Wayland.
#[test]
fn an_empty_value_is_not_a_session() {
    assert_eq!(session(&[("WAYLAND_DISPLAY", "")]), DisplaySession::None);
    assert_eq!(
        session(&[("WAYLAND_DISPLAY", ""), ("DISPLAY", "")]),
        DisplaySession::None
    );
    assert_eq!(
        session(&[("WAYLAND_DISPLAY", ""), ("DISPLAY", ":0")]),
        DisplaySession::X11
    );
}

/// The session v0 is built for has nothing to refuse, so a caller asks once and
/// carries on — there is no message to tell apart from an empty one.
#[test]
fn wayland_is_not_refused() {
    assert_eq!(DisplaySession::Wayland.refusal("the panel"), None);
}

/// The panel reports through this sentence, in the words a person reads. It
/// names the session, says v0 is Wayland-only, and points at the one surface
/// X11 does have: the command line, which needs no display (spec §1, §14).
#[test]
fn the_x11_sentence_names_the_session_and_the_way_out() {
    let message = DisplaySession::X11
        .refusal("the panel and its clipboard reader")
        .expect("X11 is refused")
        .to_string();

    assert!(message.contains("X11"), "{message}");
    assert!(message.contains("Wayland"), "{message}");
    assert!(message.contains("plainly"), "{message}");
    assert!(
        message.contains("the panel and its clipboard reader"),
        "the caller says what it could not offer: {message}"
    );
}

/// A session with no display at all is refused too, and in different words: "no
/// Wayland session" is not the X11 exclusion, and the fix is different.
#[test]
fn no_session_is_refused_in_its_own_words() {
    let message = DisplaySession::None
        .refusal("the panel")
        .expect("no session is refused")
        .to_string();

    assert!(message.contains("no Wayland session"), "{message}");
    assert!(!message.contains("X11"), "{message}");
}
