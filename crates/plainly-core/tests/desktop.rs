//! What the desktop session can do, decided from the protocol names the platform
//! observed.
//!
//! The probe has to drive behaviour rather than be logged (spec §12, issues/07):
//! the panel's whole shape — a layer surface that never takes focus, or a normal
//! window that does — follows from whether the compositor speaks data-control.

use plainly_core::desktop::{DATA_CONTROL, Desktop, Presentation};

fn desktop(protocols: &[&str]) -> Desktop {
    Desktop::new(protocols.iter().copied())
}

/// The compositor that lets a client read the clipboard without focus gets the
/// focusless layer surface.
#[test]
fn ext_data_control_means_a_layer_surface_that_never_takes_focus() {
    let desktop = desktop(&["wl_compositor", "ext_data_control_manager_v1"]);

    assert!(desktop.data_control());
    assert_eq!(desktop.presentation(), Presentation::Layer);
}

/// The wlroots spelling is the older one and counts the same — niri and the
/// other wlroots-family compositors publish it alongside (or instead).
#[test]
fn the_wlr_spelling_of_data_control_counts_too() {
    let desktop = desktop(&["zwlr_data_control_manager_v1"]);

    assert!(desktop.data_control());
    assert_eq!(desktop.presentation(), Presentation::Layer);
}

/// No data-control: the panel degrades to a focused window, where GTK's own
/// clipboard works. This is the GNOME case, and it is the decision the probe
/// exists to make (spec §12).
#[test]
fn no_data_control_means_a_window_that_takes_focus() {
    let desktop = desktop(&["wl_compositor", "wl_shm", "xdg_shell"]);

    assert!(!desktop.data_control());
    assert_eq!(desktop.presentation(), Presentation::Window);
}

/// The names are matched, not resembled: a compositor is only credited with the
/// protocol it actually published.
#[test]
fn a_protocol_that_merely_resembles_one_does_not_count() {
    let desktop = desktop(&[
        "ext_data_control_manager_v2",
        "zwlr_data_control_manager_v1_unstable",
    ]);

    assert!(!desktop.data_control());
    assert_eq!(desktop.presentation(), Presentation::Window);
}

/// The probe is a fact about the session, not an accumulation: an empty registry
/// (or one the platform could not read) is the cautious answer.
#[test]
fn a_session_with_nothing_to_show_is_the_window() {
    let desktop = desktop(&[]);

    assert!(!desktop.data_control());
    assert_eq!(desktop.presentation(), Presentation::Window);
}

/// The protocol names are data the platform hands over, so they are published
/// beside the type rather than buried in it.
#[test]
fn the_data_control_protocols_are_the_two_the_desktop_publishes() {
    assert_eq!(
        DATA_CONTROL,
        [
            "ext_data_control_manager_v1",
            "zwlr_data_control_manager_v1"
        ]
    );
}
