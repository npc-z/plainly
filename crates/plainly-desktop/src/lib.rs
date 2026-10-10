//! The Plainly main window: history, search, settings and export, as a Tauri
//! window.
//!
//! The window is where the panel sends you (⤢): a resident place to look things
//! up, change your mind about a setting, and take your history somewhere else.
//! It is an ordinary Tauri window — ticket 12's finding was about layer
//! surfaces, which is what the panel is not allowed to be — and the split of
//! work inside it is the one every crate here uses:
//!
//! - [`app`] holds the decisions and is reachable from a test with no display:
//!   which Records a search means, whether the window opens on the history or on
//!   the first-run settings screen, where an export lands;
//! - [`view`] and [`settings`] are the shapes the frontend draws, one value per
//!   screen, so the frontend is a renderer rather than a second implementation;
//! - [`commands`] is the translation into Tauri's calling convention, and
//!   `src/main.rs` is the process around it.
//!
//! [`strings`] is the window's copy, in one module for the same reason the
//! panel's is (spec §12): that is the difference between "translatable later"
//! and "rewrite later".

pub mod app;
pub mod capability;
pub mod clipboard;
pub mod commands;
pub mod copy;
pub mod error;
pub mod open;
pub mod settings;
pub mod single;
pub mod strings;
pub mod view;
pub mod watch;
pub mod window;

pub use app::{
    Clock, Desktop, Environment, HistoryView, Screen, SettingsPatch, SystemClock, WindowState,
};
pub use commands::Window;
pub use error::WindowError;
pub use settings::SettingsView;
pub use view::{DetailView, ExportOffer, RecordView};
