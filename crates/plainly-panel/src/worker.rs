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

/// What came of one Passage.
pub enum Outcome {
    /// It has an Explanation, from the cache or from the provider.
    /// Boxed: an Artifact dwarfs the failure beside it, and this crosses a
    /// channel per Passage.
    Explained(Box<Artifact>),
    /// It has none, and this says why.
    Failed(String),
}

/// One thing that happened to one Passage.
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
pub fn start(passages: Vec<String>, sender: Sender<Event>) {
    std::thread::spawn(move || {
        let send = |event: Event| {
            let _ = sender.send_blocking(event);
        };

        if let Err(message) = run(&passages, &send) {
            send(Event::Failed(message));
        }
    });
}

/// The whole run: everything that can fail before the first Passage is a failure
/// of the run rather than of a chunk.
fn run(passages: &[String], send: &dyn Fn(Event)) -> Result<(), String> {
    let paths = Paths::discover().map_err(|error| error.to_string())?;
    let config = ConfigFile::open(paths.config_file())
        .map_err(|error| error.to_string())?
        .config()
        .map_err(|error| error.to_string())?;
    let setup =
        ProviderSetup::resolve(&config.app.provider, &config).map_err(|error| error.to_string())?;

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
        // answered from the row that already holds it.
        let outcome = match store.recall(&lookup, now) {
            Ok(Some(record)) => Outcome::Explained(Box::new(record.artifact)),
            Ok(None) => {
                match explain::run(&setup, &request, &transport, &capabilities, now, &pause) {
                    Ok(run) => match store.remember(&lookup, &run.artifact) {
                        Ok(record) => Outcome::Explained(Box::new(record.artifact)),
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
