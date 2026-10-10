//! Where the panel goes, and which screen it follows.
//!
//! The spec offers four corners and "follow the current output" rather than free
//! coordinates (spec §12), and the reason is that a corner is
//! **resolution-independent**: changing resolution, scale or monitor cannot put
//! the panel outside the screen, because "top-left plus a margin" is re-evaluated
//! by the compositor against whatever the screen is now. That property is worth
//! keeping in the code as well as in the configuration, so this module is a pure
//! function of `(corner, screens)` — the GTK caller only supplies the list of
//! screens and applies the answer, and the interesting cases are unit-testable
//! without a compositor.
//!
//! The other half of the decision lives in the output picker: the settings screen
//! stores `output-1`, `output-2`, … because `xdg-output` names (`eDP-1`,
//! `DP-2`) are a protocol this workspace has no route to. An unknown name
//! therefore means one of two things, and both land in the same place — the
//! current output — which is why [`Display::resolve`] returns a position rather
//! than an error.

use plainly_core::PanelCorner;

/// Which edges a corner is anchored to, and the margin from each.
///
/// A corner anchors two perpendicular edges; the two opposite edges stay free so
/// the surface is placed by the compositor rather than stretched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchors {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
    pub margin: i32,
}

impl Anchors {
    /// The anchors for a corner, at a margin from the screen's edge.
    pub fn of(corner: PanelCorner, margin: i32) -> Self {
        Self {
            left: matches!(corner, PanelCorner::TopLeft | PanelCorner::BottomLeft),
            right: matches!(corner, PanelCorner::TopRight | PanelCorner::BottomRight),
            top: matches!(corner, PanelCorner::TopLeft | PanelCorner::TopRight),
            bottom: matches!(corner, PanelCorner::BottomLeft | PanelCorner::BottomRight),
            margin,
        }
    }
}

/// Which output the panel is placed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Display {
    /// Follow the output the session is working on. The compositor's own choice,
    /// and the default: the panel appears where the person is looking.
    Current,
    /// One named output, as the settings screen spells it: `output-1` is the
    /// first screen the session lists.
    Named(String),
}

impl Display {
    /// The stored spelling, as `app.panel_display` holds it.
    pub fn of(stored: &str) -> Self {
        match stored.trim() {
            "" | "current" => Display::Current,
            other => Display::Named(other.to_string()),
        }
    }

    /// Which screen of `screens` this display means.
    ///
    /// `screens` is the session's screens in its own order; `None` means the
    /// compositor should decide, which is what both "the current output" and "a
    /// name this session does not have" come to. Landing on the current output
    /// for an unknown name is deliberate: a monitor unplugged since the setting
    /// was written must not leave the panel nowhere, and the person's next look
    /// at the settings screen shows the name that could not be found.
    pub fn resolve(&self, screens: &[String]) -> Option<usize> {
        match self {
            Display::Current => None,
            Display::Named(name) => screens.iter().position(|screen| screen == name),
        }
    }
}

/// The name the settings screen stores for the screen at `index`.
///
/// `output-1`, `output-2`, … — the spelling the settings screen writes and the
/// panel reads back, and therefore part of the *file's* contract between two
/// crates that cannot share a constant (spec §13's `app.panel_display`).
///
/// The desktop writes the same string from its own count of `wl_output`s
/// (`plainly-desktop`'s `capability::outputs`), and nothing in one crate can see
/// the other's literal: a drift here shows up as a picker whose choice the panel
/// silently falls back on, which is why the test below pins the spelling rather
/// than only the numbering.
pub fn screen_name(index: usize) -> String {
    format!("output-{}", index + 1)
}

/// Where a **plain window** at a corner goes, in the screen's own coordinates.
///
/// The layer surface gets this from the compositor: anchoring an edge is a
/// request the compositor re-evaluates against the screen it is now, which is
/// what "resolution-independent" means. The GTK fallback — a focused window on a
/// session with no layer shell, such as GNOME — has no anchors, so the panel
/// computes the corner itself from the screen's geometry. The same corner and the
/// same margin, and the same difference from a free position: `right` is
/// measured from the screen's right edge rather than from the origin, so a
/// resolution change cannot leave the panel past it.
pub fn window_origin(
    corner: PanelCorner,
    margin: i32,
    screen: (i32, i32, i32, i32),
    size: (i32, i32),
) -> (i32, i32) {
    let (x, y, width, height) = screen;
    let (window_width, window_height) = size;

    // Clamped to the screen's own origin: a window wider or taller than the
    // workarea would otherwise be measured to a negative offset, which is outside
    // the screen in a way no resolution change caused — it is one panel too big for
    // one monitor, and the origin is the closest point that is still on it.
    let left = (x + margin).max(x);
    let right = (x + width - window_width - margin).max(x);
    let top = (y + margin).max(y);
    let bottom = (y + height - window_height - margin).max(y);

    match corner {
        PanelCorner::TopLeft => (left, top),
        PanelCorner::TopRight => (right, top),
        PanelCorner::BottomLeft => (left, bottom),
        PanelCorner::BottomRight => (right, bottom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_corner_anchors_two_perpendicular_edges() {
        for corner in [
            PanelCorner::TopLeft,
            PanelCorner::TopRight,
            PanelCorner::BottomLeft,
            PanelCorner::BottomRight,
        ] {
            let anchors = Anchors::of(corner, 12);

            assert_ne!(
                anchors.left, anchors.right,
                "{corner:?} anchors exactly one horizontal edge"
            );
            assert_ne!(
                anchors.top, anchors.bottom,
                "{corner:?} anchors exactly one vertical edge"
            );
            assert_eq!(anchors.margin, 12);
        }
    }

    #[test]
    fn the_corners_anchor_the_edges_they_name() {
        assert_eq!(
            Anchors::of(PanelCorner::TopLeft, 12),
            Anchors {
                left: true,
                right: false,
                top: true,
                bottom: false,
                margin: 12
            }
        );
        assert_eq!(
            Anchors::of(PanelCorner::BottomRight, 12),
            Anchors {
                left: false,
                right: true,
                top: false,
                bottom: true,
                margin: 12
            }
        );
    }

    #[test]
    fn the_current_output_is_the_compositors_choice() {
        assert_eq!(Display::of("current"), Display::Current);
        assert_eq!(Display::of(""), Display::Current);
        assert_eq!(Display::of("current").resolve(&["output-1".into()]), None);
    }

    #[test]
    fn a_named_output_is_the_screen_with_that_name() {
        let screens = vec!["output-1".to_string(), "output-2".to_string()];

        assert_eq!(Display::of("output-2").resolve(&screens), Some(1));
        assert_eq!(Display::of("output-1").resolve(&screens), Some(0));
    }

    /// A monitor that is gone — or a connector name another build wrote — is not
    /// a failure the panel can do anything about, and it must not put the panel
    /// somewhere no screen is: it falls back to the compositor's own choice.
    #[test]
    fn an_output_this_session_does_not_have_falls_back_to_the_current_one() {
        let screens = vec!["output-1".to_string()];

        assert_eq!(Display::of("output-9").resolve(&screens), None);
        assert_eq!(Display::of("eDP-1").resolve(&screens), None);
        assert_eq!(Display::of("output-4").resolve(&[]), None);
    }

    #[test]
    fn the_screen_names_match_the_ones_the_settings_screen_offers() {
        assert_eq!(screen_name(0), "output-1");
        assert_eq!(screen_name(2), "output-3");
    }

    /// The fallback window's corner: measured from the screen's own edges, so a
    /// screen at a non-zero origin (a second monitor beside the first) puts the
    /// window on that monitor rather than at the origin of the session.
    #[test]
    fn a_window_at_a_corner_is_measured_from_that_screens_own_edges() {
        let screen = (1920, 0, 2560, 1440);
        let size = (470, 300);

        assert_eq!(
            window_origin(PanelCorner::TopLeft, 12, screen, size),
            (1932, 12)
        );
        assert_eq!(
            window_origin(PanelCorner::TopRight, 12, screen, size),
            (1920 + 2560 - 470 - 12, 12)
        );
        assert_eq!(
            window_origin(PanelCorner::BottomLeft, 12, screen, size),
            (1932, 1440 - 300 - 12)
        );
        assert_eq!(
            window_origin(PanelCorner::BottomRight, 12, screen, size),
            (1920 + 2560 - 470 - 12, 1440 - 300 - 12)
        );
    }

    /// A window larger than the screen — a panel on a small output, or a bottom
    /// corner on a workarea shorter than the stack — is measured to the screen's
    /// origin rather than to a negative offset. Top-left is the control: it is at
    /// the margin on any screen, so it proves the clamp is a floor and not the
    /// only answer.
    #[test]
    fn a_window_larger_than_the_screen_is_pinned_to_its_origin() {
        let size = (470, 600);
        let screen = (0, 0, 300, 200);

        assert_eq!(
            window_origin(PanelCorner::TopLeft, 12, screen, size),
            (12, 12),
            "the margin is still the answer where there is room for it"
        );
        assert_eq!(
            window_origin(PanelCorner::TopRight, 12, screen, size),
            (0, 12),
            "a right corner with no room to its left comes back to the origin"
        );
        assert_eq!(
            window_origin(PanelCorner::BottomLeft, 12, screen, size),
            (12, 0)
        );
        assert_eq!(
            window_origin(PanelCorner::BottomRight, 12, screen, size),
            (0, 0)
        );
    }

    /// The reason the corner beats a free coordinate: the answer is inside the
    /// screen for every size the screen could have.
    #[test]
    fn a_corner_stays_on_the_screen_at_any_resolution() {
        for (width, height) in [(1280, 720), (2560, 1440), (3840, 2160)] {
            let size = (470, 600);
            for corner in [
                PanelCorner::TopLeft,
                PanelCorner::TopRight,
                PanelCorner::BottomLeft,
                PanelCorner::BottomRight,
            ] {
                let (x, y) = window_origin(corner, 12, (0, 0, width, height), size);

                assert!(x >= 0 && x + size.0 <= width, "{corner:?} at {x}");
                assert!(y >= 0 && y + size.1 <= height, "{corner:?} at {y}");
            }
        }
    }
}
