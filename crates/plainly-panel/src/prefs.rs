//! What one trigger reads out of the configuration, for the panel's own facts.
//!
//! The panel is its own process and has no window to be configured from, so it
//! reads the same file the settings screen writes, once per trigger. That read is
//! what makes everything the screen offers take effect at the next key press rather
//! than at the next login (spec §12), and it is why the panel's facts — the corner,
//! the timer, the section switches, and the provider the note and ⟳ talk about —
//! are gathered here rather than each caller opening the file for the one thing it
//! wants: the position, the timer, the switches and the provider are four questions
//! about one document, and four reads of it are four chances to disagree.
//!
//! **The worker opens the file again for the run itself** (`worker::run`), and that
//! is deliberate rather than an oversight: the run needs the level, the native
//! language and the prompt appendix, none of which the panel has an opinion about,
//! and it reads them where the run happens. The cost is a window — the panel reads
//! at the start of the trigger, the worker when its thread gets there, which on the
//! data-control path is after the clipboard read — so a settings write in between
//! leaves the two disagreeing. What that can and cannot do is bounded: the chip is
//! unaffected, because it comes from the Artifact and so names whoever really
//! answered, but the note under the content could promise 本地模型 for a run that
//! goes to the cloud. ⟳ closes that window when there is a cloud move to make, by
//! carrying the provider name in [`Recall::Bypass`]; when there is none, the
//! `provider: None` arm leaves the worker to resolve the configured provider, and
//! the window is open for that press as it is for an ordinary one.
//!
//! The pair that matters most here is the auto-dismiss one: **an on/off switch
//! and a duration, two independent keys**. A duration of `0` is deliberately not
//! a second spelling of "never" (spec §12) — "off" already has a switch, and two
//! spellings of one state eventually disagree about which one is in effect. A
//! duration outside the range core accepts on write is therefore *refused here
//! too*, by falling back to the default rather than by dismissing the panel
//! instantly or keeping it up for a week: a hand-edited file must not produce a
//! surface that cannot be reached.
//!
//! The provider is here because two things on the panel turn on it and nothing
//! else does: the note under the content, and ⟳'s cloud move. The chip above the
//! body is **not** one of them — it comes from the Artifact, so it names whoever
//! really answered rather than whoever was configured when the panel read the
//! file.

use plainly_core::render::SectionKind;
// The legal domain of these three keys is core's, because the write side (`config
// set`, the settings screen) and this read side have to agree on it. Reading is
// still tolerant — a hand-edited file is clamped or defaulted below — but the
// numbers themselves are not repeated here (spec §4).
use plainly_core::config::{AUTO_HIDE_RANGE, PANEL_HEIGHT_RANGE, PANEL_WIDTH_RANGE};
use plainly_core::{Config, ConfigFile, PanelCorner, Paths, Secrets};

use crate::position::Display;
use crate::provider::ProviderChoice;
use crate::worker::Recall;

/// How long the panel stays when auto-dismiss is on and the file names no usable
/// duration — the settings screen's own default (spec §12).
pub const DEFAULT_AUTOHIDE_SECONDS: u32 = 20;

/// What a trigger reads out of the configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefs {
    pub corner: PanelCorner,
    pub display: Display,
    /// How wide the panel is, in pixels.
    pub panel_width: u32,
    /// The most of the screen the whole panel may take, in pixels.
    ///
    /// A ceiling, not a size: how much of it the content uses is the content's
    /// business, and how much of *that* fits before scrolling is this.
    pub panel_height: u32,
    /// When the panel dismisses itself, in seconds — `None` when it does not.
    ///
    /// One value rather than the switch and the duration travelling separately:
    /// the two keys are independent in the file, which is the point (a `0` is not
    /// a second "off"), and everything downstream of here asks one question.
    pub autohide: Option<u32>,
    /// The provider the next run will talk to, and whether it is on this machine.
    pub provider: ProviderChoice,
    /// The cloud provider ⟳ would move to, when there is one. `None` means the
    /// button regenerates with the same model, and the note under the content says
    /// so in as many words (spec §12).
    pub cloud: Option<ProviderChoice>,
    pub show_original: bool,
    pub show_comprehensible: bool,
    pub show_glosses: bool,
    pub show_grammar: bool,
    pub show_translation: bool,
}

impl Default for Prefs {
    /// The shipped defaults, with no provider able to claim a key.
    ///
    /// A value rather than a read: the two callers are "the panel was built before
    /// anything was read" and the tests, and neither should be opening a file.
    fn default() -> Self {
        Self::of(
            &Config::default(),
            &Secrets::with_stores(plainly_core::EnvSecrets::empty(), None),
        )
    }
}

impl Prefs {
    /// The preferences a configuration holds, with the secrets that decide
    /// whether a cloud provider could answer at all.
    pub fn of(config: &Config, secrets: &Secrets) -> Self {
        let app = &config.app;
        let (provider, cloud) = ProviderChoice::of(config, secrets);

        Self {
            corner: app.panel_corner,
            display: Display::of(&app.panel_display),
            panel_width: size(app.panel_width, PANEL_WIDTH_RANGE),
            panel_height: size(app.panel_height, PANEL_HEIGHT_RANGE),
            autohide: autohide(app.panel_autohide, app.panel_autohide_seconds),
            provider,
            cloud,
            show_original: app.show_original,
            show_comprehensible: app.show_comprehensible,
            show_glosses: app.show_glosses,
            show_grammar: app.show_grammar,
            show_translation: app.show_translation,
        }
    }

    /// The preferences in effect for one trigger, as the file holds them.
    ///
    /// The panel calls this at the start of **every** trigger (`Panel::start`),
    /// which is what makes the settings screen's promises about the panel true: a
    /// corner, a duration, a switch or a provider changed while the panel is up is
    /// in effect on the next key press.
    ///
    /// A configuration that cannot be read is not a reason for no panel: the
    /// defaults are what a first run would get, and the panel's job at a key
    /// press is to show an Explanation, not to report a broken file. The worker
    /// thread says the same thing in its own words when the run actually needs
    /// the file (it cannot proceed without a provider), so nothing is hidden —
    /// it is just not this module's failure to report.
    pub fn read() -> Self {
        Self::from(Paths::discover().ok(), Secrets::from_process_env())
    }

    /// The same, with the path lookup and the secret stores supplied.
    ///
    /// The one reader, so that a test can drive every preference without a home
    /// directory or a keyring — and so that the environment is read exactly once
    /// per trigger, in one place.
    pub fn from(paths: Option<Paths>, secrets: Secrets) -> Self {
        paths
            .and_then(|paths| ConfigFile::open(paths.config_file()).ok())
            .and_then(|file| file.config().ok())
            .map(|config| Self::of(&config, &secrets))
            .unwrap_or_else(|| Self::of(&Config::default(), &secrets))
    }

    /// What a ⟳ on this configuration asks for.
    ///
    /// `None` provider means "the one in the configuration, past the cache". A
    /// named one is the cloud move, and it is offered **only from a model on this
    /// machine**: a hosted provider has a cloud alternative sitting beside it in
    /// [`Prefs::cloud`] (the first shipped preset that is usable), and pressing ⟳
    /// there means "ask this provider again", not "switch me to DeepSeek"
    /// (spec §12).
    ///
    /// It lives on `Prefs` rather than beside the provider facts because it is a
    /// decision *about* those facts, and because the two modules it would
    /// otherwise reach through — the provider table and the trigger state — are
    /// what would make a cycle out of a leaf.
    pub fn regenerate_recall(&self) -> Recall {
        match self.provider.local {
            false => Recall::Bypass { provider: None },
            true => match &self.cloud {
                Some(provider) => Recall::Bypass {
                    provider: Some(provider.name.clone()),
                },
                None => Recall::Bypass { provider: None },
            },
        }
    }

    /// The note the panel shows under the content, when it shows one.
    pub fn local_note(&self) -> Option<&'static str> {
        self.provider
            .local
            .then(|| crate::provider::local_note(self.cloud.as_ref()))
    }

    /// Whether this section is one the person asked to see.
    pub fn shows(&self, kind: SectionKind) -> bool {
        match kind {
            SectionKind::Original => self.show_original,
            SectionKind::ComprehensibleEnglish => self.show_comprehensible,
            SectionKind::KeyHelp => self.show_glosses,
            SectionKind::Grammar => self.show_grammar,
            SectionKind::Translation => self.show_translation,
        }
    }
}

/// The auto-dismiss duration, from the file's switch and its duration.
///
/// A duration outside the range is not accepted and not fatal: the switch is the
/// authority on *whether* the panel dismisses, and the default is the answer for
/// *how long* when the file's number is one no surface should honour.
fn autohide(enabled: bool, seconds: u32) -> Option<u32> {
    if !enabled {
        return None;
    }

    Some(
        match (AUTO_HIDE_RANGE.0..=AUTO_HIDE_RANGE.1).contains(&seconds) {
            true => seconds,
            false => DEFAULT_AUTOHIDE_SECONDS,
        },
    )
}

/// A size read from the file, held inside the range core accepts on write.
///
/// Clamped to the nearest bound rather than replaced by the default, which is what
/// [`autohide`] does with a duration and deliberately not what happens here: a
/// panel larger or smaller than the range is still a panel somebody can read and
/// close, so the closest thing to what the file asked for is the friendly answer,
/// where a duration outside its range is one no surface should honour at all.
fn size(pixels: u32, range: (u32, u32)) -> u32 {
    pixels.clamp(range.0, range.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plainly_core::{App, Config, EnvSecrets};

    fn config(app: App) -> Config {
        Config {
            app,
            ..Config::default()
        }
    }

    /// No secrets at all: the provider facts are about the file, and this module's
    /// tests are about the file.
    fn none() -> Secrets {
        Secrets::with_stores(EnvSecrets::empty(), None)
    }

    #[test]
    fn a_fresh_configuration_is_the_off_switch_the_spec_starts_from() {
        let prefs = Prefs::of(&config(App::default()), &none());

        assert_eq!(prefs.autohide, None, "auto-dismiss is off by default");
        assert_eq!(prefs.corner, PanelCorner::TopLeft);
        assert_eq!(prefs.display, Display::Current);
        assert!(prefs.shows(SectionKind::Original));
        assert!(prefs.shows(SectionKind::Translation));
    }

    #[test]
    fn the_panel_gets_the_size_the_file_asks_for() {
        let prefs = Prefs::of(
            &config(App {
                panel_width: 620,
                panel_height: 540,
                ..App::default()
            }),
            &none(),
        );

        assert_eq!(prefs.panel_width, 620);
        assert_eq!(prefs.panel_height, 540);
    }

    /// A hand-edited size is clamped to the range core accepts on write, not
    /// replaced by the default: a panel of the wrong size is still a panel
    /// somebody can read and close, so the nearest bound is the friendly answer.
    #[test]
    fn a_size_outside_the_range_is_clamped_to_the_nearest_bound() {
        let squeezed = Prefs::of(
            &config(App {
                panel_width: 10,
                panel_height: 1,
                ..App::default()
            }),
            &none(),
        );
        assert_eq!(squeezed.panel_width, PANEL_WIDTH_RANGE.0);
        assert_eq!(squeezed.panel_height, PANEL_HEIGHT_RANGE.0);

        let enormous = Prefs::of(
            &config(App {
                panel_width: 9_999,
                panel_height: 9_999,
                ..App::default()
            }),
            &none(),
        );
        assert_eq!(enormous.panel_width, PANEL_WIDTH_RANGE.1);
        assert_eq!(enormous.panel_height, PANEL_HEIGHT_RANGE.1);
    }

    #[test]
    fn the_switch_and_the_duration_are_two_keys() {
        let off = Prefs::of(
            &config(App {
                panel_autohide: false,
                panel_autohide_seconds: 20,
                ..App::default()
            }),
            &none(),
        );
        assert_eq!(
            off.autohide, None,
            "a duration without the switch is not a dismissal"
        );

        let on = Prefs::of(
            &config(App {
                panel_autohide: true,
                panel_autohide_seconds: 45,
                ..App::default()
            }),
            &none(),
        );
        assert_eq!(on.autohide, Some(45));
    }

    /// `0` is the second spelling of "off" the spec refuses: a hand-edited file
    /// that carries one gets the default duration, not an instantly vanishing
    /// panel and not a panel that never goes away.
    #[test]
    fn a_duration_outside_the_range_falls_back_to_the_default() {
        for seconds in [0, 1, 4, 301, u32::MAX] {
            let prefs = Prefs::of(
                &config(App {
                    panel_autohide: true,
                    panel_autohide_seconds: seconds,
                    ..App::default()
                }),
                &none(),
            );

            assert_eq!(
                prefs.autohide,
                Some(DEFAULT_AUTOHIDE_SECONDS),
                "{seconds} should not be honoured"
            );
        }
    }

    /// The note under the content is the button's instructions, so it has to
    /// follow the same fact ⟳ acts on: with a cloud provider and a key for it,
    /// "换成云端"; with neither, the same local model.
    #[test]
    fn the_local_note_follows_whether_a_cloud_move_exists() {
        let local = crate::support::local_config();

        let without = Prefs::of(&local, &none());
        assert_eq!(
            without.local_note(),
            Some(crate::strings::LOCAL_NOTE_NO_CLOUD)
        );

        let with = Prefs::of(
            &local,
            &Secrets::with_stores(
                EnvSecrets::from_pairs([(plainly_core::env_var_name("deepseek"), "k")]),
                None,
            ),
        );
        assert!(
            with.cloud.is_some(),
            "a hosted provider with a key is usable"
        );
        assert_eq!(with.local_note(), Some(crate::strings::LOCAL_NOTE));
    }

    /// A hosted provider gets no note at all: the warning is about a model on
    /// this machine, and one shown for every provider would stop meaning anything.
    #[test]
    fn a_cloud_provider_gets_no_note() {
        let prefs = Prefs::of(&config(App::default()), &none());

        assert!(!prefs.provider.local);
        assert_eq!(prefs.local_note(), None);
    }

    /// ⟳ on a local model with a usable cloud provider names that provider.
    #[test]
    fn the_regenerate_recall_moves_a_local_model_to_the_cloud() {
        let local = crate::support::local_config();
        let prefs = Prefs::of(
            &local,
            &Secrets::with_stores(
                EnvSecrets::from_pairs([(plainly_core::env_var_name("deepseek"), "k")]),
                None,
            ),
        );

        assert_eq!(
            prefs.regenerate_recall(),
            Recall::Bypass {
                provider: Some("deepseek".to_string())
            }
        );
    }

    /// ⟳ on a **hosted** provider is that provider again, even though a cloud
    /// alternative is in the preferences: the move exists for local models, and
    /// re-running DeepSeek under a chip that says OpenAI would be the panel
    /// choosing a vendor.
    #[test]
    fn the_regenerate_recall_leaves_a_hosted_provider_alone() {
        let prefs = Prefs::of(
            &Config::default(),
            &Secrets::with_stores(
                EnvSecrets::from_pairs([(plainly_core::env_var_name("deepseek"), "k")]),
                None,
            ),
        );

        assert!(!prefs.provider.local);
        assert!(prefs.cloud.is_some(), "a cloud alternative is in reach");
        assert_eq!(prefs.regenerate_recall(), Recall::Bypass { provider: None });
    }

    /// ⟳ on a local model with nothing to move to re-runs the same local model.
    #[test]
    fn the_regenerate_recall_without_a_cloud_provider_stays_local() {
        let prefs = Prefs::of(&crate::support::local_config(), &none());

        assert_eq!(prefs.cloud, None);
        assert_eq!(prefs.regenerate_recall(), Recall::Bypass { provider: None });
    }

    #[test]
    fn the_section_switches_are_read_one_by_one() {
        let prefs = Prefs::of(
            &config(App {
                show_grammar: false,
                show_glosses: false,
                ..App::default()
            }),
            &none(),
        );

        assert!(!prefs.shows(SectionKind::Grammar));
        assert!(!prefs.shows(SectionKind::KeyHelp));
        assert!(prefs.shows(SectionKind::ComprehensibleEnglish));
    }

    #[test]
    fn a_corner_and_an_output_from_the_file_are_read_back() {
        let prefs = Prefs::of(
            &config(App {
                panel_corner: PanelCorner::BottomRight,
                panel_display: "output-2".to_string(),
                ..App::default()
            }),
            &none(),
        );

        assert_eq!(prefs.corner, PanelCorner::BottomRight);
        assert_eq!(prefs.display, Display::Named("output-2".to_string()));
    }
}
