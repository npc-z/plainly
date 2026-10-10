//! The window's API, as Tauri's calling convention spells it.
//!
//! Every command here is a line or two: find the [`Desktop`], call one method,
//! hand the answer back. The decisions live in [`crate::app`], which is why this
//! module can be read as the list of what the frontend may ask for — and why
//! there is nothing in it that needs a display to test.
//!
//! The state is a `Mutex` because Tauri runs each command on whichever thread of
//! its runtime is free. It is never held across an await (nothing here is
//! async), so the one thing a mutex could cost — a stalled window — cannot
//! happen.

use std::sync::Mutex;

use tauri::State;

use plainly_core::ExportFormat;

use crate::app::{Desktop, HistoryView, SettingsPatch, WindowState};
use crate::error::WindowError;
use crate::settings::SettingsView;
use crate::view::{DetailView, ExportOffer};

/// The window's actions, behind one lock.
///
/// The state is a `Result` because opening it can fail — a `history.db` that is
/// not a SQLite file, a configuration directory that cannot be created — and a
/// window that refused to start because of it would be a window that never said
/// why. It opens instead, and every screen draws the reason until the file
/// behind it is fixed.
pub struct Window(pub Mutex<Result<Desktop, String>>);

impl Window {
    /// The window's state, or the reason there is none.
    ///
    /// One helper rather than a lock in each command that touches the state: the
    /// failure of opening the state has to reach every screen, and a command
    /// that forgot to check would be a button that silently does nothing.
    pub fn desktop(&self) -> Reply<Ready<'_>> {
        let guard = self
            .0
            .lock()
            .map_err(|_| "窗口状态已经不可用了，请重启 plainly-desktop".to_string())?;

        match guard.is_ok() {
            true => Ok(Ready(guard)),
            false => Err(guard
                .as_ref()
                .err()
                .cloned()
                .expect("the state is an error, which is what was just checked")),
        }
    }
}

/// A held [`Desktop`], which the lock's own guard would otherwise hide behind a
/// `Result` the caller has already checked.
pub struct Ready<'a>(std::sync::MutexGuard<'a, Result<Desktop, String>>);

impl std::ops::Deref for Ready<'_> {
    type Target = Desktop;

    fn deref(&self) -> &Desktop {
        self.0
            .as_ref()
            .expect("the guard was checked before it was handed over")
    }
}

/// One command's failure, in the shape the frontend receives.
///
/// Tauri serialises an error as a rejected promise; a plain `String` keeps the
/// frontend from having to know anything about this crate's error type, and the
/// messages are already sentences meant for a person.
pub type Reply<T> = Result<T, String>;

fn failed(error: WindowError) -> String {
    error.to_string()
}

/// The window's copy, which the frontend asks for once so that the interface's
/// words live in one module (spec §12). The settings screen's labels travel with
/// it: a label written into the frontend would be a second place to translate.
#[tauri::command]
pub fn strings() -> StringsReply {
    StringsReply {
        window: crate::strings::strings(),
        settings: crate::strings::settings_strings(),
    }
}

/// Every word the window may say.
#[derive(Debug, serde::Serialize)]
pub struct StringsReply {
    pub window: crate::strings::Strings,
    pub settings: crate::strings::SettingsStrings,
}

/// Everything the window needs to draw itself: which screen to open on, the
/// settings, and the history.
#[tauri::command]
pub fn window(state: State<'_, Window>) -> Reply<WindowState> {
    let desktop = state.desktop()?;
    desktop.state().map_err(failed)
}

/// Narrow the list, then hand back the list. A search with an empty query and
/// no Tag is the whole history, which is what the search box holds when it is
/// empty.
#[tauri::command]
pub fn search(state: State<'_, Window>, query: String, tag: Option<String>) -> Reply<HistoryView> {
    let desktop = state.desktop()?;
    desktop.search(&query, tag.as_deref()).map_err(failed)
}

/// One Record, in full.
#[tauri::command]
pub fn show(state: State<'_, Window>, id: i64) -> Reply<DetailView> {
    let desktop = state.desktop()?;
    desktop.show(id).map_err(failed)
}

/// Add a Tag to a Record.
#[tauri::command]
pub fn tag(state: State<'_, Window>, id: i64, tag: String) -> Reply<HistoryView> {
    let desktop = state.desktop()?;
    desktop.tag(id, &tag).map_err(failed)
}

/// Remove a Tag from a Record.
#[tauri::command]
pub fn untag(state: State<'_, Window>, id: i64, tag: String) -> Reply<HistoryView> {
    let desktop = state.desktop()?;
    desktop.untag(id, &tag).map_err(failed)
}

/// Delete a Record, for good.
#[tauri::command]
pub fn delete(state: State<'_, Window>, id: i64) -> Reply<HistoryView> {
    let desktop = state.desktop()?;
    desktop.delete(id).map_err(failed)
}

/// Delete every Record. The confirmation is the frontend's, because it is the
/// one place the user can still change their mind.
#[tauri::command]
pub fn clear(state: State<'_, Window>) -> Reply<ClearReply> {
    let desktop = state.desktop()?;
    let (removed, history) = desktop.clear().map_err(failed)?;

    Ok(ClearReply { removed, history })
}

/// What a clear did, so the window can say how much it deleted.
#[derive(Debug, serde::Serialize)]
pub struct ClearReply {
    pub removed: usize,
    pub history: HistoryView,
}

/// Write an export. `id` exports one Record; without it, the whole history.
#[tauri::command]
pub fn export(state: State<'_, Window>, format: String, id: Option<i64>) -> Reply<ExportOffer> {
    let format = format.parse::<ExportFormat>().map_err(|message| {
        // The one sentence a command builds itself, and the interface is
        // Chinese: the detail (what the domain expected) follows the colon like
        // every other failure's.
        format!("{}{message}", crate::strings::STRINGS.unknown_export)
    })?;
    let desktop = state.desktop()?;

    desktop.export(format, id).map_err(failed)
}

/// Put one Record's markdown on the clipboard.
#[tauri::command]
pub fn copy_markdown(state: State<'_, Window>, id: i64) -> Reply<()> {
    let desktop = state.desktop()?;
    let markdown = desktop.markdown_of(id).map_err(failed)?;

    desktop.copy(&markdown).map_err(failed)
}

/// The settings screen as it stands.
#[tauri::command]
pub fn settings(state: State<'_, Window>) -> Reply<SettingsView> {
    let desktop = state.desktop()?;
    desktop.settings().map_err(failed)
}

/// Change settings, then hand back the screen they now describe.
#[tauri::command]
pub fn update(state: State<'_, Window>, patch: SettingsPatch) -> Reply<SettingsView> {
    let desktop = state.desktop()?;
    desktop.update(&patch).map_err(failed)
}

/// Put an API key where Plainly can find it, and say where it ended up: a
/// keyring that is not there means the key dies with the window, and that is a
/// sentence the user has to read (spec §8).
#[tauri::command]
pub fn store_key(
    state: State<'_, Window>,
    provider: String,
    secret: String,
) -> Reply<crate::app::KeyOutcome> {
    let desktop = state.desktop()?;

    Ok(desktop.store_key(&provider, &secret))
}

/// Forget a stored key.
#[tauri::command]
pub fn clear_key(state: State<'_, Window>, provider: String) -> Reply<crate::app::KeyOutcome> {
    let desktop = state.desktop()?;

    desktop.forget_key(&provider).map_err(failed)
}

/// Open the directory the history and the exports live in.
///
/// The directory is named in the answer either way: "the desktop would not open
/// it" is an answer a person can act on, and the path is how they act on it. The
/// two cases are told apart in the value rather than by reading the sentence —
/// a success and a failure are not the same event, and the banner colours them
/// differently.
#[tauri::command]
pub fn open_data_dir(state: State<'_, Window>) -> Reply<OpenReply> {
    let desktop = state.desktop()?;
    let path = desktop.data_dir().to_path_buf();

    match crate::open::directory(&path) {
        Ok(()) => Ok(OpenReply {
            opened: true,
            message: path.display().to_string(),
        }),
        Err(reason) => Ok(OpenReply {
            opened: false,
            message: format!("{}（{reason}）", path.display()),
        }),
    }
}

/// What opening the data directory did. The sentence carries the path either
/// way, which is the one thing a person can act on when the desktop would not
/// open it.
#[derive(Debug, serde::Serialize)]
pub struct OpenReply {
    /// Whether the desktop opened it. The frontend colours the banner by this
    /// rather than by reading the sentence.
    pub opened: bool,
    pub message: String,
}

/// The commands the window registers, as one list so that the binary cannot
/// forget one when it builds the application.
#[macro_export]
macro_rules! handlers {
    () => {
        tauri::generate_handler![
            $crate::commands::strings,
            $crate::commands::window,
            $crate::commands::search,
            $crate::commands::show,
            $crate::commands::tag,
            $crate::commands::untag,
            $crate::commands::delete,
            $crate::commands::clear,
            $crate::commands::export,
            $crate::commands::copy_markdown,
            $crate::commands::settings,
            $crate::commands::update,
            $crate::commands::store_key,
            $crate::commands::clear_key,
            $crate::commands::open_data_dir,
        ]
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A window whose state could not be opened at all: what a `history.db`
    /// that is not a SQLite file produces at startup.
    #[test]
    fn a_window_that_could_not_open_its_state_says_why_to_every_screen() {
        let window = Window(Mutex::new(Err("打不开历史库或配置文件：坏的".to_string())));

        let refused = window.desktop();

        assert_eq!(
            refused.err(),
            Some("打不开历史库或配置文件：坏的".to_string()),
            "the reason reaches the caller rather than a panic"
        );
    }
}
