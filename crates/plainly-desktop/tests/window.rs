//! The main window's own behaviour, driven through the API the frontend uses.
//!
//! The seam is [`plainly_desktop::Desktop`]: the value a Tauri command holds and
//! calls one method on. Testing there rather than through the webview is what
//! makes the window's decisions reachable without a display, a compositor or a
//! session bus — and it is the right seam for a different reason too: what the
//! frontend sees *is* these return values, so a test that drives them is testing
//! the contract the interface is written against.
//!
//! What is deliberately not tested here: that a webview draws, that Tauri wires
//! the commands, and that the socket handover works across two real processes.
//! Those live in the ticket's manual checklist, because a test that opened a real
//! window would need a real session and would still not be looking at it.

mod support;

use plainly_core::{ExportFormat, Level, PanelCorner, Prompt, Secrets, Thinking, Timestamp};
use plainly_desktop::WindowError;
use plainly_desktop::app::{Clock, Desktop, Screen, SettingsPatch};
use plainly_desktop::settings::availability;

use support::{TempDir, artifact, desktop_with_wayland};

/// A clock that does not move, so that "today" is the same today in every run.
struct FixedClock(i64);

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_unix_seconds(self.0).expect("the fixture instant is in range")
    }
}

/// The instant the fixtures are stamped with: 2025-10-09 08:53:20 UTC.
const NOW: i64 = 1_760_000_000;

/// A window over a temporary home, with the store pre-filled with `records` and
/// a key in the environment — so the window is on the history screen rather than
/// the first-run state unless a test says otherwise.
fn window(dir: &TempDir, records: &[(i64, &str)]) -> Desktop {
    window_with_env(dir, records, &[("deepseek", "sk-test")])
}

/// The same, with the environment's keys spelled out: no key is the first-run
/// state of spec §13.
///
/// The pairs are `(provider, key)`, and the variable name is derived the way the
/// application derives it, so a test cannot pass by spelling one wrong.
fn window_with_env(dir: &TempDir, records: &[(i64, &str)], keys: &[(&str, &str)]) -> Desktop {
    let paths = dir.paths();
    let mut store = plainly_core::Store::open(paths.history_file()).expect("the store opens");
    for (seconds, passage) in records {
        let artifact = artifact(*seconds, passage);
        let lookup = plainly_core::Lookup {
            passage: &artifact.passage,
            level: artifact.level,
            source_language: &artifact.source_language,
            native_language: &artifact.native_language,
            prompt_version: &artifact.prompt_version,
            provider: &artifact.provider,
            model: &artifact.model,
            thinking: artifact.thinking,
        };
        store
            .remember(&lookup.key(), &artifact)
            .expect("the fixture record is written");
    }
    drop(store);

    Desktop::open_at(paths, desktop_with_wayland(), Box::new(FixedClock(NOW)))
        .expect("the window opens")
        .with_secrets(Secrets::with_stores(
            plainly_core::EnvSecrets::from_pairs(
                keys.iter()
                    .map(|(provider, key)| (plainly_core::env_var_name(provider), *key)),
            ),
            None,
        ))
}

#[test]
fn the_history_lists_records_newest_first_with_everything_a_row_shows() {
    let dir = TempDir::new("window-list");
    let desktop = window(
        &dir,
        &[
            (NOW - 60, "The report stops short of apportioning blame."),
            (NOW - 86_400, "He bottled it at the last minute."),
        ],
    );

    let history = desktop.history().expect("the history reads");

    assert_eq!(history.records.len(), 2);
    let newest = &history.records[0];
    assert_eq!(
        newest.snippet,
        "The report stops short of apportioning blame."
    );
    // A row that did not carry the prompt version would read as a duplicate of
    // the same Passage under another prompt (ticket 09, spec §13).
    assert_eq!(newest.prompt_label, "v7-descriptors");
    assert_eq!(newest.prompt_short.len(), 8);
    assert!(newest.prompt_version.starts_with(&newest.prompt_short));
    assert_eq!(newest.provider, "deepseek");
    assert_eq!(newest.model, "deepseek-flash");
    assert_eq!(newest.level, "B2");
    assert!(!newest.local, "deepseek is a hosted endpoint");
    assert_eq!(newest.last_seen, NOW - 60);
    assert!(history.has_records);
}

#[test]
fn the_same_passage_under_two_prompt_versions_is_two_rows() {
    let dir = TempDir::new("window-versions");
    // Same Passage, two Records: what a prompt change produces, and what the
    // row's version is there to make visible rather than look like a duplicate.
    let paths = dir.paths();
    let mut store = plainly_core::Store::open(paths.history_file()).expect("the store opens");
    for (version, label) in [("aaaa1111", "v6-synthesis"), ("bbbb2222", "v7-descriptors")] {
        let mut artifact = artifact(NOW, "The lessee shall indemnify the lessor.");
        artifact.prompt_version = version.to_string();
        artifact.prompt_label = label.to_string();
        let lookup = plainly_core::Lookup {
            passage: &artifact.passage,
            level: artifact.level,
            source_language: &artifact.source_language,
            native_language: &artifact.native_language,
            prompt_version: &artifact.prompt_version,
            provider: &artifact.provider,
            model: &artifact.model,
            thinking: artifact.thinking,
        };
        store
            .remember(&lookup.key(), &artifact)
            .expect("the fixture record is written");
    }
    drop(store);

    let desktop = Desktop::open_at(paths, desktop_with_wayland(), Box::new(FixedClock(NOW)))
        .expect("the window opens");

    let history = desktop.history().expect("the history reads");
    assert_eq!(history.records.len(), 2, "two prompt versions are two rows");
    let versions: Vec<&str> = history
        .records
        .iter()
        .map(|row| row.prompt_short.as_str())
        .collect();
    assert!(versions.contains(&"aaaa1111"));
    assert!(versions.contains(&"bbbb2222"));
}

#[test]
fn a_search_covers_the_english_fields_and_a_tag_narrows_it() {
    let dir = TempDir::new("window-search");
    let desktop = window(
        &dir,
        &[
            (NOW - 60, "The report stops short of apportioning blame."),
            (NOW - 120, "He bottled it at the last minute."),
        ],
    );
    let bottled = desktop
        .history()
        .expect("the history reads")
        .records
        .into_iter()
        .find(|row| row.snippet.starts_with("He bottled"))
        .expect("the second fixture is there")
        .id;

    desktop.tag(bottled, "idiom").expect("the tag is written");

    let found = desktop.search("bottled", None).expect("the search runs");
    assert_eq!(found.records.len(), 1);
    assert_eq!(found.records[0].id, bottled);

    // A Tag is a filter, not a search term: the chip narrows whatever the query
    // already found, which is what the list's chips do.
    let filtered = desktop
        .search("", Some("idiom"))
        .expect("the tag filter runs");
    assert_eq!(filtered.records.len(), 1);
    assert_eq!(filtered.records[0].id, bottled);
    assert_eq!(filtered.tags, vec!["idiom".to_string()]);

    // The Chinese translation is deliberately not indexed (spec §9), and the
    // window states that under the search box rather than letting it look like a
    // broken search.
    let chinese = desktop
        .search("经理", None)
        .expect("a Chinese search is not an error");
    assert!(chinese.records.is_empty());
}

#[test]
fn the_detail_carries_the_five_sections_and_the_markdown_export_of_one_record() {
    let dir = TempDir::new("window-detail");
    let desktop = window(
        &dir,
        &[(NOW, "The report stops short of apportioning blame.")],
    );
    let id = desktop.history().expect("the history reads").records[0].id;

    let detail = desktop.show(id).expect("the record is there");

    assert_eq!(
        detail.passage,
        "The report stops short of apportioning blame."
    );
    assert!(!detail.comprehensible.is_empty());
    assert!(!detail.glosses.is_empty());
    assert_eq!(
        detail.translation,
        "委员会对此事进行了彻底调查，但那位经理已经躲了起来。"
    );
    assert_eq!(detail.level, "B2");
    assert_eq!(detail.thinking, "off");
    // The detail is the one place the whole hash can be read (ticket 14) — and
    // the abbreviated form is the backend's, not the frontend's slice.
    assert_eq!(detail.prompt_version.len(), 64);
    assert_eq!(detail.prompt_short.len(), 8);
    assert!(detail.prompt_version.starts_with(&detail.prompt_short));
    assert_eq!(detail.last_seen_unix, NOW);
    assert_eq!(
        detail.markdown,
        plainly_core::render::markdown(
            &detail.passage,
            &plainly_core::Explanation {
                comprehensible: detail.comprehensible.clone(),
                glosses: detail
                    .glosses
                    .iter()
                    .map(|gloss| plainly_core::Gloss {
                        expression: gloss.expression.clone(),
                        gloss: gloss.gloss.clone(),
                    })
                    .collect(),
                grammar: detail.grammar.clone(),
                translation: detail.translation.clone(),
            }
        ),
        "the detail's markdown is the one renderer's, not a second rendering"
    );
    // The five sections, in the renderer's own markdown, with the Original
    // first: what "复制 markdown" puts on the clipboard.
    assert!(detail.markdown.contains("### Original"));
    assert!(detail.markdown.contains("### Comprehensible English"));
    assert!(detail.markdown.contains("### Key Help"));
    assert!(detail.markdown.contains("### Translation"));
    assert!(
        !detail.markdown.contains("### Grammar"),
        "a Grammar note the model did not give is absent from a document, not labelled"
    );
}

#[test]
fn an_export_writes_one_file_and_says_where_it_landed() {
    let dir = TempDir::new("window-export");
    let desktop = window(
        &dir,
        &[
            (NOW, "The report stops short of apportioning blame."),
            (NOW - 60, "He bottled it at the last minute."),
        ],
    );
    let id = desktop.history().expect("the history reads").records[0].id;

    let whole = desktop
        .export(ExportFormat::Markdown, None)
        .expect("the history exports");
    let single = desktop
        .export(ExportFormat::Markdown, Some(id))
        .expect("one record exports");
    let raw = desktop
        .export(ExportFormat::Raw, None)
        .expect("the raw export runs");

    // Two exports of one history are two files, and the name says which is
    // which: an export that overwrote the other would be a silent loss.
    assert_ne!(whole.path, single.path);
    assert_ne!(whole.path, raw.path);
    assert!(whole.path.ends_with(".md"));
    assert!(raw.path.ends_with(".csv"));
    assert!(std::path::Path::new(&whole.path).exists());
    assert!(std::path::Path::new(&single.path).exists());
    assert!(std::path::Path::new(&raw.path).exists());

    // The single-record export has the one Record in it and not the other.
    let single_text = std::fs::read_to_string(&single.path).expect("the export is readable");
    assert!(single_text.contains("stops short"));
    assert!(!single_text.contains("bottled"));

    let whole_text = std::fs::read_to_string(&whole.path).expect("the export is readable");
    assert!(whole_text.contains("stops short"));
    assert!(whole_text.contains("bottled"));
}

#[test]
fn a_deletion_and_a_clear_both_leave_nothing_behind() {
    let dir = TempDir::new("window-delete");
    let desktop = window(
        &dir,
        &[
            (NOW, "The report stops short of apportioning blame."),
            (NOW - 60, "He bottled it at the last minute."),
        ],
    );
    let first = desktop.history().expect("the history reads").records[0].id;

    let after_delete = desktop.delete(first).expect("the record is deleted");
    assert_eq!(after_delete.records.len(), 1);
    assert!(
        desktop.show(first).is_err(),
        "a deleted record is gone, not hidden"
    );

    let (removed, after_clear) = desktop.clear().expect("the history clears");
    assert_eq!(removed, 1);
    assert!(after_clear.records.is_empty());
    assert!(!after_clear.has_records, "an empty history says so");
}

#[test]
fn a_tag_can_be_taken_off_again_and_a_blank_one_is_refused() {
    let dir = TempDir::new("window-tags");
    let desktop = window(&dir, &[(NOW, "He bottled it at the last minute.")]);
    let id = desktop.history().expect("the history reads").records[0].id;

    let tagged = desktop.tag(id, "  idiom  ").expect("the tag is written");
    // Trimmed, so that what the chip filters by is what was typed's intent.
    assert_eq!(tagged.records[0].tags, vec!["idiom".to_string()]);

    let cleared = desktop.forget_key("deepseek").expect("the key is removed");
    assert!(
        cleared.durable,
        "a key that has been taken away is gone for good, not a warning"
    );

    let untagged = desktop.untag(id, "idiom").expect("the tag is removed");
    assert!(untagged.records[0].tags.is_empty());

    let blank = desktop.tag(id, "   ");
    assert!(
        matches!(blank, Err(WindowError::Store(_))),
        "a blank tag is refused by the store rather than stored"
    );
}

#[test]
fn settings_show_every_level_with_its_meaning() {
    let dir = TempDir::new("window-levels");
    let desktop = window(&dir, &[]);

    let settings = desktop.settings().expect("the settings read");

    assert_eq!(settings.levels.level, "B2");
    assert_eq!(settings.levels.options.len(), 4);
    for option in &settings.levels.options {
        assert!(
            !option.meaning.trim().is_empty(),
            "level {} must show what it means (ticket 09)",
            option.value
        );
        assert!(!option.overridden, "nothing has been reworded yet");
    }
    // The meaning shown is the text actually sent, not a paraphrase of it.
    assert_eq!(
        settings.levels.options[2].meaning,
        plainly_core::Prompt::factory().descriptor(Level::B2)
    );
}

#[test]
fn the_settings_screen_is_five_sections_in_the_documented_order() {
    let dir = TempDir::new("window-sections");
    let desktop = window(&dir, &[]);

    let settings = desktop.settings().expect("the settings read");

    assert_eq!(
        settings.sections,
        vec!["语言与等级", "Provider", "提示词", "行为", "数据"]
    );
    assert_eq!(settings.behavior.panel_corners.len(), 4);
    assert_eq!(
        settings
            .data
            .exports
            .iter()
            .map(|export| export.value.as_str())
            .collect::<Vec<_>>(),
        vec!["markdown", "raw"]
    );
    assert_eq!(settings.data.exports.len(), 2);
    assert_eq!(
        settings
            .data
            .exports
            .iter()
            .map(|export| export.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Markdown", "原始 CSV"],
        "each export button carries its own label"
    );
}

#[test]
fn a_setting_the_session_cannot_honour_is_refused_where_it_is_written() {
    let dir = TempDir::new("window-capability");
    // A compositor with no data-control: the switch is drawn greyed, and the
    // write is refused too — a configuration that says "on" while the session
    // cannot do it is the same lie the greyed switch exists to avoid.
    let mut environment = desktop_with_wayland();
    environment.desktop = plainly_core::Desktop::new(["wl_compositor"]);
    let desktop = Desktop::open_at(dir.paths(), environment, Box::new(FixedClock(NOW)))
        .expect("the window opens")
        .with_secrets(Secrets::with_stores(
            plainly_core::EnvSecrets::from_pairs([(
                plainly_core::env_var_name("deepseek"),
                "sk-test",
            )]),
            None,
        ));

    let refused = desktop.update(&SettingsPatch {
        copy_auto_popup: Some(true),
        ..SettingsPatch::default()
    });
    assert!(refused.is_err(), "the write has to be refused, not stored");
    assert!(
        refused.unwrap_err().to_string().contains("data-control"),
        "the refusal names the protocol the session is missing"
    );
    assert!(
        !dir.paths().config_file().exists(),
        "a refused setting writes nothing"
    );
}

#[test]
fn a_stored_switch_the_session_cannot_honour_is_reported_off() {
    let dir = TempDir::new("window-stale-capability");
    // A configuration written on a session that had data-control, opened on one
    // that does not: the switch is drawn greyed, so a checked one would be a
    // state the user could neither reach nor leave. The file is not rewritten —
    // the key is the user's, and a session that gains data-control brings it
    // back.
    std::fs::create_dir_all(dir.paths().config_dir()).expect("the config directory is creatable");
    std::fs::write(dir.paths().config_file(), "[app]\ncopy_auto_popup = true\n")
        .expect("the config is writable");

    let mut environment = desktop_with_wayland();
    environment.desktop = plainly_core::Desktop::new(["wl_compositor"]);
    let desktop = Desktop::open_at(dir.paths(), environment, Box::new(FixedClock(NOW)))
        .expect("the window opens")
        .with_secrets(Secrets::with_stores(
            plainly_core::EnvSecrets::from_pairs([(
                plainly_core::env_var_name("deepseek"),
                "sk-test",
            )]),
            None,
        ));

    let behavior = desktop.settings().expect("the settings read").behavior;
    assert!(!behavior.copy_auto_popup_availability.enabled);
    assert!(
        !behavior.copy_auto_popup,
        "a switch the session cannot honour is not reported as on"
    );
    assert!(
        std::fs::read_to_string(dir.paths().config_file())
            .expect("the config is readable")
            .contains("copy_auto_popup = true"),
        "the user's own key is left as it was written"
    );
}

#[test]
fn the_outputs_the_compositor_publishes_are_the_ones_the_picker_offers() {
    let dir = TempDir::new("window-outputs");
    let desktop = window(&dir, &[]);

    let behavior = desktop.settings().expect("the settings read").behavior;

    // Every output beside "follow the current one", so that pinning the panel to
    // a display is a choice rather than a key only a hand-edit reaches.
    assert_eq!(behavior.outputs[0].value, "current");
    assert_eq!(
        behavior.outputs.len(),
        3,
        "two outputs, plus the current one"
    );
    assert_eq!(behavior.panel_display, "current");
    assert_eq!(
        behavior.outputs[1],
        plainly_desktop::settings::OutputOption {
            value: "eDP-1".to_string(),
            label: "Built-in display".to_string(),
        }
    );
}

#[test]
fn copy_on_popup_is_offered_only_where_data_control_exists() {
    // The switch is drawn in both sessions and explained in both: a capability
    // that is missing is named, never hidden (spec §12).
    let dir = TempDir::new("window-copy-auto");
    let desktop = window(&dir, &[]);
    let behavior = desktop.settings().expect("the settings read").behavior;
    assert!(
        behavior.copy_auto_popup_availability.enabled,
        "this session does have data-control, which is what the note rests on"
    );
    let with = availability(&plainly_core::Desktop::new(["ext_data_control_manager_v1"]));
    let without = availability(&plainly_core::Desktop::new(["wl_compositor"]));

    assert!(with.enabled);
    assert!(
        with.note.contains("data-control"),
        "the available case names the capability it rests on: {}",
        with.note
    );
    // And the sentence about what turning it on costs is the same constant the
    // screen draws, so the two cannot drift into promising and retracting.
    assert_eq!(
        plainly_desktop::strings::STRINGS.copy_auto_note,
        plainly_desktop::strings::COPY_AUTO_NOTE
    );
    assert!(
        plainly_desktop::strings::COPY_AUTO_NOTE.contains("--watch"),
        "the price of the switch names the process that pays it: {}",
        plainly_desktop::strings::COPY_AUTO_NOTE
    );
    assert!(!without.enabled);
    assert!(
        without.note.contains("data-control"),
        "the reason names the protocol: {}",
        without.note
    );
}

#[test]
fn the_first_run_opens_on_the_settings_screen_and_a_key_ends_it() {
    let dir = TempDir::new("window-first-run");
    // No key in any tier: the window has nothing to talk to, which is the state
    // spec §13 says the window opens on rather than showing a history that could
    // never grow.
    let desktop = window_with_env(&dir, &[], &[]);

    let state = desktop.state().expect("the window state reads");
    assert_eq!(state.screen, Screen::Settings);
    // On the Provider section, not the language one: that is where the empty
    // state and its two ways out are (spec §13).
    assert_eq!(state.settings_section, 1);
    assert!(state.settings.provider.first_run);
    let current = state
        .settings
        .provider
        .current
        .as_ref()
        .expect("deepseek resolves without a key, it just cannot be used yet");
    assert_eq!(current.key.source, "unset");
    assert!(
        state.settings.provider.first_run,
        "an unset key on a provider that needs one is the first run"
    );

    // Storing one is what the screen's own control does, and it is enough: the
    // window leaves the empty state with no separate flow.
    let stored = desktop.store_key("deepseek", "sk-live");
    assert!(
        !stored.message.is_empty(),
        "storing a key says where it went"
    );
    assert!(
        !stored.durable,
        "this session has no keyring, so the key does not survive the window, \
         and the banner says so in its warning colour"
    );

    let after = desktop.state().expect("the window state reads");
    assert_eq!(after.screen, Screen::History);
    assert_eq!(after.settings_section, 0);
    assert!(!after.settings.provider.first_run);
    assert_eq!(
        after.settings.provider.current.unwrap().key.source,
        "session"
    );
}

#[test]
fn a_key_already_in_the_environment_means_there_is_nothing_to_set_up() {
    let dir = TempDir::new("window-env-key");

    let state = window_with_env(&dir, &[], &[("deepseek", "sk-from-env")])
        .state()
        .expect("the window state reads");

    assert_eq!(state.screen, Screen::History);
    assert!(!state.settings.provider.first_run);
    let current = state.settings.provider.current.expect("it resolves");
    assert_eq!(current.key.source, "environment");
    assert!(current.key.text.contains("PLAINLY_DEEPSEEK_API_KEY"));
}

#[test]
fn a_session_only_key_says_it_does_not_survive_the_window() {
    let dir = TempDir::new("window-session-key");
    // A session with no keyring and nothing exported: the tier the key lands in
    // is then the one whose warning the user has to read.
    let secrets = Secrets::with_stores(plainly_core::EnvSecrets::empty(), None);
    let desktop = Desktop::open_at(
        dir.paths(),
        desktop_with_wayland(),
        Box::new(FixedClock(NOW)),
    )
    .expect("the window opens")
    .with_secrets(secrets);

    let stored = desktop.store_key("deepseek", "sk-session");
    assert!(
        !stored.durable,
        "a key that only this process holds is not durable"
    );

    let key = desktop
        .settings()
        .expect("the settings read")
        .provider
        .current
        .expect("it resolves")
        .key;
    assert_eq!(key.source, "session");
    assert!(
        key.text.contains("内存"),
        "a key that dies with the window has to say so: {}",
        key.text
    );
}

#[test]
fn an_update_writes_the_configuration_file_and_leaves_the_rest_of_it_alone() {
    let dir = TempDir::new("window-update");
    let desktop = window(&dir, &[]);

    let patch = SettingsPatch {
        level: Some(Level::A2),
        thinking: Some(Thinking::On),
        appendix: Some("只解释学术语域。".to_string()),
        panel_corner: Some(PanelCorner::BottomRight),
        panel_autohide: Some(true),
        panel_autohide_seconds: Some(45),
        panel_width: Some(620),
        panel_height: Some(540),
        show_grammar: Some(false),
        ..SettingsPatch::default()
    };

    let settings = desktop.update(&patch).expect("the settings are written");

    assert_eq!(settings.levels.level, "A2");
    assert_eq!(settings.prompt.appendix, "只解释学术语域。");
    assert_eq!(settings.behavior.panel_corner, "bottom-right");
    assert!(settings.behavior.panel_autohide);
    assert_eq!(settings.behavior.panel_autohide_seconds, 45);
    assert_eq!(settings.behavior.panel_width, 620);
    assert_eq!(settings.behavior.panel_height, 540);
    assert!(!settings.behavior.sections.grammar);

    // The file is the source of truth, not the in-memory document: re-reading it
    // is what proves the write happened.
    let text = std::fs::read_to_string(dir.paths().config_file()).expect("the config is readable");
    assert!(text.contains("level = \"A2\""), "{text}");
    assert!(text.contains("panel_autohide_seconds = 45"), "{text}");
    assert!(text.contains("panel_width = 620"), "{text}");
    assert!(text.contains("panel_height = 540"), "{text}");
    assert!(
        text.contains("native_language = \"Chinese\""),
        "an untouched key keeps its value: {text}"
    );
}

#[test]
fn a_duration_outside_the_range_is_refused_rather_than_written() {
    let dir = TempDir::new("window-autohide");
    let desktop = window(&dir, &[]);

    // 0 is the spelling spec §12 refuses: "off" already has a switch.
    for seconds in [0, 4, 301] {
        let patch = SettingsPatch {
            panel_autohide_seconds: Some(seconds),
            ..SettingsPatch::default()
        };
        assert!(
            desktop.update(&patch).is_err(),
            "{seconds} should not be accepted"
        );
    }

    assert!(
        !dir.paths().config_file().exists(),
        "a refused setting writes nothing at all"
    );
}

#[test]
fn a_panel_size_outside_the_range_is_refused_rather_than_written() {
    let dir = TempDir::new("window-panel-size");
    let desktop = window(&dir, &[]);

    // Both ends of both ranges, and the zero that no size may be: the screen
    // names the range in the sentence it refuses with, so a value it would show
    // and a value it would write cannot disagree.
    for (width, height) in [(319, 470), (901, 470), (470, 239), (470, 1201), (0, 470), (470, 0)] {
        let patch = SettingsPatch {
            panel_width: Some(width),
            panel_height: Some(height),
            ..SettingsPatch::default()
        };
        let error = desktop.update(&patch).expect_err("out of range");
        assert!(error.to_string().contains("px"), "{width}x{height}: {error}");
    }

    assert!(
        !dir.paths().config_file().exists(),
        "a refused setting writes nothing at all"
    );
}

#[test]
fn a_settings_write_that_meets_someone_elses_edit_is_refused_and_leaves_nothing_behind() {
    let dir = TempDir::new("window-external-edit");
    let desktop = window(&dir, &[]);
    desktop
        .update(&SettingsPatch {
            level: Some(Level::A1),
            ..SettingsPatch::default()
        })
        .expect("the first write lands");

    // Somebody else — the CLI, or a hand edit — writes the file after this
    // window read it.
    let mut text =
        std::fs::read_to_string(dir.paths().config_file()).expect("the file is readable");
    text.push_str("\n# written by somebody else\n");
    std::fs::write(dir.paths().config_file(), &text).expect("the file is writable");

    let refused = desktop.update(&SettingsPatch {
        level: Some(Level::C1),
        ..SettingsPatch::default()
    });
    let reason = refused
        .expect_err("the other write is not overwritten")
        .to_string();
    assert!(
        !reason.contains("Re-run the command"),
        "the window does not tell a person to re-run a command line: {reason}"
    );
    assert!(
        reason.contains("别的程序"),
        "the refusal says what happened, in the interface's words: {reason}"
    );

    // And the window is back in step with the file: it shows what is on disk,
    // not the value it failed to write.
    let settings = desktop.settings().expect("the settings read");
    assert_eq!(settings.levels.level, "A1");

    // Saving again works, which is what "in step" means for a handle that
    // remembers the bytes it read.
    desktop
        .update(&SettingsPatch {
            level: Some(Level::B2),
            ..SettingsPatch::default()
        })
        .expect("the handle can save again once it has re-read the file");
    assert_eq!(
        desktop.settings().expect("the settings read").levels.level,
        "B2"
    );
}

#[test]
fn a_language_that_is_nothing_is_refused() {
    let dir = TempDir::new("window-native");
    let desktop = window(&dir, &[]);

    for blank in ["", "   "] {
        let refused = desktop.update(&SettingsPatch {
            native_language: Some(blank.to_string()),
            ..SettingsPatch::default()
        });
        assert!(
            refused.is_err(),
            "a translation cannot be written in no language at all"
        );
    }

    assert!(
        !dir.paths().config_file().exists(),
        "a refused setting writes nothing at all"
    );
}

#[test]
fn a_prompt_version_changes_when_the_appendix_does() {
    let dir = TempDir::new("window-prompt");
    let desktop = window(&dir, &[]);

    let before = desktop
        .settings()
        .expect("the settings read")
        .prompt
        .version;
    let after = desktop
        .update(&SettingsPatch {
            appendix: Some("只解释学术语域。".to_string()),
            ..SettingsPatch::default()
        })
        .expect("the appendix is written")
        .prompt
        .version;

    assert_ne!(before, after, "an appendix is a new prompt version");
    // The appendix is *added* to the factory prompt: the factory text is not
    // editable, which is what keeps two histories comparable (spec §5).
    assert!(
        Prompt::factory()
            .system_prompt(Level::B2, "Chinese")
            .starts_with(
                plainly_core::FACTORY_PROMPT
                    .split("{{")
                    .next()
                    .unwrap()
                    .trim()
            )
    );
}

#[test]
fn the_window_state_carries_the_clock_the_settings_and_the_history() {
    let dir = TempDir::new("window-state");
    let desktop = window(
        &dir,
        &[(NOW - 30, "The report stops short of apportioning blame.")],
    );

    let state = desktop.state().expect("the state reads");

    assert_eq!(state.now, NOW);
    assert_eq!(state.screen, Screen::History);
    assert_eq!(state.settings_section, 0);
    assert_eq!(state.history.records.len(), 1);
    assert_eq!(state.settings.levels.level, "B2");
    assert!(!state.settings.provider.first_run);
}
