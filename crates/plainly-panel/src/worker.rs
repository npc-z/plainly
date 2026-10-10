//! Generating the Explanations, off the main loop.
//!
//! One thread may touch GTK widgets, so the run happens on another: the
//! configuration, the history store (which is also the cache), the provider, and
//! the register of what came of each Passage. The judgements are all core's —
//! `Lookup`, `Store::recall`/`remember`, `explain::run` — and this only sequences
//! them, which is what tickets/08 hands to the panel.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_channel::Sender;
use plainly_core::{
    Artifact, Cache, ChatCompletions, ConfigFile, Endpoint, Failure, KeyRequirement, Lookup, Paths,
    Prompt, ProviderSetup, Secrets, Store, Timestamp, explain,
};

use crate::provenance::Provenance;

/// What a trigger tells the run about the cache, and about which provider.
///
/// Two facts in one value because they travel together and only ever in one
/// combination: a key press reuses the cache and the configured provider, and ⟳
/// bypasses the cache — optionally on the cloud provider the note under a local
/// model promises (spec §12).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Recall {
    /// A key press: re-read the clipboard, and let the Lookup Key answer an
    /// unchanged Passage from the history store.
    #[default]
    Reuse,
    /// ⟳: ask the provider again and overwrite what the store holds. `provider`
    /// names the one to ask when it is not the configured one — the cloud move.
    Bypass { provider: Option<String> },
}

/// What came of one Passage.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// It has an Explanation, from the cache or from the provider, and the
    /// provenance of the answer that is actually in hand.
    Explained {
        artifact: Box<Artifact>,
        provenance: Provenance,
    },
    /// It has none, and this says why.
    Failed(String),
}

/// One thing that happened to one Passage.
#[derive(Debug, Clone)]
pub enum Event {
    /// A Passage's turn came out.
    Chunk { index: usize, outcome: Outcome },
    /// The run could not start at all: configuration, key or store.
    Failed(String),
}

/// Explain the Passages in turn, reporting each one as it lands.
///
/// The channel belongs to one trigger: the surface that started this run ignores
/// whatever arrives after a later trigger has taken over.
pub fn start(passages: Vec<String>, recall: Recall, sender: Sender<Event>) {
    std::thread::spawn(move || {
        let send = |event: Event| {
            let _ = sender.send_blocking(event);
        };

        if let Err(message) = run(&passages, recall, &send) {
            send(Event::Failed(message));
        }
    });
}

/// The whole run: everything that can fail before the first Passage is a failure
/// of the run rather than of a chunk.
fn run(passages: &[String], recall: Recall, send: &dyn Fn(Event)) -> Result<(), String> {
    let paths = Paths::discover().map_err(|error| error.to_string())?;
    let config = ConfigFile::open(paths.config_file())
        .map_err(|error| error.to_string())?
        .config()
        .map_err(|error| error.to_string())?;
    // ⟳ may name a provider to move to — the cloud alternative the note under a
    // local model promises (spec §12). Everything else resolves the configured
    // one, and the choice travels as data from the panel rather than being made
    // again here: which cloud provider is usable is a settings question.
    let name = match &recall {
        Recall::Reuse => config.app.provider.clone(),
        Recall::Bypass { provider } => provider
            .clone()
            .unwrap_or_else(|| config.app.provider.clone()),
    };
    let setup = ProviderSetup::resolve(&name, &config).map_err(|error| error.to_string())?;

    // The credential rule is core's and is the same one the CLI applies: a tier
    // that fails is an error, unless the provider may authenticate nothing at
    // all, which is what a local runtime on a headless box looks like.
    let key = match Secrets::from_process_env().resolve(&setup.name) {
        Ok(key) => key.map(|resolved| resolved.secret),
        Err(_) if setup.key == KeyRequirement::Optional => None,
        Err(error) => return Err(error.to_string()),
    };

    let prompt = Prompt::from_config(&config.prompts);
    let mut store = Store::open(paths.history_file()).map_err(|error| error.to_string())?;
    let capabilities = Cache::new(paths.cache_dir().join("providers"));
    let now = now()?;

    // Whether the endpoint that answered — or will answer — is on this machine.
    // It is a fact about the endpoint rather than about the provider's name, so
    // it is asked once, of the setup the request is actually sent to.
    let local = setup.is_local();

    let transport = |setup: &ProviderSetup| -> Box<dyn Endpoint> {
        Box::new(ChatCompletions::new(
            setup.clone(),
            key.clone().unwrap_or_default(),
        ))
    };
    let pause = |wait: Duration| std::thread::sleep(wait);

    for (index, passage) in passages.iter().enumerate() {
        let request = explain::request(passage, &config, &setup, &prompt);
        let lookup = Lookup::from(&request).key();

        // The history store doubles as the cache (spec §9): the same question is
        // answered from the row that already holds it — unless this trigger is a
        // regeneration, whose whole meaning is to ask again.
        let recalled = match recall {
            Recall::Reuse => store.recall(&lookup, now),
            Recall::Bypass { .. } => Ok(None),
        };

        let outcome = match recalled {
            Ok(Some(record)) => Outcome::Explained {
                provenance: Provenance::of(&record.artifact, local),
                artifact: Box::new(record.artifact),
            },
            Ok(None) => {
                match explain::run(&setup, &request, &transport, &capabilities, now, &pause) {
                    Ok(run) => match store.remember(&lookup, &run.artifact) {
                        Ok(record) => Outcome::Explained {
                            provenance: Provenance::of(&record.artifact, local),
                            artifact: Box::new(record.artifact),
                        },
                        Err(error) => Outcome::Failed(error.to_string()),
                    },
                    Err(failure) => Outcome::Failed(describe(&failure.failure)),
                }
            }
            Err(error) => Outcome::Failed(error.to_string()),
        };

        send(Event::Chunk { index, outcome });
    }

    Ok(())
}

/// What a person is told when the policy gives up: the reason, and whether
/// anything was retried. The surface adds "nothing was stored" itself.
fn describe(failure: &Failure) -> String {
    if failure.retried() {
        format!("{}（重试了 {} 次）", failure.reason, failure.attempts)
    } else {
        failure.reason.clone()
    }
}

/// The current instant, for stamping an Artifact.
///
/// Core deliberately has no `now()`: the explain path takes the instant as a
/// value, so reading the clock belongs to the surface that can say something
/// about a clock outside the range Plainly can store.
fn now() -> Result<Timestamp, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("the system clock is before 1970: {error}"))?
        .as_secs();

    i64::try_from(seconds)
        .ok()
        .and_then(Timestamp::from_unix_seconds)
        .ok_or_else(|| "the system clock is outside the range Plainly can stamp".to_string())
}
