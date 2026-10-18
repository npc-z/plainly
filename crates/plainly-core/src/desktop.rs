//! What the desktop session can do, decided from data the platform observed.
//!
//! This is not the endpoint probe of [`crate::probe`]: that one asks what an
//! endpoint takes and this one asks what the compositor takes. Neither answer is
//! evidence about the other.
//!
//! The answer decides the panel's shape (spec §12). On GNOME, Mutter implements
//! neither data-control nor layer-shell, so the panel can only be a normal window
//! that takes focus, and once it has focus GTK's own clipboard works. The two
//! degradations are one degradation, and [`Desktop::presentation`] is where that
//! is decided.

use std::collections::BTreeSet;

/// The protocols that let a client read the clipboard without taking focus.
///
/// `ext_data_control_manager_v1` is the standardised spelling and
/// `zwlr_data_control_manager_v1` the wlroots one before it. A session publishes
/// one or the other, and which one it publishes decides whether the panel can
/// stay focusless (spec §12).
pub const DATA_CONTROL: [&str; 2] = [
    "ext_data_control_manager_v1",
    "zwlr_data_control_manager_v1",
];

/// The globals a session published, as the platform read them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Desktop {
    protocols: BTreeSet<String>,
}

/// How the panel has to present itself, given what the compositor supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presentation {
    /// A layer surface that never takes focus, reading the clipboard through
    /// data-control.
    Layer,
    /// A normal window that takes focus, reading the clipboard through GTK:
    /// what is left when the compositor has no data-control, which is also when
    /// it has no layer-shell (spec §12).
    Window,
}

impl Desktop {
    /// The session's protocols, as the compositor's registry listed them.
    pub fn new(protocols: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            protocols: protocols.into_iter().map(Into::into).collect(),
        }
    }

    /// Whether the clipboard can be read without taking focus.
    ///
    /// It decides the panel's shape and whether the "explain on copy" switch can
    /// be offered at all (spec §12, §13). A switch that cannot work is disabled
    /// and explained, never hidden.
    pub fn data_control(&self) -> bool {
        DATA_CONTROL
            .iter()
            .any(|name| self.protocols.contains(*name))
    }

    /// The shape the panel must take in this session.
    pub fn presentation(&self) -> Presentation {
        if self.data_control() {
            Presentation::Layer
        } else {
            Presentation::Window
        }
    }
}
