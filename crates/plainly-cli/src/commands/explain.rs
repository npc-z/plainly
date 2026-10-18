//! `plainly explain`: one Passage in, one Explanation out — or a long input in,
//! a batch of them out.
//!
//! The run itself is [`plainly_core::Runner`]: it opens the configuration, the
//! provider, the credential, the history store, the capability cache and the
//! prompt, and decides one Passage's turn (recall, or generate and remember).
//! What is left here is where the Passage comes from, how a long input is cut
//! into Passages ([`plainly_core::split`]), which of them repeat, what a person
//! is told, and which exit code that is.

use std::collections::HashMap;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use plainly_core::{
    Artifact, Downgrade, Failure, FailureKind, Level, LookupKey, PASSWORD_HINT, Prompt,
    PromptChoice, ProviderSetup, Reading, Recall, Record, Resolution, RunNotice, Runner,
    RunnerError, Stopped, Thinking, ThinkingSwitch, Timestamp, TurnFailure, TurnNotice, render,
    split,
};

use crate::cli::{ExplainArgs, OutputFormat};
use crate::commands::{CommandError, credential_error, short_hash, tier};
use crate::exit;

/// What to say when there is nothing to explain. One wording, used both when a
/// terminal would otherwise be waited on and when the input turned out empty.
const NO_PASSAGE: &str = "there is no Passage: pipe one in (`echo … | plainly`) \
                          or name a file (`plainly explain file.md`)";

pub fn run(args: ExplainArgs) -> Result<u8, CommandError> {
    let source = source(
        args.file.as_deref(),
        args.clipboard,
        std::io::stdin().is_terminal(),
    )?;

    // Everything that can be read without a pipe is read now. A named file
    // because a path that is not there is a usage error, and the clipboard
    // because what it holds decides whether this run happens at all: a refusal
    // has to leave no trace, so it is taken before the run is opened (spec §11).
    // Stdin is deliberately left for later: a missing key should be reported
    // without draining a pipe that may be a large file, or one that never ends.
    let from_source = match &source {
        Source::File(path) => Some(read_file(path)?),
        Source::Clipboard => Some(read_clipboard()?),
        Source::Stdin => None,
    };

    // The instant is produced before the run opens so the store is stamped with
    // the moment the person asked, not the moment a pipe ended.
    let now = Timestamp::now().map_err(|error| CommandError::Failed(error.to_string()))?;

    let choice = if args.factory_prompt {
        PromptChoice::Factory
    } else {
        PromptChoice::Configured
    };

    // Opening is what resolves the configured provider's credential, so a
    // missing key is still reported without draining stdin. A retry's wait is
    // reported through the same seam the run uses for its transport.
    let mut runner = Runner::open(choice, now)
        .map_err(runner_error)?
        .with_pause(Box::new(pause));

    let mut noticed = 0;
    report_notices(&runner, &mut noticed);

    let passage = match from_source {
        Some(passage) => passage,
        None => read_stdin()?,
    };

    // One Explanation is one Passage (spec §4), so a long input is cut into
    // Passages here and every chunk below is a whole turn of its own: its own
    // Lookup Key, its own Record, its own line of progress. The panel's
    // five-chunk limit is deliberately not consulted — the CLI is the long
    // input's proper home (spec §6).
    let chunks = split::chunks(&passage);
    let batch = chunks.len() > 1;
    let total = chunks.len();
    let format = args.format.unwrap_or_default();

    // Every distinct question is looked up before anything is sent, so the
    // request count below is a count of questions rather than of chunks. A chunk
    // already in the history is answered from its row and costs nothing
    // (spec §9); `--regenerate` is the one way past that. A Passage repeated
    // inside the batch is the same question and is asked once, because a second
    // write would overwrite the first's Record.
    let mut turns = Vec::with_capacity(total);
    let mut first_seen: HashMap<LookupKey, usize> = HashMap::new();
    for chunk in &chunks {
        // The runner builds the key, so this surface and the panel cannot
        // disagree about the fields it is made of.
        let key = runner.lookup(chunk);

        if let Some(&earlier) = first_seen.get(&key) {
            turns.push(PlannedTurn {
                stored: None,
                repeat_of: Some(earlier),
            });
            continue;
        }

        // `cached` is the same recall a `Reuse` turn would take, so the plan
        // and the turn cannot disagree about whether this chunk is a hit. A
        // store that cannot be read is not a miss: it aborts the run through the
        // store-error mapping and nothing is sent.
        let stored = if args.regenerate {
            None
        } else {
            runner.cached(chunk)?
        };
        first_seen.insert(key, turns.len());
        turns.push(PlannedTurn {
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
        let setup = runner.setup();
        eprintln!(
            "plainly: explaining with {} ({}, thinking {}){}",
            setup.label,
            setup.model,
            setup.thinking.as_str(),
            // Whether the model runs on this machine is provenance: an answer
            // from a local model is one nobody can vouch for (spec §12).
            if setup.is_local() {
                " — local model on this machine"
            } else {
                ""
            }
        );
    }

    // The chunks run serially, in the order they were written, so the products
    // come out in that order too (spec §10). One chunk's failure is recorded and
    // the next one is still asked: a batch is not all-or-nothing. A chunk that
    // produced nothing leaves its slot `None` rather than shifting the rest.
    let mut results: Vec<Option<Artifact>> = vec![None; total];
    let mut failed = 0;
    let mut not_stored = 0;
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

        // `--regenerate` was planned as a miss above, and it is a miss here too:
        // the same flag has to mean the same thing to the plan and the turn.
        let recall = if args.regenerate {
            Recall::Bypass { provider: None }
        } else {
            Recall::Reuse
        };
        let outcome = runner.explain(&chunks[index], recall);
        report_notices(&runner, &mut noticed);

        match outcome {
            Ok(turn) => {
                if let Some(resolution) = &turn.resolution {
                    report(runner.setup(), resolution, None, turn.downgrade.as_ref());
                }

                // A write the history refused is not a failed turn: the answer
                // was paid for and is printed, the batch goes on, and the exit
                // code carries the fact that it was not stored (ADR 0003).
                let reason = turn.notices.iter().find_map(|notice| match notice {
                    TurnNotice::NotStored { reason } => Some(reason.clone()),
                });
                if let Some(reason) = reason {
                    eprintln!("plainly: chunk {position}/{total} was not stored: {reason}");
                    not_stored += 1;
                } else if batch {
                    eprintln!("plainly: chunk {position}/{total} done");
                }
                results[index] = Some(turn.artifact);
            }
            Err(TurnFailure::Run(failure)) => {
                report(
                    runner.setup(),
                    &failure.resolution,
                    failure.reprobe.as_ref(),
                    failure.downgrade.as_ref(),
                );
                report_contract_failure(runner.prompt(), runner.level(), failure.failure.kind);
                let reason = describe(&failure.failure);
                if !batch {
                    return Err(CommandError::Failed(reason));
                }
                eprintln!("plainly: chunk {position}/{total} failed: {reason}");
                failed += 1;
            }
            // A `Bypass` turn's provider that could not be used. The CLI never
            // names one today, but a failed chunk is the right mapping: it costs
            // its own chunk and nothing else.
            Err(TurnFailure::Unavailable { provider, error }) => {
                let reason = format!("cannot use {provider}: {error}");
                if !batch {
                    return Err(CommandError::Failed(reason));
                }
                eprintln!("plainly: chunk {position}/{total} failed: {reason}");
                failed += 1;
            }
            // The history could not answer the recall this turn needed. The
            // provider was never asked, so nothing was paid for; a failed chunk
            // is the right mapping, like any other turn failure.
            Err(TurnFailure::Store(error)) => {
                let reason = error.to_string();
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

    match (failed, not_stored) {
        (0, 0) => Ok(exit::SUCCESS),
        (failed, 0) => Err(CommandError::Failed(format!(
            "{failed} of {total} chunks failed; what succeeded is on stdout"
        ))),
        (0, not_stored) => Err(CommandError::Failed(format!(
            "{not_stored} of {total} chunks were not stored; what was produced is on stdout"
        ))),
        (failed, not_stored) => Err(CommandError::Failed(format!(
            "{failed} of {total} chunks failed and {not_stored} were not stored; \
             what succeeded is on stdout"
        ))),
    }
}

/// The Artifacts a run produced, in input order. A chunk that failed has none,
/// and its absence must not shift the ones that follow it.
fn collected(results: &[Option<Artifact>]) -> Vec<Artifact> {
    results.iter().flatten().cloned().collect()
}

/// The batch's documents or array, or the one Artifact a single-chunk run
/// produced.
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

/// One planned chunk: the stored answer when there already is one, and — when the
/// batch says the same thing twice — the earlier turn this one repeats.
///
/// `PlannedTurn` rather than `Turn`: the plan is a surface-side row, and
/// [`plainly_core::Turn`] is the value the runner returns for a turn that ran.
struct PlannedTurn {
    stored: Option<Record>,
    /// The index of the earlier turn with the same Lookup Key. `None` for the
    /// first occurrence, and for every chunk of a run that has no repeats.
    repeat_of: Option<usize>,
}

/// How a run that could not be opened is reported.
///
/// A store that will not open is local state and a failure, a provider that does
/// not resolve is "not configured", and a credential failure keeps the
/// distinction that sends the user to the right place: a missing key means "set
/// one" (exit 3), and a store that cannot answer means something is broken
/// (exit 1).
fn runner_error(error: RunnerError) -> CommandError {
    match error {
        RunnerError::Config(error) => CommandError::from(error),
        RunnerError::Store(error) => CommandError::from(error),
        RunnerError::Setup(error) => CommandError::NotConfigured(error.to_string()),
        RunnerError::Paths(error) => CommandError::Failed(error.to_string()),
        // Core states the fact; the instruction to set a key is this surface's
        // (ADR 0003).
        RunnerError::Credential(error) => credential_error(error),
    }
}

/// Say the run-level notices this surface has not said yet.
///
/// They are facts about the machine (a credential store that could not answer),
/// not about a chunk, so each is said once — after opening, and again after a
/// turn only if a `Bypass` provider added one. The count is how "once" is kept
/// without the runner having to remember what a surface printed.
fn report_notices(runner: &Runner, printed: &mut usize) {
    for notice in &runner.notices()[*printed..] {
        match notice {
            RunNotice::KeyringUnavailable { provider, reason } => eprintln!(
                "plainly: cannot read the credential store for {provider}: {reason}; \
                 continuing without a key, since {provider} may not need one"
            ),
        }
    }
    *printed = runner.notices().len();
}

/// `request` or `requests`, so a count of one does not read as a bug.
fn request_word(count: usize) -> &'static str {
    if count == 1 { "request" } else { "requests" }
}

/// What a reuse says, not just that it reused something: which provider, model
/// and Level the stored Explanation came from, which the user cannot otherwise
/// tell. The contract tier of the run that produced it is not claimed, because a
/// Record does not carry one (spec §9) and guessing would describe a request that
/// was never made.
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
/// Every fact here is one the run established: the tier the request was actually
/// held to, a correction the endpoint forced, and — when there is one — the
/// reason nothing could be probed at all. The tier is said every time, because it
/// is the promise the answer comes with (spec §7).
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

    // "The endpoint enforces the contract" and "we check the answer ourselves
    // and ask again" are not the same promise (spec §7).
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

/// The product, on stdout.
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

/// The product of a batch.
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

/// A Passage from the named file, or the usage error for a path that cannot be
/// read: configuration has nothing to say about a path that is not there.
fn read_file(path: &Path) -> Result<String, CommandError> {
    let passage = std::fs::read_to_string(path)
        .map_err(|error| CommandError::Usage(format!("cannot read {}: {error}", path.display())))?;

    passage_or_usage(passage, NO_PASSAGE)
}

fn read_stdin() -> Result<String, CommandError> {
    let mut passage = String::new();
    std::io::stdin()
        .lock()
        .read_to_string(&mut passage)
        .map_err(|error| CommandError::Failed(format!("cannot read stdin: {error}")))?;

    passage_or_usage(passage, NO_PASSAGE)
}

/// The clipboard's Passage, or the refusal its owner asked for.
///
/// The judgement is core's (spec §11); this only words it and turns it into an
/// exit code. Nothing has been configured, opened or sent at this point, so a
/// refusal has nothing to undo.
fn read_clipboard() -> Result<String, CommandError> {
    let clipboard = crate::clipboard::read().map_err(|error| match error {
        crate::clipboard::Error::Empty | crate::clipboard::Error::NotText => {
            CommandError::Usage(format!("there is no Passage: {error}"))
        }
        error => CommandError::Failed(error.to_string()),
    })?;

    match clipboard.reading() {
        // The refusal says both halves of what happened: why, and that the
        // content went nowhere. There is deliberately no "explain it anyway".
        Reading::Sensitive => Err(CommandError::Refused(format!(
            "the clipboard is marked sensitive ({PASSWORD_HINT} = secret): \
             nothing was sent and nothing was stored"
        ))),
        // The marker was published and its value could not be read, so the run
        // cannot tell whether it is secret. It refused on purpose, which is what
        // code 4 is for, and the message names the half that is missing rather
        // than letting the content look public.
        Reading::Unread => Err(CommandError::Refused(format!(
            "the clipboard publishes {PASSWORD_HINT} but its value could not be read: \
             nothing was sent and nothing was stored"
        ))),
        Reading::Passage(passage) => {
            passage_or_usage(passage, "there is no Passage: the clipboard is empty")
        }
    }
}

/// Whitespace is not a Passage, whoever handed it over.
///
/// The wording is the caller's, because an empty pipe and an empty clipboard are
/// different mistakes to the person who made one; the rule is one rule.
fn passage_or_usage(passage: String, empty: &str) -> Result<String, CommandError> {
    if passage.trim().is_empty() {
        return Err(CommandError::Usage(empty.to_string()));
    }
    Ok(passage)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    File(PathBuf),
    Stdin,
    Clipboard,
}

/// Decide the source before reading anything.
///
/// The terminal case is the one that matters: with no file and a terminal on
/// stdin, reading would wait for a pipe that is never coming, which the person
/// at the keyboard experiences as a hang rather than as a mistake. Naming two
/// sources at once is a mistake in the asking: neither silently wins.
fn source(
    file: Option<&Path>,
    clipboard: bool,
    stdin_is_terminal: bool,
) -> Result<Source, CommandError> {
    match (file, clipboard) {
        (Some(_), true) => Err(CommandError::Usage(
            "name a file or --clipboard, not both".to_string(),
        )),
        (Some(path), false) => Ok(Source::File(path.to_path_buf())),
        (None, true) => Ok(Source::Clipboard),
        (None, false) if stdin_is_terminal => Err(CommandError::Usage(NO_PASSAGE.to_string())),
        (None, false) => Ok(Source::Stdin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bare invocation at a terminal must not wait for input that is not
    /// coming.
    #[test]
    fn a_terminal_with_no_file_is_a_usage_error_rather_than_a_wait() {
        let error = source(None, false, true).unwrap_err();

        assert!(matches!(error, CommandError::Usage(_)), "got {error:?}");
        assert!(error.to_string().contains("pipe one in"), "{error}");
    }

    #[test]
    fn a_pipe_is_read_when_no_file_is_named() {
        assert_eq!(source(None, false, false).unwrap(), Source::Stdin);
    }

    /// A named file is read even when the command happens to be run from a
    /// terminal, where stdin is a terminal too.
    #[test]
    fn a_named_file_wins_over_the_terminal() {
        assert_eq!(
            source(Some(Path::new("passage.md")), false, true).unwrap(),
            Source::File(PathBuf::from("passage.md"))
        );
    }

    /// `--clipboard` is a source of its own, and it does not need a pipe or a
    /// terminal to decide anything.
    #[test]
    fn the_clipboard_is_a_source_of_its_own() {
        assert_eq!(
            source(None, true, true).unwrap(),
            Source::Clipboard,
            "a terminal does not matter when the clipboard was named"
        );
    }

    /// Two sources at once is a mistake in the asking rather than a rule about
    /// which one wins.
    #[test]
    fn a_file_and_the_clipboard_at_once_is_a_usage_error() {
        let error = source(Some(Path::new("passage.md")), true, false).unwrap_err();

        assert!(matches!(error, CommandError::Usage(_)), "got {error:?}");
        assert!(error.to_string().contains("not both"), "{error}");
    }
}
