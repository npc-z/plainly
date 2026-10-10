//! The main window's own state: what a screen shows, and what an action does.
//!
//! One value, made once at startup and asked for everything — the same shape as
//! everywhere else in this workspace. It owns the three things a window action
//! can touch:
//!
//! - the **history store**, opened on `appDataDir` and long-lived, because the
//!   window is long-lived: a search must not pay for a second connection, and a
//!   Tag or a deletion must be visible to the next search without reconnecting;
//! - the **configuration file** as an editable document, so that a setting the
//!   user never touched keeps its comments and its place in the file, and so
//!   that a file somebody else changed is refused rather than clobbered
//!   ([`plainly_core::ConfigFile`]);
//! - **secrets**, because whether the first run has a provider at all depends on
//!   whether a key can be found (spec §13).
//!
//! Everything a surface shows is decided here and handed over as data
//! ([`crate::view`], [`crate::settings`]). `src/commands.rs` is then a
//! translation layer and nothing else.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use plainly_core::{
    Cleared, Config, ConfigFile, Desktop as Session, ExportFormat, Level, PanelCorner, Paths,
    Prompt, Secrets, Stored, Thinking, Timestamp,
};

use crate::error::WindowError;
use crate::settings::{Facts, SettingsView};
use crate::view::{self, DetailView, ExportOffer, RecordView};

/// The clock, as a value. The store orders by time and an export is named after
/// the moment it was made, so a test needs to be able to say what time it is.
pub trait Clock: Send + Sync {
    /// The current instant, to the second. The store orders by it and an export
    /// is named after it, so a test that means "today" has to be able to say so.
    fn now(&self) -> Timestamp;
}

/// The system clock. A clock before 1970 or after the year 9999 is refused by
/// [`Timestamp`], and a window has no better answer than the earliest instant
/// this build can name — so the fallback is a plain one and is not worth a
/// dialog.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs() as i64)
            .unwrap_or(0);

        Timestamp::from_unix_seconds(seconds).unwrap_or_else(|| {
            Timestamp::from_unix_seconds(0).expect("the epoch is inside the range")
        })
    }
}

/// What storing or clearing a key did, as the surface words it.
///
/// The two store outcomes are different sentences because they are different
/// facts: a key in the keyring survives the window, and a key in this session's
/// memory does not — and the second is not really a success at all, which is why
/// the outcome carries that distinction as a value rather than leaving the
/// surface to read the sentence. The sentences themselves live in
/// [`crate::strings`], where the rest of the interface's copy is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyOutcome {
    pub message: String,
    /// Whether the key is still there when the window closes. False means the
    /// banner wears its warning colour: the key works now and will not later.
    pub durable: bool,
}

impl KeyOutcome {
    /// The sentence for a store that succeeded — or only half did.
    pub fn of(stored: Stored) -> Self {
        Self {
            message: crate::strings::key_outcome(&stored),
            durable: matches!(stored, Stored::Keyring),
        }
    }

    /// The sentence for a removal, which is durable either way: a key that is
    /// gone is gone.
    fn cleared(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            durable: true,
        }
    }
}

/// What the surface asks the machine, as data, so that the settings screen is
/// reachable from a test without a compositor or a session bus.
pub struct Environment {
    /// The compositor's protocols, as its registry listed them.
    pub desktop: Session,
    /// The outputs the compositor publishes, as `(name, description)`.
    pub outputs: Vec<(String, String)>,
}

impl Environment {
    /// What this machine says about itself.
    pub fn observe() -> Self {
        Self {
            desktop: crate::capability::desktop(),
            outputs: crate::capability::outputs(),
        }
    }
}

/// Which screen the window is on. The window has no other navigation state: the
/// list is resident, so a history screen is the list plus a detail, and the
/// settings screen reuses the same list column for its five sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Screen {
    History,
    Settings,
}

/// The whole window, as one value.
#[derive(Debug, Serialize)]
pub struct WindowState {
    pub screen: Screen,
    /// Which settings section to open on: the provider one when there is
    /// nothing to talk to yet, because that section is where the first run's
    /// state and its two ways out live (spec §13).
    pub settings_section: usize,
    pub now: i64,
    pub settings: SettingsView,
    pub history: HistoryView,
}

/// The history screen: the list, what narrows it, and the detail.
#[derive(Debug, Serialize)]
pub struct HistoryView {
    pub records: Vec<RecordView>,
    /// Every Tag in the history, in order, for the filter chips. The tags a
    /// person may filter by are the ones they have actually used — there is no
    /// vocabulary of Plainly's own (spec §9).
    pub tags: Vec<String>,
    pub query: String,
    pub tag: Option<String>,
    /// Whether anything at all is in the history, whatever the search says.
    /// Told apart from `records` being empty so that "no matches" and "nothing
    /// yet" are not the same screen.
    pub has_records: bool,
}

/// One setting to change, as the frontend hands it over.
///
/// Every field is optional because the screen sends what it changed. The values
/// are typed rather than strings so that a typo in the frontend cannot write a
/// string where a number belongs; `ConfigFile::set` still validates every one of
/// them on the way in.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsPatch {
    pub level: Option<Level>,
    pub native_language: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<Thinking>,
    pub appendix: Option<String>,
    pub panel_corner: Option<PanelCorner>,
    pub panel_display: Option<String>,
    pub panel_autohide: Option<bool>,
    pub panel_autohide_seconds: Option<u32>,
    pub copy_auto_popup: Option<bool>,
    pub export_format: Option<ExportFormat>,
    pub show_original: Option<bool>,
    pub show_comprehensible: Option<bool>,
    pub show_glosses: Option<bool>,
    pub show_grammar: Option<bool>,
    pub show_translation: Option<bool>,
}

/// The main window's state and actions.
pub struct Desktop {
    paths: Paths,
    config: Mutex<ConfigFile>,
    store: Mutex<plainly_core::Store>,
    secrets: Secrets,
    environment: Environment,
    clock: Box<dyn Clock>,
    /// The history screen's own state: what the search box holds and which Tag
    /// is narrowing the list. It belongs to the window rather than to the
    /// frontend for the same reason the query is sent back with every state:
    /// the list and its filter are one thing.
    query: Mutex<(String, Option<String>)>,
}

impl Desktop {
    /// Open the window's state on the real paths, with the real clock and the
    /// real secret stores.
    pub fn open(environment: Environment) -> Result<Self, WindowError> {
        Self::with_clock(environment, Box::new(SystemClock))
    }

    /// The same, with the clock supplied.
    pub fn with_clock(
        environment: Environment,
        clock: Box<dyn Clock>,
    ) -> Result<Self, WindowError> {
        Self::open_at(Paths::discover()?, environment, clock)
    }

    /// The same, with every input supplied.
    ///
    /// This is the constructor the tests drive, and it exists for the reason
    /// [`plainly_core::Paths`] takes its environment as data: a test can point
    /// the window at a temporary directory instead of the developer's home. It
    /// is public because the alternative — a `#[cfg(test)]` door — would not be
    /// reachable from an integration test, which is where this crate's tests
    /// live.
    pub fn open_at(
        paths: Paths,
        environment: Environment,
        clock: Box<dyn Clock>,
    ) -> Result<Self, WindowError> {
        let config = ConfigFile::open(paths.config_file())?;
        let store = plainly_core::Store::open(paths.history_file())?;

        Ok(Self {
            config: Mutex::new(config),
            store: Mutex::new(store),
            secrets: Secrets::from_process_env(),
            environment,
            clock,
            query: Mutex::new((String::new(), None)),
            paths,
        })
    }

    /// The same, with the secret stores supplied as well: what a test needs to
    /// keep the developer's keyring out of it.
    pub fn with_secrets(mut self, secrets: Secrets) -> Self {
        self.secrets = secrets;
        self
    }

    /// The whole window: the screen it should open on, the settings, and the
    /// history.
    ///
    /// The screen reuses the first run's own rule (spec §13): with nothing to
    /// talk to, the window opens on the settings screen's empty state rather
    /// than on a history list that could never grow.
    pub fn state(&self) -> Result<WindowState, WindowError> {
        let settings = self.settings()?;
        let history = self.history()?;
        let usable = settings.provider.usable();

        Ok(WindowState {
            screen: match usable {
                true => Screen::History,
                false => Screen::Settings,
            },
            settings_section: usher(usable),
            now: self.clock.now().unix_seconds(),
            settings,
            history,
        })
    }

    /// The settings screen.
    pub fn settings(&self) -> Result<SettingsView, WindowError> {
        let config = self.config()?;
        let prompt = Prompt::from_config(&config.prompts);

        Ok(SettingsView::of(
            &config,
            &prompt,
            &Facts {
                desktop: self.environment.desktop.clone(),
                outputs: self.environment.outputs.clone(),
                config_file: self.paths.config_file().display().to_string(),
                data_file: self.paths.history_file().display().to_string(),
                data_dir: self.paths.data_dir().display().to_string(),
            },
            &self.secrets,
        ))
    }

    /// The history screen as it stands: the search box's contents, the Tag
    /// filter, and the rows they leave.
    pub fn history(&self) -> Result<HistoryView, WindowError> {
        let (query, tag) = self.query()?;
        let config = self.config()?;
        let store = self.store.lock().expect("the store lock is not poisoned");
        let all = store.list()?;
        let records = match (query.trim().is_empty(), tag.as_deref()) {
            (true, None) => all.clone(),
            _ => store.search(&query, tag.as_deref())?,
        };

        Ok(HistoryView {
            records: records
                .iter()
                .map(|record| RecordView::of(record, &config))
                .collect(),
            tags: tags_of(&all),
            query,
            tag,
            has_records: !all.is_empty(),
        })
    }

    /// Narrow the list. The search covers the Passage, the Comprehensible
    /// English and the Glosses and nothing else — the Translation is not
    /// indexed, which is the sentence the search box carries under it
    /// (spec §9, §13).
    pub fn search(&self, query: &str, tag: Option<&str>) -> Result<HistoryView, WindowError> {
        {
            let mut state = self.query.lock().expect("the query lock is not poisoned");
            *state = (query.to_string(), tag.map(str::to_string));
        }
        self.history()
    }

    /// One Record, in full.
    pub fn show(&self, id: i64) -> Result<DetailView, WindowError> {
        let config = self.config()?;
        let store = self.store.lock().expect("the store lock is not poisoned");
        let record = store
            .show(id)?
            .ok_or(plainly_core::StoreError::NoRecord { id })?;

        Ok(DetailView::of(&record, &config))
    }

    /// Tag a Record with the learner's own word for it, and hand back the
    /// history the tag leaves.
    pub fn tag(&self, id: i64, tag: &str) -> Result<HistoryView, WindowError> {
        {
            let mut store = self.store.lock().expect("the store lock is not poisoned");
            store.tag(id, tag)?;
        }
        self.history()
    }

    /// Take a Tag off a Record again.
    pub fn untag(&self, id: i64, tag: &str) -> Result<HistoryView, WindowError> {
        {
            let mut store = self.store.lock().expect("the store lock is not poisoned");
            store.untag(id, tag)?;
        }
        self.history()
    }

    /// Remove one Record, and say which one so the window can close the detail
    /// that was showing it.
    pub fn delete(&self, id: i64) -> Result<HistoryView, WindowError> {
        {
            let mut store = self.store.lock().expect("the store lock is not poisoned");
            store.delete(id)?;
        }
        self.history()
    }

    /// Remove every Record. Deliberately not reachable without a confirmation in
    /// the interface: there is no undo (spec §9).
    pub fn clear(&self) -> Result<(usize, HistoryView), WindowError> {
        let removed = {
            let mut store = self.store.lock().expect("the store lock is not poisoned");
            store.clear()?
        };
        Ok((removed, self.history()?))
    }

    /// Write one export and say where it landed.
    ///
    /// The file goes to the **data directory**, beside the history it is a copy
    /// of, with the moment in its name: there is no file dialog in v0, and a
    /// path the window can always name back is worth more than a chooser that
    /// would be the only thing standing between the user and their own history.
    pub fn export(
        &self,
        format: ExportFormat,
        only: Option<i64>,
    ) -> Result<ExportOffer, WindowError> {
        let records = {
            let store = self.store.lock().expect("the store lock is not poisoned");
            match only {
                Some(id) => vec![
                    store
                        .show(id)?
                        .ok_or(plainly_core::StoreError::NoRecord { id })?,
                ],
                None => store.list()?,
            }
        };

        let text = view::export_text(format, &records);
        let path = write_export(
            self.paths.data_dir(),
            format,
            only,
            self.clock.now(),
            text.as_bytes(),
        )?;

        Ok(ExportOffer {
            path: path.display().to_string(),
            summary: crate::strings::export_summary(only, records.len()),
        })
    }

    /// The markdown of one Record, as the detail's "复制 markdown" puts it on
    /// the clipboard.
    pub fn markdown_of(&self, id: i64) -> Result<String, WindowError> {
        Ok(self.show(id)?.markdown)
    }

    /// Put text on the clipboard. Fails loudly: a "copied" that copied nothing
    /// is worse than an error.
    pub fn copy(&self, text: &str) -> Result<(), WindowError> {
        crate::copy::copy(text)
    }

    /// Change one or more settings, then hand back the whole settings screen.
    ///
    /// Every write goes through `ConfigFile`, so a value that does not read back
    /// is refused before anything is written, and a file changed by another
    /// process is refused rather than overwritten.
    pub fn update(&self, patch: &SettingsPatch) -> Result<SettingsView, WindowError> {
        // Refused rather than written and ignored: a configuration that says
        // "on" while the session cannot do it is the same lie the greyed switch
        // exists to avoid (spec §12). The frontend disables the control; this is
        // what makes that a rule rather than a habit.
        if patch.copy_auto_popup == Some(true) && !self.environment.desktop.data_control() {
            return Err(WindowError::Value {
                message: crate::strings::update_failed(
                    &crate::settings::availability(&self.environment.desktop).note,
                ),
            });
        }

        {
            let mut config = self.config.lock().expect("the config lock is not poisoned");
            let current = config.config()?;

            for (key, value) in patch.keys(&current)? {
                config.set(&key, &value)?;
            }

            if let Err(error) = config.save() {
                // The document in hand holds edits that are not in the file —
                // somebody else's write got there first, which is the one thing
                // this handle refuses to overwrite. It is dropped and re-read
                // rather than kept: a handle that remembers bytes the file does
                // not hold can never save again, and every later screen would be
                // showing settings that were never written.
                if let Ok(fresh) = ConfigFile::open(self.paths.config_file()) {
                    *config = fresh;
                }
                return Err(match error {
                    // Core words this one for a command line ("re-run the
                    // command"), and the window is not one. The fact is the same
                    // and the sentence is the interface's.
                    plainly_core::ConfigError::ExternallyModified { .. } => WindowError::Value {
                        message: crate::strings::settings_changed_on_disk().to_string(),
                    },
                    other => other.into(),
                });
            }
        }

        self.settings()
    }

    /// The configuration as it stands.
    pub fn config(&self) -> Result<Config, WindowError> {
        let config = self.config.lock().expect("the config lock is not poisoned");
        Ok(config.config()?)
    }

    /// Put an API key where Plainly can find it, and say where it went. A
    /// keyring that is not there is a sentence rather than a failure: the key
    /// still works for as long as the window is open, and the sentence says so.
    pub fn store_key(&self, provider: &str, secret: &str) -> KeyOutcome {
        KeyOutcome::of(self.secrets.store(provider, secret))
    }

    /// Take a stored key away again.
    pub fn forget_key(&self, provider: &str) -> Result<KeyOutcome, WindowError> {
        match self.secrets.clear(provider) {
            Ok(Cleared::Keyring) => Ok(KeyOutcome::cleared(crate::strings::KEY_CLEARED)),
            Ok(Cleared::Nothing) => Ok(KeyOutcome::cleared(crate::strings::KEY_CLEARED_NOTHING)),
            Err(error) => Err(WindowError::Value {
                message: error.to_string(),
            }),
        }
    }

    /// Where the durable records live, so the window can offer to open them.
    pub fn data_dir(&self) -> &Path {
        self.paths.data_dir()
    }

    /// Where the configuration file is.
    ///
    /// The listener reads it again on every copy (that is how the switch is
    /// obeyed), and it is opened before the window is built, so the window hands
    /// the path over rather than letting a second discovery of the same
    /// environment produce a second opinion about where the file is.
    pub fn config_file(&self) -> PathBuf {
        self.paths.config_file()
    }

    fn query(&self) -> Result<(String, Option<String>), WindowError> {
        let query = self.query.lock().expect("the query lock is not poisoned");
        Ok(query.clone())
    }
}

impl SettingsPatch {
    /// The `(key, value)` pairs this patch means, as the configuration document
    /// spells them.
    ///
    /// Written out one setting at a time rather than derived from the struct, so
    /// that a field added to [`SettingsPatch`] cannot be silently accepted and
    /// dropped: the compile error in this function is the reminder.
    fn keys(&self, current: &Config) -> Result<Vec<(String, String)>, WindowError> {
        let mut pairs: Vec<(String, String)> = Vec::new();
        let mut put = |key: &str, value: String| pairs.push((key.to_string(), value));

        // The provider the patch is *about*: the one it names, or the one in
        // effect. A patch that changes the provider and its model together has
        // to write the model under the new name, not the old one.
        let provider = || {
            self.provider
                .clone()
                .unwrap_or_else(|| current.app.provider.clone())
        };

        if let Some(level) = self.level {
            put("app.level", level.as_str().to_string());
        }
        if let Some(language) = &self.native_language {
            if language.trim().is_empty() {
                return Err(WindowError::Value {
                    message: crate::strings::native_language_required().to_string(),
                });
            }
            put("app.native_language", language.clone());
        }
        if let Some(provider) = &self.provider {
            put("app.provider", provider.clone());
        }
        if let Some(model) = &self.model {
            let name = provider();
            if name.trim().is_empty() {
                return Err(WindowError::Value {
                    message: crate::strings::KEY_NO_PROVIDER.to_string(),
                });
            }
            put(&model_key(&name), model.clone());
        }
        if let Some(thinking) = self.thinking {
            put(&thinking_key(&provider()), thinking.as_str().to_string());
        }
        if let Some(appendix) = &self.appendix {
            put("prompts.appendix", appendix.clone());
        }
        if let Some(corner) = self.panel_corner {
            put("app.panel_corner", corner.as_str().to_string());
        }
        if let Some(display) = &self.panel_display {
            put("app.panel_display", display.clone());
        }
        if let Some(autohide) = self.panel_autohide {
            put("app.panel_autohide", autohide.to_string());
        }
        if let Some(seconds) = self.panel_autohide_seconds {
            if !(crate::settings::AUTO_HIDE_RANGE.0..=crate::settings::AUTO_HIDE_RANGE.1)
                .contains(&seconds)
            {
                return Err(WindowError::Value {
                    message: crate::strings::auto_hide_out_of_range(
                        crate::settings::AUTO_HIDE_RANGE.0,
                        crate::settings::AUTO_HIDE_RANGE.1,
                    ),
                });
            }
            put("app.panel_autohide_seconds", seconds.to_string());
        }
        if let Some(copy) = self.copy_auto_popup {
            put("app.copy_auto_popup", copy.to_string());
        }
        if let Some(format) = self.export_format {
            put("app.export_format", format.as_str().to_string());
        }
        for (key, value) in [
            ("app.show_original", self.show_original),
            ("app.show_comprehensible", self.show_comprehensible),
            ("app.show_glosses", self.show_glosses),
            ("app.show_grammar", self.show_grammar),
            ("app.show_translation", self.show_translation),
        ] {
            if let Some(value) = value {
                put(key, value.to_string());
            }
        }

        Ok(pairs)
    }
}

/// Which settings section a freshly opened window shows.
///
/// The only choice is the first run's: with nothing to talk to, the window opens
/// on the Provider section rather than on the language one, because that is
/// where the empty state and the two ways out of it are (spec §13).
fn usher(usable: bool) -> usize {
    match usable {
        true => 0,
        false => 1,
    }
}

/// Write an export under the first name this directory does not already hold.
///
/// The moment in the name is only accurate to the second and the window does not
/// stop existing between two clicks, so the counter is what makes two exports in
/// one second two files. The name is **created**, not checked and then written:
/// `create_new` fails if the file appeared in between, which is the difference
/// between a counter and a race — two windows, or a file left by an earlier run,
/// can otherwise resolve to the same name and the second export silently
/// replaces the first, the one outcome the name exists to prevent.
fn write_export(
    directory: &Path,
    format: ExportFormat,
    only: Option<i64>,
    now: Timestamp,
    bytes: &[u8],
) -> Result<PathBuf, WindowError> {
    std::fs::create_dir_all(directory).map_err(|source| WindowError::Write {
        path: directory.to_path_buf(),
        source,
    })?;

    // A bound rather than an open range: a directory with ten thousand exports
    // of one history is not a state to keep probing, and saying so beats a loop
    // that never ends.
    for attempt in 0..10_000u32 {
        let candidate = directory.join(export_name(format, only, now, attempt));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                if let Err(source) = file.write_all(bytes) {
                    // A name that looks like a finished export is worse than no
                    // file at all: the next person to open the directory cannot
                    // tell a truncated one from a complete one.
                    drop(file);
                    let _ = std::fs::remove_file(&candidate);
                    return Err(WindowError::Write {
                        path: candidate,
                        source,
                    });
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(WindowError::Write {
                    path: candidate,
                    source,
                });
            }
        }
    }

    Err(WindowError::Value {
        message: crate::strings::export_names_exhausted(&directory.display().to_string()),
    })
}

/// The dotted key of one provider's model.
fn model_key(provider: &str) -> String {
    format!("providers.{provider}.model")
}

/// The dotted key of one provider's thinking switch.
fn thinking_key(provider: &str) -> String {
    format!("providers.{provider}.thinking")
}

/// Every Tag the history carries, without repeats.
///
/// Sorted rather than in the order the Records were read, because the order the
/// chip row is drawn in must not change every time a search reorders the list:
/// the chips are the learner's own vocabulary, and a stable alphabetical row is
/// the one a person can learn the position of.
fn tags_of(records: &[plainly_core::Record]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for record in records {
        for tag in &record.tags {
            if !tags.contains(tag) {
                tags.push(tag.clone());
            }
        }
    }
    tags.sort();
    tags
}

/// The name an export would be written under, before the directory is asked.
///
/// The moment is in the name and so is a counter, because the moment is only
/// accurate to the second and the window does not stop existing between two
/// clicks: a second export of the same history in the same second would
/// otherwise land on the first one's path, and an export that silently replaces
/// another is the one outcome this name exists to prevent.
fn export_name(format: ExportFormat, only: Option<i64>, now: Timestamp, attempt: u32) -> String {
    let stamp = view::utc_text(now).replace(' ', "-").replace(':', "");
    let extension = match format {
        ExportFormat::Markdown => "md",
        ExportFormat::Anki | ExportFormat::Raw => "csv",
    };
    let subject = match only {
        Some(id) => format!("record-{id}"),
        None => "history".to_string(),
    };

    let stem = match format {
        ExportFormat::Anki => format!("plainly-anki-{subject}-{stamp}"),
        ExportFormat::Raw => format!("plainly-raw-{subject}-{stamp}"),
        ExportFormat::Markdown => format!("plainly-{subject}-{stamp}"),
    };

    match attempt {
        0 => format!("{stem}.{extension}"),
        n => format!("{stem}-{n}.{extension}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_with_nothing_to_talk_to_opens_on_the_provider_section() {
        assert_eq!(usher(false), 1, "the first run lands on Provider");
        assert_eq!(
            usher(true),
            0,
            "an ordinary launch lands on the first section"
        );
    }

    #[test]
    fn an_export_name_carries_the_moment_and_which_export_it_is() {
        let now = Timestamp::from_unix_seconds(1_760_000_000).expect("in range");
        assert_eq!(
            export_name(ExportFormat::Markdown, None, now, 0),
            "plainly-history-2025-10-09-085320.md"
        );
        assert_eq!(
            export_name(ExportFormat::Anki, Some(7), now, 0),
            "plainly-anki-record-7-2025-10-09-085320.csv"
        );
        assert_eq!(
            export_name(ExportFormat::Markdown, None, now, 1),
            "plainly-history-2025-10-09-085320-1.md",
            "a second export in the same second is a second file"
        );
    }

    #[test]
    fn a_second_export_in_the_same_second_does_not_replace_the_first() {
        let directory =
            std::env::temp_dir().join(format!("plainly-desktop-name-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("the temp directory is creatable");

        let now = Timestamp::from_unix_seconds(1_760_000_000).expect("in range");
        let first = write_export(&directory, ExportFormat::Markdown, None, now, b"one")
            .expect("the first export is written");
        let second = write_export(&directory, ExportFormat::Markdown, None, now, b"two")
            .expect("the second export is written");

        assert_ne!(first, second, "the name in use is not handed out again");
        assert_eq!(
            second.file_name().and_then(|name| name.to_str()),
            Some("plainly-history-2025-10-09-085320-1.md")
        );
        assert_eq!(
            std::fs::read(&first).expect("the first export is readable"),
            b"one",
            "the first export kept its content"
        );
        assert_eq!(
            std::fs::read(&second).expect("the second export is readable"),
            b"two"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }
}
