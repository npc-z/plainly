//! Where Plainly's files go.
//!
//! The rules are the interesting part, and the inputs are data, so these run
//! anywhere without touching the process environment.

use plainly_core::{CONFIG_FILE, IDENTIFIER, PathError, PathInputs, Paths};

#[test]
fn xdg_variables_are_honoured() {
    let inputs = PathInputs::new("/home/learner")
        .with_var("XDG_CONFIG_HOME", "/xdg/config")
        .with_var("XDG_DATA_HOME", "/xdg/data")
        .with_var("XDG_CACHE_HOME", "/xdg/cache");

    let paths = Paths::resolve(&inputs).expect("a home directory is set");

    assert_eq!(
        paths.config_dir().to_path_buf(),
        path(&["/xdg/config", IDENTIFIER])
    );
    assert_eq!(
        paths.data_dir().to_path_buf(),
        path(&["/xdg/data", IDENTIFIER])
    );
    assert_eq!(
        paths.cache_dir().to_path_buf(),
        path(&["/xdg/cache", IDENTIFIER])
    );
}

#[test]
fn without_xdg_the_defaults_follow_the_home_directory() {
    let paths = Paths::resolve(&PathInputs::new("/home/learner")).expect("a home directory is set");

    assert_eq!(
        paths.config_dir(),
        path(&["/home/learner", ".config", IDENTIFIER])
    );
    assert_eq!(
        paths.data_dir(),
        path(&["/home/learner", ".local", "share", IDENTIFIER])
    );
    assert_eq!(
        paths.cache_dir(),
        path(&["/home/learner", ".cache", IDENTIFIER])
    );
}

/// The XDG spec says a relative variable is ignored, and an empty one is not a
/// path at all. Both must fall back rather than produce a path relative to
/// wherever the process happens to be.
#[test]
fn empty_or_relative_xdg_values_are_ignored() {
    let inputs = PathInputs::new("/home/learner")
        .with_var("XDG_CONFIG_HOME", "")
        .with_var("XDG_DATA_HOME", "relative/data")
        .with_var("XDG_CACHE_HOME", "");

    let paths = Paths::resolve(&inputs).expect("a home directory is set");

    assert_eq!(
        paths.config_dir(),
        path(&["/home/learner", ".config", IDENTIFIER])
    );
    assert_eq!(
        paths.data_dir(),
        path(&["/home/learner", ".local", "share", IDENTIFIER])
    );
    assert_eq!(
        paths.cache_dir(),
        path(&["/home/learner", ".cache", IDENTIFIER])
    );
}

#[test]
fn every_directory_is_namespaced_by_the_identifier() {
    let paths = Paths::resolve(&PathInputs::new("/home/learner")).expect("a home directory is set");

    for directory in [paths.config_dir(), paths.data_dir(), paths.cache_dir()] {
        assert!(
            directory.ends_with(IDENTIFIER),
            "{} should end with the identifier",
            directory.display()
        );
    }
}

#[test]
fn the_configuration_file_is_named_and_placed_by_the_config_directory() {
    let paths = Paths::resolve(&PathInputs::new("/home/learner")).expect("a home directory is set");

    assert_eq!(paths.config_file(), paths.config_dir().join(CONFIG_FILE));
}

/// The identifier decides where everyone's files live, so it is worth pinning:
/// changing it silently moves a user's history.
#[test]
fn the_identifier_is_the_shipped_one() {
    assert_eq!(IDENTIFIER, "dev.plainly.app");
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[test]
fn no_home_directory_is_an_error_on_platforms_that_need_one() {
    let error = Paths::resolve(&PathInputs::without_home()).expect_err("there is no home");
    assert_eq!(error, PathError::NoHome);
}

/// `discover` reads the real environment; on any machine that can run the test
/// suite there is a home directory.
#[test]
fn discover_resolves_against_the_real_environment() {
    let paths = Paths::discover().expect("this process has a home directory");

    assert!(paths.config_file().ends_with(CONFIG_FILE));
    assert!(paths.config_dir().ends_with(IDENTIFIER));
}

fn path(parts: &[&str]) -> std::path::PathBuf {
    parts.iter().collect()
}
