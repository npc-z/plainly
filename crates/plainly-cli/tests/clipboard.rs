//! `plainly explain --clipboard`: the clipboard as a Passage, and the one
//! refusal Plainly makes on purpose.
//!
//! The reader asks `wl-paste` for the clipboard and hands what it says to core,
//! so a test puts its own `wl-paste` on `PATH`: the subject is what the CLI does
//! with the text and the MIME types a clipboard publishes (spec §11).

mod support;

use plainly_core::PASSWORD_HINT;

use support::TempDir;
use support::provider::{FakeProvider, Reply};
use support::{code, custom_provider, stderr, stdout};

const SUCCESS: i32 = 0;
const FAILURE: i32 = 1;
const USAGE: i32 = 2;
const REFUSED: i32 = 4;

const PASSAGE: &str = "The committee had already gone to ground.";

/// A contract-shaped answer, as a model would return it.
const ANSWER: &str = r#"{
  "comprehensible": "The committee had already hidden.",
  "glosses": [
    { "expression": "go to ground", "gloss": "hide so that nobody can find you" }
  ],
  "grammar": null,
  "translation": "委员会已经躲了起来。"
}"#;

/// A `wl-paste` for a clipboard that publishes `types`, answers a request for
/// the password hint with `hint`, and every other request with `passage`.
fn wl_paste(types: &[&str], hint: &str, passage: &str) -> String {
    let list = types.join("\\n");
    format!(
        "case \"$*\" in\n  \
         *--list-types*) printf '{list}\\n' ;;\n  \
         *{PASSWORD_HINT}*) printf '%s' '{hint}' ;;\n  \
         *) printf '%s' '{passage}' ;;\nesac"
    )
}

/// The clipboard can be a run's Passage like a pipe or a file.
#[test]
fn the_clipboard_text_is_explained_like_any_other_passage() {
    let dir = TempDir::new("clipboard-plain");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));
    let path = dir.wl_paste_stub(&wl_paste(&["text/plain;charset=utf-8"], "public", PASSAGE));

    let output = dir.plainly_with(
        &["--clipboard"],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key"), ("PATH", &path)],
    );

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    assert!(stdout(&output).contains(PASSAGE), "{}", stdout(&output));
    assert_eq!(
        server.chat_requests()[0].body["messages"][1]["content"],
        PASSAGE,
        "the clipboard's text is the Passage"
    );
    assert!(
        stderr(&output).contains("explaining with"),
        "{}",
        stderr(&output)
    );
}

/// The hint's other value is an ordinary copy: KeePassXC publishes the same
/// type with `public`, and that must not refuse anything.
#[test]
fn the_hint_with_a_public_value_is_explained() {
    let dir = TempDir::new("clipboard-public");
    let server = FakeProvider::start([Reply::content(ANSWER)]);
    dir.write_config(&custom_provider(&server.base_url()));
    let path = dir.wl_paste_stub(&wl_paste(
        &["text/plain;charset=utf-8", PASSWORD_HINT],
        "public",
        PASSAGE,
    ));

    let output = dir.plainly_with(
        &["--clipboard"],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key"), ("PATH", &path)],
    );

    assert_eq!(code(&output), SUCCESS, "{}", stderr(&output));
    assert_eq!(server.chat_requests().len(), 1);
}

/// A secret marker refuses the run before configuration or the store are
/// touched: no key is configured here, so a run that got as far as resolving a
/// provider would answer 3 instead.
#[test]
fn a_clipboard_marked_secret_is_refused_before_anything_is_sent_or_stored() {
    let dir = TempDir::new("clipboard-secret");
    let server = FakeProvider::start([]);
    let path = dir.wl_paste_stub(&wl_paste(
        &["text/plain;charset=utf-8", PASSWORD_HINT],
        "secret",
        PASSAGE,
    ));

    let output = dir.plainly_with(&["--clipboard"], "", &[("PATH", &path)]);

    assert_eq!(code(&output), REFUSED);
    assert_eq!(stdout(&output), "", "a refusal produces no product");
    assert!(
        server.requests().is_empty(),
        "nothing is sent for refused content: {:?}",
        server.requests()
    );

    let message = stderr(&output);
    assert!(message.contains("sensitive"), "{message}");
    assert!(message.contains("nothing was sent"), "{message}");

    let history = dir.plainly(&["history", "list"]);
    assert_eq!(code(&history), SUCCESS, "{}", stderr(&history));
    assert_eq!(stdout(&history), "", "a refused Passage is never stored");
}

/// The rule is about the content, not about where it would be answered: a local
/// model on this machine is not an exception (spec §11).
#[test]
fn a_local_provider_does_not_relax_the_rule() {
    let dir = TempDir::new("clipboard-local-secret");
    let server = FakeProvider::start([]);
    dir.write_config(&format!(
        "[app]\nprovider = \"ollama\"\n\n\
         [providers.ollama]\nendpoint = \"http://{}/v1\"\nmodel = \"qwen3.5:4b\"\n",
        server.authority().trim_start_matches("http://")
    ));
    let path = dir.wl_paste_stub(&wl_paste(
        &["text/plain;charset=utf-8", PASSWORD_HINT],
        "secret",
        PASSAGE,
    ));

    let output = dir.plainly_with(&["--clipboard"], "", &[("PATH", &path)]);

    assert_eq!(code(&output), REFUSED, "{}", stderr(&output));
    assert!(
        server.requests().is_empty(),
        "a local provider is not an exception: {:?}",
        server.requests()
    );
}

/// A marker that is published but cannot be read leaves the run in doubt, and
/// doubt is not a reason to send: the run refuses rather than treating an unread
/// marker as a public one, and says which half of the criterion is missing.
#[test]
fn a_marker_that_cannot_be_read_stops_the_run_rather_than_looking_public() {
    let dir = TempDir::new("clipboard-unreadable-marker");
    let server = FakeProvider::start([]);
    dir.write_config(&custom_provider(&server.base_url()));
    let path = dir.wl_paste_stub(
        "case \"$*\" in\n  \
         *--list-types*) printf 'text/plain;charset=utf-8\\nx-kde-passwordManagerHint\\n' ;;\n  \
         *x-kde-passwordManagerHint*) printf 'the owner refused' >&2; exit 1 ;;\n  \
         *) printf '%s' 'The committee had already gone to ground.' ;;\nesac",
    );

    let output = dir.plainly_with(
        &["--clipboard"],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key"), ("PATH", &path)],
    );

    assert_eq!(code(&output), REFUSED, "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert!(server.requests().is_empty());
    let message = stderr(&output);
    assert!(message.contains(PASSWORD_HINT), "{message}");
    assert!(message.contains("could not be read"), "{message}");
    assert!(message.contains("nothing was sent"), "{message}");
}

/// A clipboard that holds something other than text is no Passage either:
/// `wl-paste` asked for a generic text type is what refuses it, rather than
/// `wl-paste`'s own fallback handing over an image's bytes as a Passage.
#[test]
fn a_clipboard_that_holds_no_text_is_a_usage_error() {
    let dir = TempDir::new("clipboard-no-text");
    let server = FakeProvider::start([]);
    dir.write_config(&custom_provider(&server.base_url()));
    let path = dir.wl_paste_stub(
        "case \"$*\" in\n  \
         *--list-types*) printf 'image/png\\n' ;;\n  \
         *--type*text*) printf 'no suitable type\\n' >&2; exit 1 ;;\n  \
         *) printf 'PNG-BYTES' ;;\nesac",
    );

    let output = dir.plainly_with(
        &["--clipboard"],
        "",
        &[("PLAINLY_STUB_API_KEY", "test-key"), ("PATH", &path)],
    );

    assert_eq!(code(&output), USAGE, "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert!(server.requests().is_empty());
    assert!(
        stderr(&output).contains("holds no text"),
        "{}",
        stderr(&output)
    );
}

/// A selection whose text is empty is no Passage, and so is no selection at all:
/// `wl-paste` reports the second as a failure, and that failure is read as what
/// it is rather than as a session that cannot be reached.
#[test]
fn an_empty_clipboard_is_a_usage_error() {
    let dir = TempDir::new("clipboard-empty");
    let path = dir.wl_paste_stub(&wl_paste(&["text/plain;charset=utf-8"], "public", ""));

    let output = dir.plainly_with(&["--clipboard"], "", &[("PATH", &path)]);

    assert_eq!(code(&output), USAGE);
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains("no Passage"),
        "{}",
        stderr(&output)
    );

    // The same, as the real command reports it: no selection is a failed
    // `wl-paste` rather than an empty success.
    let dir = TempDir::new("clipboard-no-selection");
    let path = dir.wl_paste_stub(
        "case \"$*\" in\n  \
         *--list-types*) printf 'Nothing is copied\\n' >&2; exit 1 ;;\n  \
         *) printf '%s' 'unreachable' ;;\nesac",
    );

    let output = dir.plainly_with(&["--clipboard"], "", &[("PATH", &path)]);

    assert_eq!(code(&output), USAGE, "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains("the clipboard is empty"),
        "{}",
        stderr(&output)
    );
}

/// Without a reader the clipboard cannot be read at all, and the failure names
/// the program that is missing rather than blaming the clipboard.
#[test]
fn a_missing_wl_paste_is_a_failure_that_names_what_is_missing() {
    let dir = TempDir::new("clipboard-no-reader");
    let path = dir.empty_path();

    let output = dir.plainly_with(&["--clipboard"], "", &[("PATH", &path)]);

    assert_eq!(code(&output), FAILURE);
    assert_eq!(stdout(&output), "");
    let message = stderr(&output);
    assert!(message.contains("wl-paste"), "{message}");
    assert!(message.contains("wl-clipboard"), "{message}");
}

/// Two sources for one run is a mistake in the asking, not a rule about which
/// one wins.
#[test]
fn naming_a_file_and_the_clipboard_at_once_is_a_usage_error() {
    let dir = TempDir::new("clipboard-both-sources");

    let output = dir.plainly_with(&["explain", "passage.md", "--clipboard"], "", &[]);

    assert_eq!(code(&output), USAGE);
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("not both"), "{}", stderr(&output));
}
