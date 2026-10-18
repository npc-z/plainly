//! The Plainly main window: history, search, settings and export, as a Tauri
//! window.
//!
//! The window is where the panel sends you (⤢): a resident place to look things
//! up, change your mind about a setting, and take your history somewhere else.
//! It is an ordinary Tauri window, so it takes focus and is not a layer surface.
//!
//! [`app`] holds the decisions and is reachable from a test with no display,
//! [`view`] and [`settings`] are the shapes the frontend draws, and [`commands`]
//! is the translation into Tauri's calling convention. [`strings`] is the
//! window's copy in one module so the interface stays translatable (spec §12).

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
