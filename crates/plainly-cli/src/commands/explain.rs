//! `plainly explain`: one Passage in, one Explanation out.
//!
//! stdout carries the product and nothing else — the five sections or the
//! Artifact as JSON — because this command exists to be piped. Everything a
//! person needs to read (which provider answered, why it did not) goes to
//! stderr, and the exit code says which kind of outcome it was.

use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use plainly_core::{
    ChatCompletions, ConfigFile, ExplainRequest, Failure, KeyRequirement, Paths, ProviderSetup,
    SOURCE_LANGUAGE, Secrets, Stopped, Thinking, ThinkingSwitch, Timestamp, env_var_name, prompt,
    render, retry,
};

use crate::cli::{ExplainArgs, OutputFormat};
use crate::commands::CommandError;
use crate::exit;

/// What to say when there is nothing to explain. One wording, used both when a
/// terminal would otherwise be waited on and when the input turned out empty.
const NO_PASSAGE: &str = "there is no Passage: pipe one in (`echo … | plainly`) \
                          or name a file (`plainly explain file.md`)";

pub fn run(args: ExplainArgs) -> Result<u8, CommandError> {
    // The invocation is settled before anything is read: a bare `plainly` at a
    // terminal has no Passage, and answering that beats waiting for input that
    // is not coming.
    let source = source(args.file.as_deref(), std::io::stdin().is_terminal())?;

    // A named file is read right away, because a path that is not there is a
    // usage error and configuration has nothing to say about it. Stdin is
    // deliberately left for later: a missing key should be reported without
    // draining a pipe that may be a large file, or one that never ends.
    let from_file = match source {
        Source::File(_) => Some(read_passage(&source)?),
        Source::Stdin => None,
    };

    let paths = Paths::discover().map_err(|error| CommandError::Failed(error.to_string()))?;
    let config = ConfigFile::open(paths.config_file())?.config()?;

    let setup = ProviderSetup::resolve(&config.app.provider, &config)
        .map_err(|error| CommandError::NotConfigured(error.to_string()))?;

    let key = resolve_key(&setup, &Secrets::from_process_env())?;
    if key.is_none() && setup.key == KeyRequirement::Required {
        return Err(CommandError::NotConfigured(format!(
            "no API key for {}: export {}, or store one with \
             `plainly providers key set {}`",
            setup.name,
            env_var_name(&setup.name),
            setup.name,
        )));
    }

    let passage = match from_file {
        Some(passage) => passage,
        None => read_passage(&source)?,
    };

    let request = ExplainRequest {
        passage,
        level: config.app.level,
        source_language: SOURCE_LANGUAGE.to_string(),
        native_language: config.app.native_language.clone(),
        provider: setup.name.clone(),
        model: setup.model.clone(),
        thinking: setup.thinking,
        system_prompt: prompt::system_prompt(config.app.level, &config.app.native_language),
        prompt_version: prompt::version(),
        prompt_label: prompt::PROMPT_LABEL.to_string(),
    };

    let provider = ChatCompletions::new(setup.clone(), key.unwrap_or_default());

    eprintln!(
        "plainly: explaining with {} ({}, thinking {})",
        setup.label,
        setup.model,
        setup.thinking.as_str()
    );
    if setup.thinking == Thinking::On && setup.thinking_switch == ThinkingSwitch::Unsupported {
        // The switch is a capability, and this endpoint has none we know of:
        // saying nothing would let a setting look like it worked (spec §7).
        eprintln!(
            "plainly: note: {} takes no thinking switch, so this run is not more detailed",
            setup.label
        );
    }

    let artifact = retry::explain(&provider, &request, now()?, &pause)
        .map_err(|failure| CommandError::Failed(describe(&failure)))?;

    let format = args.format.unwrap_or_default();
    match format {
        OutputFormat::Markdown => print!(
            "{}",
            render::markdown(&artifact.passage, &artifact.explanation)
        ),
        OutputFormat::Json => {
            let document = serde_json::to_string_pretty(&artifact).map_err(|error| {
                CommandError::Failed(format!("cannot write the Artifact as JSON: {error}"))
            })?;
            println!("{document}");
        }
    }

    Ok(exit::SUCCESS)
}

/// Wait as the policy asks, and say so while waiting.
///
/// This is where a person learns that the run is retrying rather than hanging;
/// the immediate retries are silent because there is nothing to wait through.
fn pause(wait: Duration) {
    if wait > Duration::ZERO {
        eprintln!("plainly: asking again in {}s", wait.as_secs());
    }
    std::thread::sleep(wait);
}

/// What a person is told when the policy gives up.
///
/// The class is not named — the reason already says what happened — but whether
/// anything was retried, and why the attempts stopped there, are part of the
/// answer. So is the fact that a failed run leaves nothing behind: there is no
/// Explanation, so there is nothing to store, and the input was only ever read.
fn describe(failure: &Failure) -> String {
    let stopped = match failure.stopped {
        Stopped::Repeated => "the same answer came back twice",
        Stopped::NotRetryable => "another attempt would fail the same way",
        Stopped::Exhausted => "the attempts this class allows are used up",
    };
    let attempts = if failure.retried() {
        format!("tried {} times; {stopped}", failure.attempts)
    } else {
        format!("not retried: {stopped}")
    };

    format!(
        "{} ({attempts}). Nothing was stored and the input was not changed.",
        failure.reason
    )
}

/// The key to send, if there is one.
///
/// A tier that fails to answer is normally an error rather than a skip, because
/// quietly using a different credential is worse than saying so ([`Secrets`]).
/// An endpoint that may authenticate nothing is the exception: a local runtime
/// is exactly what a headless box points at, there is no key to protect, and
/// the endpoint still gets to answer 401 if it wanted one. A provider that
/// needs a key keeps the failure, because there the fix is the keyring.
fn resolve_key(setup: &ProviderSetup, secrets: &Secrets) -> Result<Option<String>, CommandError> {
    match secrets.resolve(&setup.name) {
        Ok(key) => Ok(key.map(|key| key.secret)),
        Err(error) if setup.key == KeyRequirement::Optional => {
            eprintln!(
                "plainly: {error}; sending no key, since {} may not need one",
                setup.label
            );
            Ok(None)
        }
        Err(error) => Err(CommandError::Failed(error.to_string())),
    }
}

/// The Passage, from the named file or from stdin.
fn read_passage(source: &Source) -> Result<String, CommandError> {
    let passage = match source {
        Source::File(path) => std::fs::read_to_string(path).map_err(|error| {
            CommandError::Usage(format!("cannot read {}: {error}", path.display()))
        })?,
        Source::Stdin => {
            let mut passage = String::new();
            std::io::stdin()
                .lock()
                .read_to_string(&mut passage)
                .map_err(|error| CommandError::Failed(format!("cannot read stdin: {error}")))?;
            passage
        }
    };

    if passage.trim().is_empty() {
        return Err(CommandError::Usage(NO_PASSAGE.to_string()));
    }
    Ok(passage)
}

/// Where a run's Passage comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    File(PathBuf),
    Stdin,
}

/// Decide the source before reading anything.
///
/// The terminal case is the one that matters: with no file and a terminal on
/// stdin, reading would wait for a pipe that is never coming, which the person
/// at the keyboard experiences as a hang rather than as a mistake.
fn source(file: Option<&Path>, stdin_is_terminal: bool) -> Result<Source, CommandError> {
    match file {
        Some(path) => Ok(Source::File(path.to_path_buf())),
        None if stdin_is_terminal => Err(CommandError::Usage(NO_PASSAGE.to_string())),
        None => Ok(Source::Stdin),
    }
}

/// The current instant, for stamping the Artifact.
fn now() -> Result<Timestamp, CommandError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| CommandError::Failed(format!("the system clock is before 1970: {error}")))?
        .as_secs();

    i64::try_from(seconds)
        .ok()
        .and_then(Timestamp::from_unix_seconds)
        .ok_or_else(|| {
            CommandError::Failed(
                "the system clock is outside the range Plainly can stamp".to_string(),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use plainly_core::{Config, EnvSecrets, SecretError, SecretStore};

    /// A bare invocation at a terminal must not wait for input that is not
    /// coming. This is the decision, taken before any read.
    #[test]
    fn a_terminal_with_no_file_is_a_usage_error_rather_than_a_wait() {
        let error = source(None, true).unwrap_err();

        assert!(matches!(error, CommandError::Usage(_)), "got {error:?}");
        assert!(error.to_string().contains("pipe one in"), "{error}");
    }

    #[test]
    fn a_pipe_is_read_when_no_file_is_named() {
        assert_eq!(source(None, false).unwrap(), Source::Stdin);
    }

    /// A named file is read even when the command happens to be run from a
    /// terminal, where stdin is a terminal too.
    #[test]
    fn a_named_file_wins_over_the_terminal() {
        assert_eq!(
            source(Some(Path::new("passage.md")), true).unwrap(),
            Source::File(PathBuf::from("passage.md"))
        );
    }

    /// A keyring that is present but cannot answer: a headless box with a
    /// session bus and no Secret Service behind it.
    struct BrokenKeyring;

    impl SecretStore for BrokenKeyring {
        fn get(&self, _provider: &str) -> Result<Option<String>, SecretError> {
            Err(SecretError::Backend("no secret service".to_string()))
        }

        fn set(&self, _provider: &str, _secret: &str) -> Result<(), SecretError> {
            Err(SecretError::Backend("no secret service".to_string()))
        }

        fn delete(&self, _provider: &str) -> Result<(), SecretError> {
            Err(SecretError::Backend("no secret service".to_string()))
        }
    }

    fn broken_keyring() -> Secrets {
        Secrets::with_stores(EnvSecrets::empty(), Some(Box::new(BrokenKeyring)))
    }

    fn setup(name: &str, config: &str) -> ProviderSetup {
        let config = Config::parse(config).expect("the fixture is valid TOML");
        ProviderSetup::resolve(name, &config).expect("the fixture provider resolves")
    }

    /// The whole point of `KeyRequirement::Optional`: a local runtime is what a
    /// headless box points at, and a keyring that cannot answer must not be the
    /// thing that stops it.
    #[test]
    fn a_provider_that_may_authenticate_nothing_survives_a_broken_keyring() {
        let setup = setup("ollama", "[providers.ollama]\nmodel = \"qwen3.5:4b\"\n");

        assert_eq!(resolve_key(&setup, &broken_keyring()).unwrap(), None);
    }

    /// A provider that needs a key keeps the failure: there the keyring is the
    /// thing to fix, and sending an unauthenticated request would hide that.
    #[test]
    fn a_provider_that_needs_a_key_still_reports_the_keyring() {
        let setup = setup("deepseek", "");

        let error = resolve_key(&setup, &broken_keyring()).unwrap_err();

        assert!(matches!(error, CommandError::Failed(_)), "got {error:?}");
        assert!(error.to_string().contains("no secret service"), "{error}");
    }
}
