//! What the desktop session can do, decided from the protocol names the platform
//! observed.

use plainly_core::desktop::{DATA_CONTROL, Desktop, Presentation};

fn desktop(protocols: &[&str]) -> Desktop {
    Desktop::new(protocols.iter().copied())
}

#[test]
fn ext_data_control_means_a_layer_surface_that_never_takes_focus() {
    let desktop = desktop(&["wl_compositor", "ext_data_control_manager_v1"]);

    assert!(desktop.data_control());
    assert_eq!(desktop.presentation(), Presentation::Layer);
}

#[test]
fn the_wlr_spelling_of_data_control_counts_too() {
    let desktop = desktop(&["zwlr_data_control_manager_v1"]);

    assert!(desktop.data_control());
    assert_eq!(desktop.presentation(), Presentation::Layer);
}

/// The GNOME case: no data-control and no layer-shell.
#[test]
fn no_data_control_means_a_window_that_takes_focus() {
    let desktop = desktop(&["wl_compositor", "wl_shm", "xdg_shell"]);

    assert!(!desktop.data_control());
    assert_eq!(desktop.presentation(), Presentation::Window);
}

#[test]
fn a_protocol_that_merely_resembles_one_does_not_count() {
    let desktop = desktop(&[
        "ext_data_control_manager_v2",
        "zwlr_data_control_manager_v1_unstable",
    ]);

    assert!(!desktop.data_control());
    assert_eq!(desktop.presentation(), Presentation::Window);
}

#[test]
fn a_session_with_nothing_to_show_is_the_window() {
    let desktop = desktop(&[]);

    assert!(!desktop.data_control());
    assert_eq!(desktop.presentation(), Presentation::Window);
}

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
