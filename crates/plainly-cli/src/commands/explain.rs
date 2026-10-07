//! `plainly explain`: one Passage in, one Explanation out.
//!
//! stdout carries the product and nothing else — the five sections or the
//! Artifact as JSON — because this command exists to be piped. Everything a
//! person needs to read (which provider answered, what its endpoint takes, why
//! it did not) goes to stderr, and the exit code says which kind of outcome it
//! was.
//!
//! The run itself is [`plainly_core::explain::run`]: probing the endpoint's
//! capability, the retry policy, and the one downgrade-and-ask-again cycle when
//! the endpoint rejects the shape. What is left here is where the Passage comes
//! from, what a person is told, and which exit code that is.

use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use plainly_core::{
    Artifact, ChatCompletions, ConfigFile, Downgrade, Endpoint, ExplainRequest, Failure,
    KeyRequirement, Paths, ProviderSetup, Resolution, SOURCE_LANGUAGE, Secrets, Stopped, Thinking,
    ThinkingSwitch, env_var_name, explain, prompt, render,
};

use crate::cli::{ExplainArgs, OutputFormat};
use crate::commands::{CommandError, cache, now, resolve_key, tier};
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

    eprintln!(
        "plainly: explaining with {} ({}, thinking {})",
        setup.label,
        setup.model,
        setup.thinking.as_str()
    );

    // The transport for a capability-resolved setup. The key is the surface's
    // business and lives here; everything the run decides is the same for the
    // panel, which is why none of it is in this file.
    let transport = |setup: &ProviderSetup| -> Box<dyn Endpoint> {
        Box::new(ChatCompletions::new(
            setup.clone(),
            key.clone().unwrap_or_default(),
        ))
    };

    match explain::run(&setup, &request, &transport, &cache(&paths), now()?, &pause) {
        Ok(run) => {
            report(&setup, &run.resolution, None, run.downgrade.as_ref());
            print(&run.artifact, args.format.unwrap_or_default())?;
            Ok(exit::SUCCESS)
        }
        Err(failure) => {
            report(
                &setup,
                &failure.resolution,
                failure.reprobe.as_ref(),
                failure.downgrade.as_ref(),
            );
            Err(CommandError::Failed(describe(&failure.failure)))
        }
    }
}

/// What a person is told about the endpoint, whatever the run's outcome.
///
/// Every fact here is one the run established rather than a guess: the tier the
/// request was actually held to, a correction the endpoint forced, and — when
/// there is one — the reason nothing could be probed at all. The tier is the one
/// thing said every time, because it is the promise the answer comes with and
/// the user cannot otherwise tell the two tiers apart (spec §7).
///
/// `resolution` is the one behind the request being reported on, which on a
/// failed run is the *rejected* attempt rather than whatever a later probe
/// concluded; `reprobe` carries that later probe when it did not change what the
/// run could send.
fn report(
    setup: &ProviderSetup,
    resolution: &Resolution,
    reprobe: Option<&Resolution>,
    downgrade: Option<&Downgrade>,
) {
    if let Some(downgrade) = downgrade {
        eprintln!(
            "plainly: {} rejected the shape Plainly assumed ({}); probed again",
            setup.label,
            one_line(&downgrade.reason),
        );
    }
    if let Some(reason) = &resolution.unanswered {
        eprintln!("plainly: could not probe {} ({reason})", setup.label);
    }

    // Always, and before anything else a person has to weigh: which tier this
    // Passage was held to. "The endpoint enforces the contract" and "we check
    // the answer ourselves and ask again" are not the same promise, and a run
    // that says nothing leaves the user unable to tell them apart (spec §7).
    eprintln!("plainly: contract: {}", tier(resolution.capability.schema));

    if let Some(reprobe) = reprobe {
        match &reprobe.unanswered {
            // A correction needs the endpoint to answer; a probe that could not
            // be answered leaves nothing to ask with, and saying so is what
            // stops the report from looking like a silent give-up.
            Some(reason) => {
                eprintln!("plainly: probed again and could not settle it ({reason})");
            }
            None => eprintln!(
                "plainly: probed again: {} — the rejected request would not change, \
                 so it was not re-sent",
                tier(reprobe.capability.schema),
            ),
        }
    }

    // The latest probe is the one whose cache outcome still matters: an earlier
    // failure is answered by the conclusion it could not keep, which a later
    // probe either replaced or failed to reach in turn.
    if let Some(reason) = reprobe.unwrap_or(resolution).unwritten.as_ref() {
        eprintln!("plainly: note: the probe result could not be cached: {reason}");
    }

    if setup.thinking == Thinking::On
        && resolution.capability.thinking == ThinkingSwitch::Unsupported
    {
        // The switch is a capability, and this endpoint has none we know of:
        // saying nothing would let a setting look like it worked (spec §7).
        eprintln!(
            "plainly: note: {} takes no thinking switch, so this run is not more detailed",
            setup.label
        );
    }
}

/// An endpoint's own complaint is quoted back inside a sentence of ours, so it
/// is flattened onto one line first.
fn one_line(reason: &str) -> String {
    reason.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The product, on stdout and nothing else.
fn print(artifact: &Artifact, format: OutputFormat) -> Result<(), CommandError> {
    match format {
        OutputFormat::Markdown => print!(
            "{}",
            render::markdown(&artifact.passage, &artifact.explanation)
        ),
        OutputFormat::Json => {
            let document = serde_json::to_string_pretty(artifact).map_err(|error| {
                CommandError::Failed(format!("cannot write the Artifact as JSON: {error}"))
            })?;
            println!("{document}");
        }
    }
    Ok(())
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
