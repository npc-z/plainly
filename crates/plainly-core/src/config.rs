//! The configuration file: a hand-editable TOML document with three sections.
//!
//! Three things matter here, and all three are decisions rather than
//! implementation details:
//!
//! - **The file is the user's.** It is read as a TOML *document*, so comments,
//!   key order and keys this version does not know about survive a `set`.
//!   Unknown keys parse fine (no `deny_unknown_fields`) precisely so a file
//!   written by a newer Plainly does not break an older one.
//! - **Writes are atomic.** A temporary file in the same directory plus a
//!   rename, because the CLI and the desktop app both write this file.
//! - **External edits are detected, not overwritten.** A [`ConfigFile`]
//!   remembers the bytes it read; if the file on disk has changed by the time
//!   `save` runs, the save is refused rather than silently clobbering the
//!   other writer's work.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Table, Value};

/// The configuration document a fresh install gets: every key, with its
/// default, and the comments that explain it.
pub const DEFAULT_CONFIG: &str = include_str!("default_config.toml");

/// The learner's reading level on the A1–C1 scale. It sets the vocabulary and
/// how much syntax the Explanation may keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Level {
    A1,
    A2,
    /// B2 sits in the middle of the scale, and it is the level the prompt
    /// calibration was measured against.
    #[default]
    B2,
    C1,
}

impl Level {
    /// Every level, in ascending order.
    pub const ALL: [Level; 4] = [Level::A1, Level::A2, Level::B2, Level::C1];

    /// The label as it is written in configuration and handed to the model.
    pub fn as_str(self) -> &'static str {
        match self {
            Level::A1 => "A1",
            Level::A2 => "A2",
            Level::B2 => "B2",
            Level::C1 => "C1",
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Level {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Level::ALL
            .into_iter()
            .find(|level| level.as_str() == value)
            .ok_or_else(|| format!("expected one of A1, A2, B2, C1; got {value:?}"))
    }
}

/// The shape of `plainly history export`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    #[default]
    Markdown,
    Anki,
    Raw,
}

impl FromStr for ExportFormat {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "markdown" => Ok(ExportFormat::Markdown),
            "anki" => Ok(ExportFormat::Anki),
            "raw" => Ok(ExportFormat::Raw),
            other => Err(format!("expected markdown, anki or raw; got {other:?}")),
        }
    }
}

/// Which corner of the current output the panel is anchored to. Corners are
/// resolution-independent, which is why free positioning is not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PanelCorner {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl FromStr for PanelCorner {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "top-left" => Ok(PanelCorner::TopLeft),
            "top-right" => Ok(PanelCorner::TopRight),
            "bottom-left" => Ok(PanelCorner::BottomLeft),
            "bottom-right" => Ok(PanelCorner::BottomRight),
            other => Err(format!(
                "expected top-left, top-right, bottom-left or bottom-right; got {other:?}"
            )),
        }
    }
}

/// The user's choice of reasoning mode. `unsupported` is not a setting — it is
/// what a capability probe can report about a provider, and it lives in the
/// probe cache rather than here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Thinking {
    On,
    #[default]
    Off,
}

impl FromStr for Thinking {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "on" => Ok(Thinking::On),
            "off" => Ok(Thinking::Off),
            other => Err(format!("expected on or off; got {other:?}")),
        }
    }
}

/// The `[app]` section: choices the user made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct App {
    pub provider: String,
    pub level: Level,
    pub native_language: String,
    pub export_format: ExportFormat,
    pub ui_language: String,
    pub show_original: bool,
    pub show_comprehensible: bool,
    pub show_glosses: bool,
    pub show_grammar: bool,
    pub show_translation: bool,
    pub panel_corner: PanelCorner,
    pub panel_display: String,
    pub panel_autohide: bool,
    pub panel_autohide_seconds: u32,
    pub copy_auto_popup: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            provider: "deepseek".to_string(),
            level: Level::default(),
            native_language: "Chinese".to_string(),
            export_format: ExportFormat::default(),
            ui_language: "zh-CN".to_string(),
            show_original: true,
            show_comprehensible: true,
            show_glosses: true,
            show_grammar: true,
            show_translation: true,
            panel_corner: PanelCorner::default(),
            panel_display: "current".to_string(),
            panel_autohide: false,
            panel_autohide_seconds: 20,
            copy_auto_popup: false,
        }
    }
}

/// One `[providers.<name>]` table. Anything left out falls back to the shipped
/// preset for that name.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Provider {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub thinking: Thinking,
}

/// The `[prompts]` section: prompt *data* the user owns. The shipped default
/// prompt is not here — it travels with the application so that upstream
/// improvements reach the user — and neither is `prompt_version`, which is
/// derived from the content that is in effect.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Prompts {
    pub appendix: String,
    pub level_descriptors: BTreeMap<String, String>,
}

/// The effective configuration.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub app: App,
    pub providers: BTreeMap<String, Provider>,
    pub prompts: Prompts,
}

impl Config {
    /// Parse a configuration document. Missing keys take their defaults;
    /// unknown keys are ignored so that a newer file still loads.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        toml_edit::de::from_str(text).map_err(|source| ConfigError::Invalid {
            message: source.to_string(),
        })
    }

    /// The effective configuration as TOML, with every default filled in. Used
    /// by `plainly config show`; comments live in the file, not here.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        let invalid = |source: toml_edit::ser::Error| ConfigError::Invalid {
            message: source.to_string(),
        };
        // `to_document` renders structs as inline tables; pretty-printing and
        // re-parsing gives the ordinary `[section]` shape a file should have.
        let text = toml_edit::ser::to_string_pretty(self).map_err(invalid)?;
        let mut doc = text
            .parse::<DocumentMut>()
            .map_err(|source| ConfigError::Invalid {
                message: source.to_string(),
            })?;
        // An empty map can serialise to nothing at all; the three sections are
        // part of the file's shape, so make sure they are present.
        for section in ["app", "providers", "prompts"] {
            let is_table = doc.get(section).map(Item::is_table).unwrap_or(false);
            if !is_table {
                doc[section] = Item::Table(Table::new());
            }
        }
        Ok(doc.to_string())
    }
}

/// Everything that can go wrong while reading or writing the configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not valid TOML: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("{message}")]
    Invalid { message: String },
    #[error("unknown configuration key {key:?}")]
    UnknownKey { key: String },
    #[error("{key}: {message}")]
    Value { key: String, message: String },
    #[error(
        "{path} changed on disk since it was read; refusing to overwrite it. \
         Re-run the command so the change is picked up."
    )]
    ExternallyModified { path: PathBuf },
}

/// A configuration file, held in memory as an editable document.
#[derive(Debug, Clone)]
pub struct ConfigFile {
    path: PathBuf,
    doc: DocumentMut,
    loaded: Option<Vec<u8>>,
}

impl ConfigFile {
    /// Open the file at `path`. A missing file is not an error: it opens as the
    /// shipped default document and is only created when something is saved.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, ConfigError> {
        let path = path.into();
        match fs::read(&path) {
            Ok(bytes) => {
                let text =
                    String::from_utf8(bytes.clone()).map_err(|source| ConfigError::Parse {
                        path: path.clone(),
                        message: format!("file is not UTF-8: {source}"),
                    })?;
                let doc = text
                    .parse::<DocumentMut>()
                    .map_err(|source| ConfigError::Parse {
                        path: path.clone(),
                        message: source.to_string(),
                    })?;
                Ok(Self {
                    path,
                    doc,
                    loaded: Some(bytes),
                })
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                let doc = DEFAULT_CONFIG
                    .parse::<DocumentMut>()
                    .expect("the shipped default configuration is valid TOML");
                Ok(Self {
                    path,
                    doc,
                    loaded: None,
                })
            }
            Err(source) => Err(ConfigError::Io { path, source }),
        }
    }

    /// The file this handle is bound to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the file existed when it was opened.
    pub fn exists(&self) -> bool {
        self.loaded.is_some()
    }

    /// The file's content as it stands (the default document, if the file does
    /// not exist yet).
    pub fn to_toml(&self) -> String {
        self.doc.to_string()
    }

    /// The typed configuration this document describes.
    pub fn config(&self) -> Result<Config, ConfigError> {
        Config::parse(&self.to_toml())
    }

    /// The effective configuration as TOML, defaults filled in.
    pub fn effective_toml(&self) -> Result<String, ConfigError> {
        self.config()?.to_toml()
    }

    /// Set one dotted key, coercing the value according to that key's type.
    ///
    /// The document is edited in a copy and only adopted once the result parses,
    /// so a rejected value leaves the handle — and therefore the file — exactly
    /// as it was.
    pub fn set(&mut self, key: &str, raw: &str) -> Result<(), ConfigError> {
        let spec = KeySpec::parse(key)?;
        let value = spec.coerce(key, raw)?;

        let mut candidate = self.doc.clone();
        set_in_table(candidate.as_table_mut(), &spec.path, value);

        // Reject anything that would not read back.
        let text = candidate.to_string();
        Config::parse(&text).map_err(|error| ConfigError::Value {
            key: key.to_string(),
            message: error.to_string(),
        })?;

        self.doc = candidate;
        Ok(())
    }

    /// Write the document back, atomically, unless someone else got there first.
    pub fn save(&mut self) -> Result<(), ConfigError> {
        match fs::read(&self.path) {
            Ok(current) if Some(&current) != self.loaded.as_ref() => {
                return Err(ConfigError::ExternallyModified {
                    path: self.path.clone(),
                });
            }
            Ok(_) => {}
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                if self.loaded.is_some() {
                    return Err(ConfigError::ExternallyModified {
                        path: self.path.clone(),
                    });
                }
            }
            Err(source) => {
                return Err(ConfigError::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        }

        let bytes = self.to_toml().into_bytes();
        write_atomic(&self.path, &bytes)?;
        self.loaded = Some(bytes);
        Ok(())
    }
}

/// The type a configuration key takes, and where it lives in the document.
struct KeySpec {
    path: Vec<String>,
    kind: Kind,
}

#[derive(Clone, Copy)]
enum Kind {
    Str,
    Bool,
    Seconds,
    Level,
    ExportFormat,
    PanelCorner,
    Thinking,
}

impl KeySpec {
    /// Resolve a dotted key. Only keys Plainly knows about can be set: a typo
    /// should fail loudly rather than write a key nothing ever reads.
    fn parse(key: &str) -> Result<Self, ConfigError> {
        let unknown = || ConfigError::UnknownKey {
            key: key.to_string(),
        };
        let path = |parts: &[&str]| parts.iter().map(|p| p.to_string()).collect::<Vec<_>>();

        let (path, kind) = match key {
            "app.provider" => (path(&["app", "provider"]), Kind::Str),
            "app.level" => (path(&["app", "level"]), Kind::Level),
            "app.native_language" => (path(&["app", "native_language"]), Kind::Str),
            "app.export_format" => (path(&["app", "export_format"]), Kind::ExportFormat),
            "app.ui_language" => (path(&["app", "ui_language"]), Kind::Str),
            "app.show_original" => (path(&["app", "show_original"]), Kind::Bool),
            "app.show_comprehensible" => (path(&["app", "show_comprehensible"]), Kind::Bool),
            "app.show_glosses" => (path(&["app", "show_glosses"]), Kind::Bool),
            "app.show_grammar" => (path(&["app", "show_grammar"]), Kind::Bool),
            "app.show_translation" => (path(&["app", "show_translation"]), Kind::Bool),
            "app.panel_corner" => (path(&["app", "panel_corner"]), Kind::PanelCorner),
            "app.panel_display" => (path(&["app", "panel_display"]), Kind::Str),
            "app.panel_autohide" => (path(&["app", "panel_autohide"]), Kind::Bool),
            "app.panel_autohide_seconds" => {
                (path(&["app", "panel_autohide_seconds"]), Kind::Seconds)
            }
            "app.copy_auto_popup" => (path(&["app", "copy_auto_popup"]), Kind::Bool),
            "prompts.appendix" => (path(&["prompts", "appendix"]), Kind::Str),
            other => {
                if let Some(name) = other.strip_prefix("providers.") {
                    let (name, field) = name.split_once('.').ok_or_else(unknown)?;
                    if name.is_empty() {
                        return Err(unknown());
                    }
                    match field {
                        "endpoint" => (path(&["providers", name, "endpoint"]), Kind::Str),
                        "model" => (path(&["providers", name, "model"]), Kind::Str),
                        "thinking" => (path(&["providers", name, "thinking"]), Kind::Thinking),
                        _ => return Err(unknown()),
                    }
                } else if let Some(level) = other.strip_prefix("prompts.level_descriptors.") {
                    level
                        .parse::<Level>()
                        .map_err(|message| ConfigError::Value {
                            key: key.to_string(),
                            message,
                        })?;
                    (path(&["prompts", "level_descriptors", level]), Kind::Str)
                } else {
                    return Err(unknown());
                }
            }
        };

        Ok(Self { path, kind })
    }

    /// Turn the command-line string into a TOML value of the right type. Values
    /// are coerced, not quoted: `plainly config set app.level A2` works.
    fn coerce(&self, key: &str, raw: &str) -> Result<Value, ConfigError> {
        let invalid = |message: String| ConfigError::Value {
            key: key.to_string(),
            message,
        };

        Ok(match self.kind {
            Kind::Str => Value::from(raw),
            Kind::Bool => Value::from(
                raw.parse::<bool>()
                    .map_err(|_| invalid(format!("expected true or false; got {raw:?}")))?,
            ),
            Kind::Seconds => {
                let seconds = raw.parse::<u32>().map_err(|_| {
                    invalid(format!("expected a whole number of seconds; got {raw:?}"))
                })?;
                if seconds == 0 {
                    return Err(invalid(
                        "a duration of 0 is not how auto-dismiss is turned off: \
                         set app.panel_autohide false instead"
                            .to_string(),
                    ));
                }
                Value::from(seconds as i64)
            }
            Kind::Level => Value::from(raw.parse::<Level>().map_err(invalid)?.as_str()),
            Kind::ExportFormat => {
                Value::from(match raw.parse::<ExportFormat>().map_err(invalid)? {
                    ExportFormat::Markdown => "markdown",
                    ExportFormat::Anki => "anki",
                    ExportFormat::Raw => "raw",
                })
            }
            Kind::PanelCorner => {
                Value::from(match raw.parse::<PanelCorner>().map_err(invalid)? {
                    PanelCorner::TopLeft => "top-left",
                    PanelCorner::TopRight => "top-right",
                    PanelCorner::BottomLeft => "bottom-left",
                    PanelCorner::BottomRight => "bottom-right",
                })
            }
            Kind::Thinking => Value::from(match raw.parse::<Thinking>().map_err(invalid)? {
                Thinking::On => "on",
                Thinking::Off => "off",
            }),
        })
    }
}

/// Set a dotted path inside a document, creating intermediate tables.
fn set_in_table(table: &mut Table, path: &[String], value: Value) {
    let (head, rest) = path
        .split_first()
        .expect("a key path always has at least one segment");

    if rest.is_empty() {
        table[head.as_str()] = Item::Value(value);
        return;
    }

    if table
        .get(head.as_str())
        .map(|item| !item.is_table())
        .unwrap_or(false)
    {
        table[head.as_str()] = Item::Table(Table::new());
    }
    let item = table
        .entry(head.as_str())
        .or_insert(Item::Table(Table::new()));
    let child = item
        .as_table_mut()
        .expect("the entry was just ensured to be a table");
    set_in_table(child, rest, value);
}

/// Write `bytes` to `path` by way of a temporary file in the same directory, so
/// a reader either sees the old content or the new one.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ConfigError> {
    let io_error = |source: std::io::Error| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    };

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(io_error)?;

    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "config.toml".to_string());
    let tmp = dir.join(format!(".{file_name}.tmp-{}", std::process::id()));

    let write = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();

    if let Err(source) = write {
        let _ = fs::remove_file(&tmp);
        return Err(io_error(source));
    }
    Ok(())
}
