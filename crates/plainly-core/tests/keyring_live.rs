//! The real OS keyring — the one path that cannot be faked at the seam.
//!
//! It is `#[ignore]`d because it needs a session bus with a Secret Service
//! running, which a build machine may not have. Run it explicitly:
//!
//! ```text
//! cargo test -p plainly-core -- --ignored
//! ```

use plainly_core::{Cleared, EnvSecrets, KeySource, KeyringSecrets, SecretStore, Secrets};

/// Whether this machine is supposed to have a usable keyring.
///
/// Set `PLAINLY_REQUIRE_KEYRING=1` where it is (CI on a desktop session, a
/// developer's own machine) so that a missing bus fails instead of skipping.
fn require_keyring() -> bool {
    std::env::var("PLAINLY_REQUIRE_KEYRING")
        .map(|value| !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
}

/// Removes the test's entry even if an assertion fails partway.
struct Cleanup(String);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = KeyringSecrets::new().delete(&self.0);
    }
}

#[test]
#[ignore = "needs a session bus with a Secret Service"]
fn a_key_can_be_stored_read_and_cleared_in_the_real_keyring() {
    // A name of its own, so a stray entry is obviously a leftover from a test.
    let provider = format!("plainly-selftest-{}", std::process::id());
    let _cleanup = Cleanup(provider.clone());

    let keyring = KeyringSecrets::new();

    // A machine with no Secret Service on its bus cannot run this test at all,
    // and failing here would read as "the keyring code is broken" when nothing
    // about it has been exercised. Silence is worse: a skipped test that reports
    // `ok` is indistinguishable from a passing one. So the skip is loud, and a
    // machine that is *supposed* to have a keyring can demand one outright.
    if let Err(error) = keyring.get(&provider) {
        let message = format!("no usable Secret Service on this machine ({error})");
        if require_keyring() {
            panic!("PLAINLY_REQUIRE_KEYRING is set, but {message}");
        }
        eprintln!("SKIPPED, NOT A PASS: the keyring live test did not run: {message}");
        return;
    }

    assert_eq!(
        keyring.get(&provider).expect("the keyring answers"),
        None,
        "the test entry should not already exist"
    );

    keyring
        .set(&provider, "selftest-secret")
        .expect("the host has a usable secret service");
    assert_eq!(
        keyring.get(&provider).expect("the keyring answers"),
        Some("selftest-secret".to_string())
    );

    let secrets = Secrets::with_stores(EnvSecrets::empty(), Some(Box::new(KeyringSecrets::new())));
    assert_eq!(secrets.key_source(&provider), Ok(Some(KeySource::Keyring)));

    assert_eq!(secrets.clear(&provider), Ok(Cleared::Keyring));
    assert_eq!(
        keyring.get(&provider).expect("the keyring answers"),
        None,
        "clearing must remove the entry"
    );
}
