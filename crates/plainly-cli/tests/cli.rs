//! The `plainly` command line, exercised the way a user or a script does: the
//! real binary, a real configuration file, and the exit codes in the contract.
//!
//! Nothing here needs a key, a provider or a network. The environment is
//! redirected into a temporary directory so no test can touch the developer's
//! own configuration.

mod support;

use support::{TempDir, code, stderr, stdout};

const SUCCESS: i32 = 0;
const FAILURE: i32 = 1;
const USAGE: i32 = 2;

#[test]
fn config_path_prints_the_file_under_the_config_directory() {
    let dir = TempDir::new("config-path");

    let output = dir.plainly(&["config", "path"]);

    assert_eq!(code(&output), SUCCESS);
    assert_eq!(stdout(&output).trim(), dir.config_file().to_string_lossy());
    assert_eq!(stderr(&output), "");
}

#[test]
fn config_show_prints_the_effective_configuration() {
    let dir = TempDir::new("config-show");

    let output = dir.plainly(&["config", "show"]);

    assert_eq!(code(&output), SUCCESS);
    let shown = stdout(&output);
    for expected in [
        "[app]",
        "[providers]",
        "[prompts]",
        "provider = \"deepseek\"",
        "level = \"B2\"",
        "panel_autohide_seconds = 20",
    ] {
        assert!(shown.contains(expected), "missing {expected} in:\n{shown}");
    }
}

#[test]
fn config_show_does_not_create_the_file() {
    let dir = TempDir::new("config-show-readonly");

    let output = dir.plainly(&["config", "show"]);

    assert_eq!(code(&output), SUCCESS);
    assert!(!dir.config_file().exists(), "showing is a read");
}

#[test]
fn config_set_writes_a_value_that_a_later_process_reads_back() {
    let dir = TempDir::new("config-set");

    let set = dir.plainly(&["config", "set", "app.level", "A2"]);
    assert_eq!(code(&set), SUCCESS);
    assert_eq!(stdout(&set), "", "confirmations belong on stderr");
    assert!(stderr(&set).contains("app.level"));

    let show = dir.plainly(&["config", "show"]);
    assert!(stdout(&show).contains("level = \"A2\""));

    // And it is a real file, with the shipped comments still in it.
    let written = std::fs::read_to_string(dir.config_file()).expect("the file exists");
    assert!(written.contains("Reading level"));
    assert!(written.contains("level = \"A2\""));
}

#[test]
fn config_set_accepts_values_without_quoting_them() {
    let dir = TempDir::new("config-set-coerce");

    for (key, value) in [
        ("app.export_format", "anki"),
        ("app.panel_corner", "bottom-right"),
        ("app.show_grammar", "false"),
        ("app.panel_autohide_seconds", "45"),
        ("providers.deepseek.thinking", "on"),
    ] {
        let output = dir.plainly(&["config", "set", key, value]);
        assert_eq!(code(&output), SUCCESS, "{key} = {value}: {output:?}");
    }

    let shown = stdout(&dir.plainly(&["config", "show"]));
    assert!(shown.contains("export_format = \"anki\""));
    assert!(shown.contains("panel_corner = \"bottom-right\""));
    assert!(shown.contains("show_grammar = false"));
    assert!(shown.contains("panel_autohide_seconds = 45"));
    assert!(shown.contains("thinking = \"on\""));
}

#[test]
fn config_set_refuses_an_unknown_key_without_writing_anything() {
    let dir = TempDir::new("config-set-unknown");

    let output = dir.plainly(&["config", "set", "app.levle", "A2"]);

    assert_eq!(code(&output), USAGE);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("unknown configuration key"));
    assert!(!dir.config_file().exists());
}

#[test]
fn config_set_refuses_a_bad_value_without_writing_anything() {
    let dir = TempDir::new("config-set-bad-value");

    let output = dir.plainly(&["config", "set", "app.level", "Z9"]);

    assert_eq!(code(&output), USAGE);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("A1, A2, B2, C1"));
    assert!(!dir.config_file().exists());
}

#[test]
fn config_set_refuses_a_zero_duration() {
    let dir = TempDir::new("config-set-zero");

    let output = dir.plainly(&["config", "set", "app.panel_autohide_seconds", "0"]);

    assert_eq!(code(&output), USAGE);
    assert!(stderr(&output).contains("app.panel_autohide"));
}

#[test]
fn a_key_without_a_keyring_is_not_reported_as_saved() {
    let dir = TempDir::new("key-no-keyring");

    let output = dir.plainly_with_stdin(&["providers", "key", "set", "openai"], "sk-test");

    assert_eq!(code(&output), FAILURE);
    assert_eq!(stdout(&output), "", "a key never travels over stdout");
    let message = stderr(&output);
    assert!(message.contains("NOT saved"), "{message}");
    assert!(message.contains("PLAINLY_OPENAI_API_KEY"), "{message}");
}

#[test]
fn an_empty_key_on_stdin_is_a_usage_error() {
    let dir = TempDir::new("key-empty");

    let output = dir.plainly_with_stdin(&["providers", "key", "set", "openai"], "");

    assert_eq!(code(&output), USAGE);
    assert!(stderr(&output).contains("no key on stdin"));
}

#[test]
fn clearing_a_key_without_a_keyring_is_quietly_idempotent() {
    let dir = TempDir::new("key-clear");

    let output = dir.plainly(&["providers", "key", "clear", "openai"]);

    assert_eq!(code(&output), SUCCESS);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("nothing to clear"));
}

#[test]
fn saying_nothing_is_a_usage_error_and_leaves_stdout_clean() {
    let dir = TempDir::new("no-subcommand");

    let output = dir.plainly(&[]);

    assert_eq!(code(&output), USAGE);
    assert_eq!(
        stdout(&output),
        "",
        "help for a usage error belongs on stderr"
    );
    assert!(stderr(&output).contains("Usage"));
}

#[test]
fn an_unknown_subcommand_is_a_usage_error() {
    let dir = TempDir::new("unknown-subcommand");

    let output = dir.plainly(&["frobnicate"]);

    assert_eq!(code(&output), USAGE);
    assert_eq!(stdout(&output), "");
    assert!(!stderr(&output).is_empty());
}

/// Asking for help is not an error: it goes to stdout and exits zero.
#[test]
fn help_and_version_go_to_stdout_and_exit_zero() {
    let dir = TempDir::new("help");

    let help = dir.plainly(&["--help"]);
    assert_eq!(code(&help), SUCCESS);
    assert!(stdout(&help).contains("Usage"));
    assert!(stdout(&help).contains("config"));

    let version = dir.plainly(&["--version"]);
    assert_eq!(code(&version), SUCCESS);
    assert!(stdout(&version).starts_with("plainly "));
}
