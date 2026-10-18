//! Which display session Plainly is running under, decided from the variables
//! the session publishes.

use std::ffi::OsString;

use plainly_core::display::DisplaySession;

fn session(vars: &[(&str, &str)]) -> DisplaySession {
    DisplaySession::from_env(
        vars.iter()
            .map(|(name, value)| (OsString::from(*name), OsString::from(*value))),
    )
}

#[test]
fn a_wayland_display_is_a_wayland_session_even_with_display_set() {
    let session = session(&[("WAYLAND_DISPLAY", "wayland-1"), ("DISPLAY", ":0")]);

    assert_eq!(session, DisplaySession::Wayland);
}

#[test]
fn a_display_without_a_wayland_display_is_an_x11_session() {
    assert_eq!(session(&[("DISPLAY", ":0")]), DisplaySession::X11);
    assert_eq!(
        session(&[("DISPLAY", ":0"), ("XDG_SESSION_TYPE", "wayland")]),
        DisplaySession::X11,
        "the name the session type carries is not what a client connects with"
    );
}

#[test]
fn neither_variable_is_no_session_rather_than_x11() {
    assert_eq!(session(&[]), DisplaySession::None);
    assert_eq!(session(&[("TERM", "dumb")]), DisplaySession::None);
}

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

#[test]
fn wayland_is_not_refused() {
    assert_eq!(DisplaySession::Wayland.refusal("the panel"), None);
}

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

#[test]
fn no_session_is_refused_in_its_own_words() {
    let message = DisplaySession::None
        .refusal("the panel")
        .expect("no session is refused")
        .to_string();

    assert!(message.contains("no Wayland session"), "{message}");
    assert!(!message.contains("X11"), "{message}");
}
