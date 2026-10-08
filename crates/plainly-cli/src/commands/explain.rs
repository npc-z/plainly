//! `plainly explain`: one Passage in, one Explanation out — or a long input in,
//! a batch of them out.
//!
//! stdout carries the product and nothing else — the five sections or the
//! Artifact as JSON — because this command exists to be piped. Everything a
//! person needs to read (which provider answered, what its endpoint takes, why
//! it did not, how far through a batch it is) goes to stderr, and the exit code
//! says which kind of outcome it was.
//!
//! The run itself is [`plainly_core::explain::run`]: probing the endpoint's
//! capability, the retry policy, and the one downgrade-and-ask-again cycle when
//! the endpoint rejects the shape. What is left here is where the Passage comes
//! from, how a long input is cut into Passages ([`plainly_core::split`]), what a
//! person is told, and which exit code that is.

use std::collections::HashMap;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use plainly_core::{
    Artifact, ChatCompletions, Config, ConfigFile, Downgrade, Endpoint, ExplainRequest, Failure,
    FailureKind, KeyRequirement, Level, Lookup, LookupKey, Paths, Prompt, ProviderSetup, Record,
    Resolution, SOURCE_LANGUAGE, Secrets, Stopped, Thinking, ThinkingSwitch, env_var_name, explain,
    render, split,
};

use crate::cli::{ExplainArgs, OutputFormat};
use crate::commands::{CommandError, cache, history_store, now, resolve_key, short_hash, tier};
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

    // The history store is opened before stdin is read for the same reason the
    // key is checked here: a store that cannot be opened is local state to fix,
    // and saying so must not need a pipe to end first.
    let mut store = history_store(&paths)?;

    let passage = match from_file {
        Some(passage) => passage,
        None => read_passage(&source)?,
    };

    // The effective prompt: the factory text plus the user's appendix and
    // descriptor overrides (spec §5). `--factory-prompt` leaves those out for
    // this run only, which is what makes the retry the message below offers a
    // single step rather than a trip into the configuration file.
    let prompt = if args.factory_prompt {
        Prompt::factory()
    } else {
        Prompt::from_config(&config.prompts)
    };

    // One Explanation is one Passage (spec §4), so a long input is cut into
    // Passages here and every chunk below is a whole run of its own: its own
    // Lookup Key, its own Record, its own line of progress. The panel's
    // five-chunk limit is deliberately not consulted — the CLI is the long
    // input's proper home (spec §6).
    let chunks = split::chunks(&passage);
    let batch = chunks.len() > 1;
    let total = chunks.len();
    let format = args.format.unwrap_or_default();

    let now = now()?;

    // Every distinct question is looked up before anything is sent, which is what
    // makes the request count below honest rather than a count of chunks: a chunk
    // already in the history is answered from the row that holds it and costs
    // nothing (spec §9). `--regenerate` is the one way past that.
    //
    // A Passage the batch repeats is the same question — the Lookup Key is over
    // the text, so it says so — and is planned once: asking it a second time
    // would pay twice for one answer, and the second write would overwrite the
    // first's Record.
    let mut turns = Vec::with_capacity(total);
    let mut first_seen: HashMap<LookupKey, usize> = HashMap::new();
    for chunk in &chunks {
        let request = request_for(chunk, &prompt, &config, &setup);
        let key = Lookup::from(&request).key();

        if let Some(&earlier) = first_seen.get(&key) {
            turns.push(Turn {
                request,
                key,
                stored: None,
                repeat_of: Some(earlier),
            });
            continue;
        }

        let stored = if args.regenerate {
            None
        } else {
            store.recall(&key, now)?
        };
        first_seen.insert(key.clone(), turns.len());
        turns.push(Turn {
            request,
            key,
            stored,
            repeat_of: None,
        });
    }

    let to_generate = turns
        .iter()
        .filter(|turn| turn.repeat_of.is_none() && turn.stored.is_none())
        .count();
    if batch {
        eprintln!(
            "plainly: {total} chunks; will make {to_generate} {}",
            request_word(to_generate)
        );
    }

    if to_generate > 0 {
        eprintln!(
            "plainly: explaining with {} ({}, thinking {}){}",
            setup.label,
            setup.model,
            setup.thinking.as_str(),
            // Provenance is which provider answered, and whether it is on this
            // machine is part of that: an answer from a local model is one nobody
            // can vouch for. The wording of the warning itself belongs to the panel
            // (spec §12, tickets/15); the CLI's job is to say where the answer came
            // from, and stdout stays the product (tickets/06).
            if setup.is_local() {
                " — local model on this machine"
            } else {
                ""
            }
        );
    }

    // The transport for a capability-resolved setup. The key is the surface's
    // business and lives here; everything the run decides is the same for the
    // panel, which is why none of it is in this file.
    let transport = |setup: &ProviderSetup| -> Box<dyn Endpoint> {
        Box::new(ChatCompletions::new(
            setup.clone(),
            key.clone().unwrap_or_default(),
        ))
    };
    let capabilities = cache(&paths);

    // The chunks run serially, in the order they were written, so the products
    // come out in that order too (spec §10). One chunk's failure is recorded and
    // the next one is still asked: a batch is not all-or-nothing. A chunk that
    // produced nothing leaves its slot `None` rather than shifting the rest.
    let mut results: Vec<Option<Artifact>> = vec![None; total];
    let mut failed = 0;
    for (index, turn) in turns.iter().enumerate() {
        let position = index + 1;

        // A repeat is only possible in a batch — one chunk cannot repeat itself —
        // so it is reported with the batch's positions like every other chunk.
        if let Some(earlier) = turn.repeat_of {
            match results[earlier].clone() {
                Some(artifact) => {
                    eprintln!(
                        "plainly: chunk {position}/{total}: the same Passage as chunk {}/{total}; \
                         reusing its Explanation",
                        earlier + 1
                    );
                    results[index] = Some(artifact);
                }
                None => {
                    // The first occurrence produced nothing, and asking the same
                    // failing question again would not answer it.
                    eprintln!(
                        "plainly: chunk {position}/{total} failed: the same Passage as chunk \
                         {}/{total} was not explained",
                        earlier + 1
                    );
                    failed += 1;
                }
            }
            continue;
        }

        if let Some(record) = &turn.stored {
            if batch {
                eprintln!(
                    "plainly: chunk {position}/{total}: {}",
                    reusing(&record.artifact)
                );
            } else {
                eprintln!("plainly: {}", reusing(&record.artifact));
            }
            results[index] = Some(record.artifact.clone());
            continue;
        }

        match explain::run(
            &setup,
            &turn.request,
            &transport,
            &capabilities,
            now,
            &pause,
        ) {
            Ok(run) => {
                report(&setup, &run.resolution, None, run.downgrade.as_ref());
                // Stored before it is printed, and the printed Artifact is the
                // *stored* one: a regeneration keeps the original `created_at`, so
                // printing the fresh one would contradict what `history show` says
                // about the same record. Storing first is also what stops a run whose
                // answer never reached the store from being paid for again.
                match store.remember(&turn.key, &run.artifact) {
                    Ok(record) => {
                        if batch {
                            eprintln!("plainly: chunk {position}/{total} done");
                        }
                        results[index] = Some(record.artifact);
                    }
                    Err(error) => {
                        // The store is the run's own state rather than one chunk's,
                        // so the batch stops here instead of paying for answers it
                        // cannot keep. The answers already paid for are shown first
                        // (spec §10) — this one included, or a lost write would
                        // throw away everything the run had produced.
                        eprintln!("plainly: chunk {position}/{total} was not stored: {error}");
                        results[index] = Some(run.artifact.clone());
                        print_all(&collected(&results), format, batch)?;
                        return Err(CommandError::Failed(error.to_string()));
                    }
                }
            }
            Err(failure) => {
                report(
                    &setup,
                    &failure.resolution,
                    failure.reprobe.as_ref(),
                    failure.downgrade.as_ref(),
                );
                report_contract_failure(&prompt, config.app.level, failure.failure.kind);
                let reason = describe(&failure.failure);
                if !batch {
                    return Err(CommandError::Failed(reason));
                }
                eprintln!("plainly: chunk {position}/{total} failed: {reason}");
                failed += 1;
            }
        }
    }

    // Everything that was produced goes out, whether or not a chunk failed: the
    // parts already paid for are kept (spec §10).
    print_all(&collected(&results), format, batch)?;

    if failed > 0 {
        return Err(CommandError::Failed(format!(
            "{failed} of {total} chunks failed; what succeeded is on stdout"
        )));
    }

    Ok(exit::SUCCESS)
}

/// The Artifacts a run produced, in input order. A chunk that failed has none,
/// and its absence must not shift the ones that follow it.
fn collected(results: &[Option<Artifact>]) -> Vec<Artifact> {
    results.iter().flatten().cloned().collect()
}

/// The product, on stdout and nothing else: the batch's documents or array, or
/// the one Artifact a single-chunk run produced.
fn print_all(
    artifacts: &[Artifact],
    format: OutputFormat,
    batch: bool,
) -> Result<(), CommandError> {
    if batch {
        return print_batch(artifacts, format);
    }

    let artifact = artifacts.first().ok_or_else(|| {
        // A single chunk that failed returned before reaching here; what is left
        // is "there was nothing to explain", kept loud rather than silent.
        CommandError::Failed("the input split into no Passage at all".to_string())
    })?;
    print(artifact, format)
}

/// One chunk's turn: the request it would send, the Lookup Key that identifies
/// the question, the stored answer when there already is one, and — when the
/// batch says the same thing twice — the earlier turn this one repeats.
struct Turn {
    request: ExplainRequest,
    key: LookupKey,
    stored: Option<Record>,
    /// The index of the earlier turn with the same Lookup Key. `None` for the
    /// first occurrence, and for every chunk of a run that has no repeats.
    repeat_of: Option<usize>,
}

/// The request for one Passage. Everything but the text is the same for every
/// chunk of a run, so the chunks are the same question at the same Level under
/// the same prompt — which is what leaves the Passage as the only difference
/// between their Lookup Keys.
fn request_for(
    passage: &str,
    prompt: &Prompt,
    config: &Config,
    setup: &ProviderSetup,
) -> ExplainRequest {
    ExplainRequest {
        passage: passage.to_string(),
        level: config.app.level,
        source_language: SOURCE_LANGUAGE.to_string(),
        native_language: config.app.native_language.clone(),
        provider: setup.name.clone(),
        model: setup.model.clone(),
        thinking: setup.thinking,
        system_prompt: prompt.system_prompt(config.app.level, &config.app.native_language),
        prompt_version: prompt.version(),
        prompt_label: prompt.label().to_string(),
    }
}

/// `request` or `requests`, so a count of one does not read as a bug.
fn request_word(count: usize) -> &'static str {
    if count == 1 { "request" } else { "requests" }
}

/// What a reuse says, not just that it reused something: which provider, model
/// and Level the stored Explanation came from is the same provenance a generated
/// run prints, and the user has no other way to tell whose answer this was. The
/// contract tier of the run that produced it is deliberately not claimed — a
/// Record does not carry one (spec §9), and guessing the tier of this run would
/// describe a request that was never made.
fn reusing(artifact: &Artifact) -> String {
    format!(
        "reusing the stored Explanation — {}/{} (thinking {}), {}, {}@{}; generated {}; \
         nothing was sent",
        artifact.provider,
        artifact.model,
        artifact.thinking.as_str(),
        artifact.level,
        artifact.prompt_label,
        short_hash(&artifact.prompt_version),
        artifact.generated_at.to_rfc3339(),
    )
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

/// What a person is told when the answer, not the endpoint, is what failed.
///
/// The run never falls back on its own (spec §5): switching to the factory
/// prompt behind the user's back would silently undo rules they wrote and hide
/// that anything went wrong. So a contract failure always names the prompt that
/// did not pass it — whose prompt it was, and, when there is one, the one-step
/// retry with the factory prompt. The decision and the fallback both come from
/// [`Prompt::fallback_after`]; this only words them.
fn report_contract_failure(prompt: &Prompt, level: Level, kind: FailureKind) {
    if !kind.is_contract() {
        return;
    }

    match prompt.fallback_after(kind, level) {
        // No whole command is printed: this run's Passage came from a file, a
        // pipe or a terminal, and the surface cannot know which, so a command
        // written out here would send a user who named a file back to stdin.
        Some(factory) => eprintln!(
            "plainly: your prompt did not pass the contract; re-run the same command with \
             --factory-prompt to retry on the factory prompt ({})",
            factory.label()
        ),
        // What this run sent is byte-identical to the factory text at this level
        // (the appendix is empty, or only another level's row was reworded), so
        // there is no fallback to offer: naming the factory prompt would name
        // the text already sent.
        None => eprintln!("plainly: the factory prompt did not pass the contract"),
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

/// The product of a batch, on stdout and nothing else.
///
/// Markdown is one document per chunk with a blank line between them, so a
/// reader can see where one Passage ends and the next begins (spec §10). JSON is
/// an array in the input's order. The shape follows the input rather than the
/// outcome: a batch that lost a chunk is still an array, holding the chunks that
/// succeeded.
fn print_batch(artifacts: &[Artifact], format: OutputFormat) -> Result<(), CommandError> {
    match format {
        OutputFormat::Markdown => {
            let documents: Vec<String> = artifacts
                .iter()
                .map(|artifact| render::markdown(&artifact.passage, &artifact.explanation))
                .collect();
            // Each document already ends in one newline, so a single newline
            // between them is the blank line.
            print!("{}", documents.join("\n"));
        }
        OutputFormat::Json => {
            let document = serde_json::to_string_pretty(&artifacts).map_err(|error| {
                CommandError::Failed(format!("cannot write the Artifacts as JSON: {error}"))
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
