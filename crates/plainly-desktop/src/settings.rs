//! The settings screen, as data: five sections of choices (spec §13).
//!
//! Every value is decided here rather than in the frontend, because the rules
//! are domain rules: which providers a person may choose, whether a key has been
//! found, what each Level means, and whether this session can offer "explain on
//! copy". The frontend draws what it is handed and owns no sentence of its own
//! (spec §12).
//!
//! Three of these placements are constraints rather than preferences:
//!
//! - a Level is shown **with its meaning**, because a bare label measurably
//!   produced identical Explanations for neighbouring levels;
//! - "复制自动弹" is **disabled with a sentence**, never hidden, when the session
//!   has no data-control (spec §12), and the same rule is enforced where the
//!   setting is written;
//! - the prompt appendix is **append-only**, and the screen shows the factory
//!   text it extends so that "you can only add" is something a person can see.

use serde::Serialize;

// The numeric ranges are core's: a write refuses a value outside them wherever
// it comes from, and this screen only draws their ends. Reading stays tolerant,
// so a hand-edited file still opens and is shown as written (spec §4).
use plainly_core::config::{AUTO_HIDE_RANGE, PANEL_HEIGHT_RANGE, PANEL_WIDTH_RANGE};
use plainly_core::presets::{self, KeyRequirement};
use plainly_core::{
    App, Config, Desktop, ExportFormat, KeyPresence, KeySource, Level, PanelCorner, Prompts,
    Readiness, Secrets, local_of,
};

use crate::strings;

/// The whole settings screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettingsView {
    /// The five section titles, in the order the list draws them. They are here
    /// rather than in the frontend because they are the window's copy, which
    /// lives in one place (spec §12).
    pub sections: Vec<&'static str>,
    pub levels: LevelsView,
    pub provider: ProvidersView,
    pub prompt: PromptView,
    pub behavior: BehaviorView,
    pub data: DataView,
}

/// 语言与等级.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LevelsView {
    pub level: String,
    /// Every Level with its meaning — the meaning being what the model is
    /// actually sent, so the screen shows the text in effect rather than a
    /// paraphrase of it.
    pub options: Vec<LevelOption>,
    pub native_language: String,
    /// The languages a person can pick without typing. The field also accepts
    /// any other string: this is the shipped prompt's own translation
    /// instruction.
    pub native_language_options: Vec<String>,
}

/// One Level, with what it means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LevelOption {
    pub value: String,
    pub label: String,
    pub meaning: String,
    /// Whether the meaning is the user's own wording rather than the shipped
    /// one: shown so that a reworded row is visible as reworded.
    pub overridden: bool,
}

/// Which sections the panel shows. They belong to `[app]` and are drawn in the
/// settings screen because the panel itself has no room for a preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SectionsView {
    pub original: bool,
    pub comprehensible: bool,
    pub glosses: bool,
    pub grammar: bool,
    pub translation: bool,
}

/// Provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProvidersView {
    pub configured: String,
    pub options: Vec<ProviderOption>,
    /// What the configured provider resolves to, or why it does not resolve.
    pub current: Option<CurrentProvider>,
    pub current_problem: Option<String>,
    /// Whether the screen should open on the "nothing to talk to yet" state
    /// (spec §13): no provider that both resolves and can be used.
    ///
    /// The same answer as [`ProvidersView::usable`], asked the other way round and
    /// derived from it, so the two cannot disagree.
    pub first_run: bool,
}

/// One provider a person may choose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderOption {
    pub name: String,
    pub label: String,
    /// Whether the endpoint is on this machine (spec §12).
    pub local: bool,
}

/// The provider in effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CurrentProvider {
    pub name: String,
    pub label: String,
    pub endpoint: String,
    pub model: String,
    pub local: bool,
    pub key: KeyView,
    pub thinking: String,
    /// Whether this provider's endpoint understands the canonical thinking
    /// switch. A provider that does not is told so rather than silently ignoring
    /// the setting (spec §7).
    pub thinking_supported: bool,
}

/// Where the key for the configured provider stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KeyView {
    /// `environment`, `keyring`, `session`, `unset` or `unknown`.
    pub source: String,
    /// What the screen says about it.
    pub text: String,
    /// The environment variable that would outrank anything stored here.
    pub variable: String,
    /// Whether a key is still needed. False for an endpoint that may
    /// authenticate nothing, or that already has one.
    ///
    /// Not serialised: the sentence in `text` is what the screen draws, and this
    /// is the crate's own question — "can this provider be used" — asked by
    /// [`ProvidersView::usable`].
    #[serde(skip)]
    pub(crate) needed: bool,
}

/// 提示词.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromptView {
    /// The prompt version in effect, which is also the version stamped on the
    /// next Record.
    pub version: String,
    pub label: String,
    pub appendix: String,
    /// The factory text the appendix is appended to. Shown rather than
    /// described: "you can only add rules, never remove one" is a claim about
    /// this text, and a person who can read it can check the claim.
    pub factory_text: String,
    /// Every Level's meaning, which is prompt data the user may reword.
    pub level_meanings: Vec<LevelOption>,
}

/// 行为.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BehaviorView {
    pub panel_corner: String,
    /// Every corner, as `(value, label)`.
    pub panel_corners: Vec<(String, String)>,
    /// Either `current` — follow the output the pointer is on — or the name of
    /// one output.
    pub panel_display: String,
    pub outputs: Vec<OutputOption>,

    pub panel_width: u32,
    pub panel_width_min: u32,
    pub panel_width_max: u32,
    pub panel_height: u32,
    pub panel_height_min: u32,
    pub panel_height_max: u32,
    pub panel_autohide: bool,
    pub panel_autohide_seconds: u32,
    pub autohide_min: u32,
    pub autohide_max: u32,
    /// Whether the switch reads on: the stored setting and the session's
    /// capability together.
    pub copy_auto_popup: bool,
    pub copy_auto_popup_availability: Availability,
    pub sections: SectionsView,
}

/// A switch that may be unavailable on this session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Availability {
    pub enabled: bool,
    pub note: String,
}

/// One output the panel can be pinned to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OutputOption {
    /// The value stored in `app.panel_display`: `current`, or an output name.
    pub value: String,
    pub label: String,
}

/// 数据.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DataView {
    /// The global exports, each with the value the backend takes and the
    /// label the button shows.
    pub exports: Vec<ExportAction>,
    pub data_dir: String,
    pub config_file: String,
    pub history_file: String,
}

/// One export format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportAction {
    pub value: String,
    pub label: String,
}

/// What a session offers beyond its configuration, passed in as data so that
/// the settings screen is reachable from a test without a compositor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    /// The compositor's protocols, as the registry listed them.
    pub desktop: Desktop,
    /// The outputs the compositor publishes, as `(name, label)`.
    pub outputs: Vec<(String, String)>,
    /// The resolved config, data and cache directories.
    pub config_file: String,
    pub data_file: String,
    pub data_dir: String,
}

/// The label of one export format.
///
/// The format's own spelling comes from [`ExportFormat::as_str`], so a format
/// added to the domain cannot be missing here by accident — the match would not
/// compile.
pub fn export_label(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Markdown => "Markdown",
        ExportFormat::Raw => "原始 CSV",
    }
}

/// The formats a global export offers, in the order they are offered.
///
/// [`ExportFormat::ALL`] is the domain's list; this is the same list as the
/// screen's buttons, so the two cannot disagree about how many there are.
pub fn exports() -> Vec<ExportAction> {
    ExportFormat::ALL
        .into_iter()
        .map(|format| ExportAction {
            value: format.as_str().to_string(),
            label: export_label(format).to_string(),
        })
        .collect()
}

impl SettingsView {
    /// The settings screen for one configuration.
    ///
    /// `prompt` is passed rather than rebuilt from `prompts`, because the version
    /// in effect is what the screen shows and it is derived from the same value
    /// the next run will send.
    pub fn of(
        config: &Config,
        prompt: &plainly_core::Prompt,
        facts: &Facts,
        secrets: &Secrets,
    ) -> Self {
        Self {
            sections: strings::SETTINGS.section_titles.to_vec(),
            levels: levels(&config.app, prompt),
            provider: providers(config, secrets),
            prompt: prompt_view(&config.prompts, prompt),
            behavior: behavior(&config.app, facts),
            data: data(facts),
        }
    }
}

impl ProvidersView {
    /// Whether what is configured can be used right now: it resolves, and it is
    /// not waiting on a key.
    pub fn usable(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|provider| !provider.key.needed)
    }
}

impl KeyView {
    /// The key as core's readiness reports it, in this screen's words.
    ///
    /// Core decides whether a key was found (and from which tier), that there is
    /// none, or that a store could not answer. This only picks the sentence.
    /// `needed` is the same readiness answer the history screen opens on, read
    /// backwards: a provider whose required key is missing or unreadable is the
    /// one thing that blocks a run.
    fn of(readiness: &Readiness) -> Self {
        let variable = plainly_core::env_var_name(&readiness.name);
        let needed = !readiness.usable();

        match &readiness.key {
            KeyPresence::Found(source) => Self {
                source: match source {
                    KeySource::Environment => "environment",
                    KeySource::Keyring => "keyring",
                    KeySource::Session => "session",
                }
                .to_string(),
                text: match source {
                    KeySource::Environment => strings::key_in_environment(&variable),
                    KeySource::Keyring => strings::KEY_IN_KEYRING.to_string(),
                    KeySource::Session => strings::KEY_IN_SESSION.to_string(),
                },
                variable,
                needed,
            },
            KeyPresence::Missing => Self {
                source: "unset".to_string(),
                text: match readiness.requirement {
                    KeyRequirement::Required => strings::KEY_MISSING.to_string(),
                    KeyRequirement::Optional => strings::KEY_OPTIONAL.to_string(),
                },
                variable,
                needed,
            },
            KeyPresence::Unreadable(error) => Self {
                source: "unknown".to_string(),
                text: strings::key_unknown(&error.to_string()),
                variable,
                needed,
            },
        }
    }
}

fn levels(app: &App, prompt: &plainly_core::Prompt) -> LevelsView {
    LevelsView {
        level: app.level.to_string(),
        options: level_options(prompt),
        native_language: app.native_language.clone(),
        native_language_options: vec![
            "Chinese".to_string(),
            "Japanese".to_string(),
            "Korean".to_string(),
            "Spanish".to_string(),
            "French".to_string(),
        ],
    }
}

/// Every Level, with the meaning that will be sent for it.
///
/// One function because the two sections that show levels — 语言与等级 and the
/// prompt's own table — must show the same text: they are the same prompt data,
/// read twice.
fn level_options(prompt: &plainly_core::Prompt) -> Vec<LevelOption> {
    Level::ALL
        .into_iter()
        .map(|level| level_option(level, prompt))
        .collect()
}

/// One Level with the meaning that will be sent for it, and whether that
/// meaning is the user's wording.
///
/// The value shown is what the model is handed, verbatim: a screen that
/// paraphrased the descriptor would be showing a different prompt from the one
/// that runs.
fn level_option(level: Level, prompt: &plainly_core::Prompt) -> LevelOption {
    let meaning = prompt.descriptor(level).to_string();
    LevelOption {
        value: level.to_string(),
        label: level.to_string(),
        overridden: meaning != plainly_core::factory_descriptor(level),
        meaning,
    }
}

fn providers(config: &Config, secrets: &Secrets) -> ProvidersView {
    let configured = config.app.provider.clone();
    let readiness = Readiness::of(&configured, config, secrets);

    let options = presets::PRESETS
        .iter()
        .map(|preset| ProviderOption {
            name: preset.name.to_string(),
            label: preset.label.to_string(),
            local: local_of(preset.name, config),
        })
        .collect();

    // The configured name may be one nobody ships, in which case it is still the
    // one in effect: resolution reports the problem and the screen shows it.
    let (current, current_problem) = match &readiness.resolution {
        Ok(setup) => (
            Some(CurrentProvider {
                name: setup.name.clone(),
                label: setup.label.clone(),
                endpoint: setup.endpoint.clone(),
                model: setup.model.clone(),
                local: readiness.local,
                key: KeyView::of(&readiness),
                thinking: setup.thinking.as_str().to_string(),
                thinking_supported: setup.thinking_switch
                    == plainly_core::presets::ThinkingSwitch::Canonical,
            }),
            None,
        ),
        Err(error) => (None, Some(error.to_string())),
    };

    let view = ProvidersView {
        configured,
        options,
        current,
        current_problem,
        // Filled in below rather than computed twice: `usable` derives it, and
        // `first_run` is its negation by construction.
        first_run: false,
    };

    ProvidersView {
        first_run: !view.usable(),
        ..view
    }
}

fn prompt_view(prompts: &Prompts, prompt: &plainly_core::Prompt) -> PromptView {
    PromptView {
        version: prompt.version(),
        label: prompt.label().to_string(),
        appendix: prompts.appendix.clone(),
        factory_text: plainly_core::FACTORY_PROMPT.to_string(),
        level_meanings: level_options(prompt),
    }
}

fn behavior(app: &App, facts: &Facts) -> BehaviorView {
    let copy_auto = availability(&facts.desktop);

    let mut outputs = vec![OutputOption {
        value: "current".to_string(),
        label: strings::SETTINGS.panel_display_current.to_string(),
    }];
    outputs.extend(facts.outputs.iter().map(|(name, label)| OutputOption {
        value: name.clone(),
        label: label.clone(),
    }));

    BehaviorView {
        panel_corner: app.panel_corner.as_str().to_string(),
        panel_corners: [
            (
                PanelCorner::TopLeft,
                strings::SETTINGS.panel_corner_top_left,
            ),
            (
                PanelCorner::TopRight,
                strings::SETTINGS.panel_corner_top_right,
            ),
            (
                PanelCorner::BottomLeft,
                strings::SETTINGS.panel_corner_bottom_left,
            ),
            (
                PanelCorner::BottomRight,
                strings::SETTINGS.panel_corner_bottom_right,
            ),
        ]
        .into_iter()
        .map(|(corner, label)| (corner.as_str().to_string(), label.to_string()))
        .collect(),
        panel_display: app.panel_display.clone(),
        outputs,
        panel_width: app.panel_width,
        panel_width_min: PANEL_WIDTH_RANGE.0,
        panel_width_max: PANEL_WIDTH_RANGE.1,
        panel_height: app.panel_height,
        panel_height_min: PANEL_HEIGHT_RANGE.0,
        panel_height_max: PANEL_HEIGHT_RANGE.1,
        panel_autohide: app.panel_autohide,
        panel_autohide_seconds: app.panel_autohide_seconds,
        autohide_min: AUTO_HIDE_RANGE.0,
        autohide_max: AUTO_HIDE_RANGE.1,
        // A configuration that says "on" on a session that cannot do it is
        // reported off: the control is drawn greyed, so a checked one would be a
        // state the user could neither reach nor leave. The value in the file is
        // not rewritten, and a session that gains data-control brings it back.
        copy_auto_popup: app.copy_auto_popup && copy_auto.enabled,
        copy_auto_popup_availability: copy_auto,
        sections: SectionsView {
            original: app.show_original,
            comprehensible: app.show_comprehensible,
            glosses: app.show_glosses,
            grammar: app.show_grammar,
            translation: app.show_translation,
        },
    }
}

/// Whether "explain on copy" can work here, and what to say about it.
///
/// The two cases are not "on" and "hidden": the switch is drawn in both, and the
/// unavailable one carries the reason and what would fix it (spec §12).
pub fn availability(desktop: &Desktop) -> Availability {
    match desktop.data_control() {
        true => Availability {
            enabled: true,
            note: strings::COPY_AUTO_AVAILABLE.to_string(),
        },
        false => Availability {
            enabled: false,
            note: strings::COPY_AUTO_UNAVAILABLE.to_string(),
        },
    }
}

fn data(facts: &Facts) -> DataView {
    DataView {
        exports: exports(),
        data_dir: facts.data_dir.clone(),
        config_file: facts.config_file.clone(),
        history_file: facts.data_file.clone(),
    }
}
