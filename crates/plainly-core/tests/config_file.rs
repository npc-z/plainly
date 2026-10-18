//! The configuration file: defaults, editing, atomicity, and the refusal to
//! clobber someone else's edit.

mod support;

use std::fs;

use plainly_core::{Config, ConfigError, ConfigFile, DEFAULT_CONFIG, Level, Thinking};
use support::TempDir;

#[test]
fn the_shipped_default_document_describes_the_default_configuration() {
    // Guards drift: adding a field without adding it to the document, or
    // changing a default in one place only, fails here.
    assert_eq!(
        Config::parse(DEFAULT_CONFIG).expect("the shipped default parses"),
        Config::default()
    );
}

#[test]
fn the_shipped_default_document_declares_all_three_sections() {
    for section in ["[app]", "[providers]", "[prompts]"] {
        assert!(
            DEFAULT_CONFIG.contains(section),
            "the default configuration should declare {section}"
        );
    }
}

#[test]
fn a_missing_file_opens_as_the_default_and_is_not_created() {
    let dir = TempDir::new("config-missing");
    let path = dir.join("config.toml");

    let file = ConfigFile::open(&path).expect("a missing file is not an error");

    assert!(!file.exists());
    assert_eq!(file.config().expect("defaults parse"), Config::default());
    assert!(!path.exists(), "opening must not create the file");
}

#[test]
fn a_partial_file_falls_back_to_defaults_key_by_key() {
    let dir = TempDir::new("config-partial");
    let path = dir.join("config.toml");
    fs::write(&path, "[app]\nlevel = \"A1\"\n").expect("the file is writable");

    let file = ConfigFile::open(&path).expect("the file opens");
    let config = file.config().expect("the partial file parses");

    assert_eq!(config.app.level, Level::A1);
    // Everything the file does not mention keeps its default.
    assert_eq!(config.app.provider, Config::default().app.provider);
    assert_eq!(config.app.panel_autohide_seconds, 20);
    assert_eq!(config.app.panel_width, Config::default().app.panel_width);
    assert_eq!(config.app.panel_height, Config::default().app.panel_height);
}

#[test]
fn unknown_keys_load_and_survive_a_write() {
    let dir = TempDir::new("config-forward");
    let path = dir.join("config.toml");
    fs::write(
        &path,
        "[app]\nlevel = \"A2\"\nfrom_a_newer_version = \"keep me\"\n",
    )
    .expect("the file is writable");

    let mut file = ConfigFile::open(&path).expect("the file opens");
    // A key this version does not know must not break loading...
    file.config().expect("unknown keys are ignored");

    // ...nor be dropped when we write something else.
    file.set("app.panel_autohide", "true")
        .expect("a known key is settable");
    file.save().expect("the save succeeds");

    let written = fs::read_to_string(&path).expect("the file is readable");
    assert!(
        written.contains("from_a_newer_version = \"keep me\""),
        "unknown keys should survive: {written}"
    );
}

#[test]
fn comments_survive_an_edit() {
    let dir = TempDir::new("config-comments");
    let path = dir.join("config.toml");
    fs::write(&path, "# my note about the level\n[app]\nlevel = \"B2\"\n")
        .expect("the file is writable");

    let mut file = ConfigFile::open(&path).expect("the file opens");
    file.set("app.level", "C1").expect("the level is settable");
    file.save().expect("the save succeeds");

    let written = fs::read_to_string(&path).expect("the file is readable");
    assert!(written.contains("# my note about the level"));
    assert!(written.contains("level = \"C1\""));
}

#[test]
fn values_are_coerced_by_the_type_of_the_key() {
    let dir = TempDir::new("config-coerce");
    let path = dir.join("config.toml");
    let mut file = ConfigFile::open(&path).expect("a missing file opens");

    // No quotes needed on the command line; the key's type decides.
    file.set("app.level", "A2").expect("level");
    file.set("app.export_format", "raw").expect("export format");
    file.set("app.panel_corner", "bottom-right")
        .expect("corner");
    file.set("app.show_grammar", "false").expect("boolean");
    file.set("app.panel_autohide_seconds", "45")
        .expect("seconds");
    file.set("app.panel_width", "620").expect("width");
    file.set("app.panel_height", "540").expect("height");
    file.set("providers.deepseek.thinking", "on")
        .expect("thinking");
    file.set("providers.deepseek.model", "deepseek-flash")
        .expect("model");
    file.set("prompts.appendix", "Prefer British spellings.")
        .expect("appendix");
    file.set("prompts.level_descriptors.A2", "very common words")
        .expect("descriptor");

    let config = file.config().expect("the edited document parses");
    assert_eq!(config.app.level, Level::A2);
    assert_eq!(config.app.panel_autohide_seconds, 45);
    assert_eq!(config.app.panel_width, 620);
    assert_eq!(config.app.panel_height, 540);
    assert!(!config.app.show_grammar);
    assert_eq!(config.providers["deepseek"].thinking, Thinking::On);
    assert_eq!(
        config.providers["deepseek"].model.as_deref(),
        Some("deepseek-flash")
    );
    assert_eq!(config.prompts.level_descriptors["A2"], "very common words");
}

#[test]
fn an_unknown_key_is_refused() {
    let dir = TempDir::new("config-unknown");
    let mut file = ConfigFile::open(dir.join("config.toml")).expect("a missing file opens");

    let error = file
        .set("app.levle", "A2")
        .expect_err("a typo is not a key");
    assert!(matches!(error, ConfigError::UnknownKey { .. }), "{error}");

    let error = file
        .set("providers.deepseek.nope", "x")
        .expect_err("unknown provider field");
    assert!(matches!(error, ConfigError::UnknownKey { .. }), "{error}");

    let error = file
        .set("providers.deepseek", "x")
        .expect_err("a section is not a key");
    assert!(matches!(error, ConfigError::UnknownKey { .. }), "{error}");
}

#[test]
fn a_value_of_the_wrong_shape_is_refused() {
    let dir = TempDir::new("config-value");
    let mut file = ConfigFile::open(dir.join("config.toml")).expect("a missing file opens");

    for (key, value) in [
        ("app.level", "Z9"),
        ("app.export_format", "pdf"),
        // Anki was a format v0 shipped and no longer does; the name is now
        // refused like any other value the enum has no variant for.
        ("app.export_format", "anki"),
        ("app.panel_corner", "middle"),
        ("app.show_grammar", "maybe"),
        ("app.panel_autohide_seconds", "soon"),
        ("app.panel_width", "wide"),
        ("app.panel_height", "-40"),
        ("providers.deepseek.thinking", "unsupported"),
        ("prompts.level_descriptors.Z9", "whatever"),
    ] {
        let error = file.set(key, value).expect_err("this value is invalid");
        assert!(
            matches!(error, ConfigError::Value { .. }),
            "{key} = {value:?} should be a value error, got {error}"
        );
    }
}

/// "Off" has a switch. A duration of zero would be a second spelling of it, and
/// two spellings drift apart.
#[test]
fn a_zero_duration_is_refused_and_points_at_the_switch() {
    let dir = TempDir::new("config-zero");
    let mut file = ConfigFile::open(dir.join("config.toml")).expect("a missing file opens");

    let error = file
        .set("app.panel_autohide_seconds", "0")
        .expect_err("zero is not a duration");

    assert!(error.to_string().contains("app.panel_autohide"), "{error}");
}

/// A size of zero is nobody's: the panel would be a surface with nothing in it,
/// and the key would read as written while doing nothing at all.
#[test]
fn a_zero_size_is_refused() {
    let dir = TempDir::new("config-zero-pixels");
    let mut file = ConfigFile::open(dir.join("config.toml")).expect("a missing file opens");

    for key in ["app.panel_width", "app.panel_height"] {
        let error = file.set(key, "0").expect_err("zero is not a size");
        assert!(error.to_string().contains("invisible"), "{key}: {error}");
    }
}

/// The legal ranges live in core now, so the settings screen, the panel and the
/// CLI cannot each keep their own copy of the same three numbers (spec §4).
#[test]
fn the_legal_ranges_are_exported_from_core() {
    use plainly_core::config::{AUTO_HIDE_RANGE, PANEL_HEIGHT_RANGE, PANEL_WIDTH_RANGE};

    assert_eq!(AUTO_HIDE_RANGE, (5, 300));
    assert_eq!(PANEL_WIDTH_RANGE, (320, 900));
    assert_eq!(PANEL_HEIGHT_RANGE, (240, 1200));
}

/// The write side is strict: the settings screen's range is now the file's
/// range, so `config set` cannot write a number that screen would refuse. The
/// boundary values themselves are legal.
#[test]
fn a_numeric_value_outside_the_range_is_refused_with_the_range_as_facts() {
    let dir = TempDir::new("config-range");
    let mut file = ConfigFile::open(dir.join("config.toml")).expect("a missing file opens");

    for (key, value) in [
        ("app.panel_autohide_seconds", "5"),
        ("app.panel_autohide_seconds", "300"),
        ("app.panel_width", "320"),
        ("app.panel_width", "900"),
        ("app.panel_height", "240"),
        ("app.panel_height", "1200"),
    ] {
        file.set(key, value)
            .unwrap_or_else(|error| panic!("{key} = {value} is a legal value: {error}"));
    }

    for (key, value, min, max) in [
        ("app.panel_autohide_seconds", "4", 5, 300),
        ("app.panel_autohide_seconds", "301", 5, 300),
        ("app.panel_width", "319", 320, 900),
        ("app.panel_width", "901", 320, 900),
        ("app.panel_height", "239", 240, 1200),
        ("app.panel_height", "1201", 240, 1200),
    ] {
        let error = file
            .set(key, value)
            .expect_err("a value outside the range is refused");
        let message = error.to_string();

        match error {
            ConfigError::OutOfRange {
                key: reported_key,
                value: reported_value,
                min: reported_min,
                max: reported_max,
            } => {
                assert_eq!(reported_key, key);
                assert_eq!(reported_value, value.parse::<u32>().unwrap());
                assert_eq!(reported_min, min);
                assert_eq!(reported_max, max);
            }
            other => panic!("{key} = {value} should be out of range, got {other}"),
        }

        // The sentence the CLI prints has to name the range, or the user has to
        // go looking for it.
        assert!(message.contains(&value.to_string()), "{message}");
        assert!(message.contains(&min.to_string()), "{message}");
        assert!(message.contains(&max.to_string()), "{message}");
    }
}

/// Zero keeps its own dedicated messages: the range check comes after them, so
/// `0` still says why zero is special rather than quoting a range that starts at
/// a number the user may not have thought about.
#[test]
fn a_zero_is_refused_by_its_own_rule_before_the_range_is_considered() {
    let dir = TempDir::new("config-zero-first");
    let mut file = ConfigFile::open(dir.join("config.toml")).expect("a missing file opens");

    for (key, expected) in [
        (
            "app.panel_autohide_seconds",
            "not how auto-dismiss is turned off",
        ),
        ("app.panel_width", "invisible"),
        ("app.panel_height", "invisible"),
    ] {
        let error = file.set(key, "0").expect_err("zero is refused");
        assert!(
            matches!(error, ConfigError::Value { .. }),
            "{key} = 0 should keep its own message, got {error:?}"
        );
        assert!(error.to_string().contains(expected), "{key}: {error}");
    }
}

/// The read side stays tolerant: a hand-edited file with an out-of-range number
/// still opens, because the panel must always be able to appear and close. The
/// panel clamps and the settings screen shows the file as written; the strictness
/// is on the way in, not on the way out (spec §4).
#[test]
fn a_hand_edited_out_of_range_file_still_opens() {
    let dir = TempDir::new("config-range-read");
    let path = dir.join("config.toml");
    fs::write(
        &path,
        "[app]\npanel_width = 2000\npanel_height = 0\npanel_autohide_seconds = 4294967295\n",
    )
    .expect("the file is writable");

    let file = ConfigFile::open(&path).expect("an out-of-range file still opens");
    let config = file.config().expect("it still parses");

    assert_eq!(config.app.panel_width, 2000);
    assert_eq!(config.app.panel_height, 0);
    assert_eq!(config.app.panel_autohide_seconds, u32::MAX);

    // The same tolerance stated at the parse seam itself.
    let parsed = Config::parse("[app]\npanel_width = 4294967295\n").expect("parse is permissive");
    assert_eq!(parsed.app.panel_width, u32::MAX);
}

#[test]
fn a_refused_value_leaves_the_document_untouched() {
    let dir = TempDir::new("config-untouched");
    let mut file = ConfigFile::open(dir.join("config.toml")).expect("a missing file opens");
    let before = file.to_toml();

    let _ = file.set("app.level", "Z9").expect_err("invalid");

    assert_eq!(file.to_toml(), before);
}

#[test]
fn saving_creates_the_directory_and_leaves_no_temporary_files_behind() {
    let dir = TempDir::new("config-atomic");
    let path = dir.join("nested/config.toml");
    let mut file = ConfigFile::open(&path).expect("a missing file opens");

    file.set("app.level", "A2").expect("set");
    file.save().expect("save");

    assert!(path.exists());
    let leftovers: Vec<_> = fs::read_dir(path.parent().expect("a parent"))
        .expect("the directory is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temporary files left: {leftovers:?}");
}

#[test]
fn saving_twice_in_a_row_is_fine() {
    let dir = TempDir::new("config-twice");
    let path = dir.join("config.toml");
    let mut file = ConfigFile::open(&path).expect("a missing file opens");

    file.set("app.level", "A2").expect("set");
    file.save().expect("first save");
    file.set("app.level", "C1").expect("set again");
    file.save().expect("second save");

    let text = fs::read_to_string(&path).expect("the file is readable");
    assert!(text.contains("level = \"C1\""));
}

/// The CLI and the desktop app both write this file. Whoever writes second must
/// notice, not overwrite.
#[test]
fn saving_refuses_to_clobber_an_edit_made_by_someone_else() {
    let dir = TempDir::new("config-external");
    let path = dir.join("config.toml");
    fs::write(&path, "[app]\nlevel = \"B2\"\n").expect("the file is writable");

    let mut file = ConfigFile::open(&path).expect("the file opens");
    fs::write(&path, "[app]\nlevel = \"C1\"\n").expect("written by someone else");

    file.set("app.provider", "openai").expect("set");
    let error = file.save().expect_err("the external edit must be noticed");
    assert!(
        matches!(error, ConfigError::ExternallyModified { .. }),
        "{error}"
    );

    let text = fs::read_to_string(&path).expect("the file is readable");
    assert!(
        text.contains("level = \"C1\""),
        "the other writer's content should be intact: {text}"
    );
}

#[test]
fn a_file_created_behind_our_back_is_also_an_external_edit() {
    let dir = TempDir::new("config-created");
    let path = dir.join("config.toml");

    let mut file = ConfigFile::open(&path).expect("a missing file opens");
    fs::write(&path, "[app]\nlevel = \"A1\"\n").expect("someone creates it");

    file.set("app.level", "A2").expect("set");
    let error = file.save().expect_err("the creation must be noticed");
    assert!(matches!(error, ConfigError::ExternallyModified { .. }));
}

#[test]
fn the_effective_configuration_fills_in_every_default() {
    let dir = TempDir::new("config-effective");
    let path = dir.join("config.toml");
    fs::write(&path, "[app]\nlevel = \"A1\"\n").expect("the file is writable");

    let file = ConfigFile::open(&path).expect("the file opens");
    let effective = file.effective_toml().expect("the effective view");

    assert!(effective.contains("level = \"A1\""));
    assert!(effective.contains("provider = \"deepseek\""));
    assert!(effective.contains("panel_autohide_seconds = 20"));
    assert!(effective.contains("panel_width = 470"), "{effective}");
    assert!(effective.contains("panel_height = 470"), "{effective}");
    for section in ["[app]", "[providers]", "[prompts]"] {
        assert!(
            effective.contains(section),
            "missing {section}: {effective}"
        );
    }
    // And it round-trips: what `show` prints is what Plainly would use.
    assert_eq!(
        Config::parse(&effective).expect("the effective view parses"),
        file.config().expect("the file parses")
    );
}

#[test]
fn the_default_document_is_a_valid_configuration_before_anything_is_written() {
    let dir = TempDir::new("config-default-doc");
    let file = ConfigFile::open(dir.join("config.toml")).expect("a missing file opens");

    let effective = file.effective_toml().expect("the effective view");
    assert_eq!(
        Config::parse(&effective).expect("parses"),
        Config::default()
    );
}

/// A key must land in its own table. When a sub-table comes last, a naive
/// append would put the parent's key inside the child's scope — and because
/// unknown keys are tolerated, that mistake would look like a successful write
/// that reads back as the default.
#[test]
fn a_key_lands_in_its_own_table_even_when_a_sub_table_comes_last() {
    let dir = TempDir::new("config-ordering");
    let path = dir.join("config.toml");
    fs::write(
        &path,
        "[prompts]\n[prompts.level_descriptors]\nA2 = \"very common words\"\n",
    )
    .expect("the file is writable");

    let mut file = ConfigFile::open(&path).expect("the file opens");
    file.set("prompts.appendix", "Prefer British spellings.")
        .expect("set");
    file.save().expect("save");

    let config = ConfigFile::open(&path)
        .expect("the file opens again")
        .config()
        .expect("the file parses");
    assert_eq!(config.prompts.appendix, "Prefer British spellings.");
    assert_eq!(config.prompts.level_descriptors["A2"], "very common words");

    // The same hazard between sections: an app key written while the file's
    // last table happens to belong to another section.
    let mut file = ConfigFile::open(&path).expect("the file opens");
    file.set("app.level", "C1").expect("set");
    file.set("app.native_language", "Japanese").expect("set");
    file.save().expect("save");

    let config = ConfigFile::open(&path)
        .expect("the file opens again")
        .config()
        .expect("the file parses");
    assert_eq!(config.app.level, Level::C1);
    assert_eq!(config.app.native_language, "Japanese");
    assert_eq!(config.prompts.appendix, "Prefer British spellings.");
}

/// `prompts.level_descriptors.A2.extra` is a key nobody wrote. Reporting it as
/// an invalid *level* would quote "A2.extra" back at the user and send them
/// looking for a level they never typed.
#[test]
fn a_dotted_level_descriptor_key_is_an_unknown_key() {
    let dir = TempDir::new("config-level-dotted");
    let mut file = ConfigFile::open(dir.join("config.toml")).expect("a missing file opens");

    let error = file
        .set("prompts.level_descriptors.A2.extra", "x")
        .expect_err("that is not a key");

    assert!(matches!(error, ConfigError::UnknownKey { .. }), "{error}");
}
