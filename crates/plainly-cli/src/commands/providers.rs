//! `plainly providers`: what Plainly is talking to, what that endpoint takes,
//! what is running locally, and the API keys.
//!
//! The status report goes to stdout because it *is* the product of the command —
//! the same reason `config show` prints there — while progress and complaints go
//! to stderr. The same holds for the local listing: it is what the command was
//! asked for, so it is the product.
//!
//! `probe`, `discover` and `use` touch the network; nothing else does. Reporting
//! reads the cache a probe wrote and says "not probed yet" when there is none: a
//! display that reaches out to an endpoint would turn `plainly providers` into a
//! request nobody asked for.

use std::cell::Cell;
use std::io::Read;

use plainly_core::setup::{is_loopback, same_service};
use plainly_core::{
    Capability, ChatCompletions, Cleared, Config, ConfigFile, EndpointModel, KeyRequirement,
    LocalRuntime, ModelLister, ModelReader, Origin, Paths, ProviderError, ProviderErrorKind,
    ProviderSetup, Secrets, Stored, Surface, Thinking, ThinkingSwitch, context_fits, discover,
    env_var_name, min_context_length, presets, probe,
};

use crate::cli::{KeyCommand, ProviderCommand};
use crate::commands::{CommandError, cache, now, resolve_key, tier};
use crate::exit;

pub fn run(command: Option<ProviderCommand>) -> Result<u8, CommandError> {
    match command {
        // `plainly providers` on its own is the report, not a usage error.
        None => status(),
        Some(ProviderCommand::Probe { name }) => probe_one(name),
        Some(ProviderCommand::Discover) => discover_local(),
        Some(ProviderCommand::Use { provider, model }) => use_model(&provider, &model),
        Some(ProviderCommand::Key { command }) => match command {
            KeyCommand::Set { name } => store_key(&name),
            KeyCommand::Clear { name } => clear_key(&name),
        },
    }
}

/// Discovery's transport: the model reader, plus whatever credential the name a
/// candidate would be configured under resolves to.
///
/// A loopback runtime may authenticate — spec §7 asks the llama.cpp sidecar to
/// carry `--api-key`, and `#29462`'s unauthenticated OOM path is the reason — so
/// a scan without a key would report the project's own recommended setup as
/// missing. The key is resolved exactly the way a run resolves it (environment,
/// then keyring, then this session), and a name with no key sends none.
struct CandidateReader {
    reader: ModelReader,
    secrets: Secrets,
    /// Whether a credential tier that could not answer has been reported. A scan
    /// asks several names, and one broken keyring is one fact, not one per port.
    reported: Cell<bool>,
}

impl ModelLister for CandidateReader {
    fn models_at(
        &self,
        endpoint: &str,
        provider: &str,
    ) -> Result<Vec<EndpointModel>, ProviderError> {
        let key = candidate_key(&self.secrets, provider, &self.reported);

        self.reader.models(endpoint, key.as_deref())
    }
}

/// The key to send for one candidate's name, and the single warning a credential
/// tier that cannot answer earns.
///
/// A tier that fails is an error rather than a skip ([`Secrets`]), and the whole
/// point of saying so is that a scan which finds nothing must not be how a broken
/// keyring is discovered: `discover` reporting "no local runtime answered" while
/// the keyring was the reason is exactly the kind of lie this command exists not
/// to tell. The scan does go on without the key — most loopback runtimes need
/// none, and refusing to look would be worse than looking unauthenticated.
fn candidate_key(secrets: &Secrets, provider: &str, reported: &Cell<bool>) -> Option<String> {
    match secrets.resolve(provider) {
        Ok(key) => key.map(|key| key.secret),
        Err(error) => {
            if report_once(reported) {
                eprintln!("plainly: {error}; looking for local runtimes without a key");
            }
            None
        }
    }
}

/// Whether a failure is the one to report: the first of a scan, and no later one.
///
/// Its own function because "once" is the claim worth testing: the five names a
/// scan asks share one keyring, and five copies of one failure is noise that
/// teaches the reader to skip the line.
fn report_once(reported: &Cell<bool>) -> bool {
    !reported.replace(true)
}

/// The configured provider and what is known about its endpoint, from the cache.
fn status() -> Result<u8, CommandError> {
    let paths = Paths::discover().map_err(|error| CommandError::Failed(error.to_string()))?;
    let config = ConfigFile::open(paths.config_file())?.config()?;
    let setup = configured(&config)?;

    let known = cache(&paths).load(&setup);
    print_report(&setup, known.as_ref());

    Ok(exit::SUCCESS)
}

/// Ask the endpoint again, ignoring whatever the cache says, and keep the answer.
fn probe_one(name: Option<String>) -> Result<u8, CommandError> {
    let paths = Paths::discover().map_err(|error| CommandError::Failed(error.to_string()))?;
    let config = ConfigFile::open(paths.config_file())?.config()?;
    let setup = match name {
        Some(name) => ProviderSetup::resolve(&name, &config)
            .map_err(|error| CommandError::NotConfigured(error.to_string()))?,
        None => configured(&config)?,
    };

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

    eprintln!("plainly: probing {} at {}", setup.label, setup.endpoint);
    let client = ChatCompletions::new(setup.clone(), key.unwrap_or_default());
    let found = probe::reprobe(&client, &setup, &cache(&paths), now()?);
    let active = setup.with_capability(found.capability.schema, found.capability.thinking);

    match found.origin {
        // The endpoint answered, so the report can say what it takes.
        Origin::Probed => print_report(&active, Some(&found.capability)),
        // Nothing was learned: the report is the unprobed one, carrying the
        // cautious shape a run would use, and the reason for the failure goes to
        // stderr with the exit code rather than into the report as a conclusion.
        Origin::Cached | Origin::Assumed => {
            print_report(&active, None);
            if let Some(reason) = &found.unanswered {
                eprintln!("plainly: {reason}");
            }
        }
    }
    if let Some(reason) = &found.unwritten {
        eprintln!("plainly: note: the probe result could not be cached: {reason}");
    }

    // A probe that could not reach the endpoint is a provider failure, and a
    // script asking "is this endpoint usable?" wants to hear about it. The
    // report still goes out first: it says which provider, at which endpoint,
    // and what a run would assume instead.
    match found.origin {
        Origin::Probed => Ok(exit::SUCCESS),
        Origin::Cached | Origin::Assumed => Ok(exit::FAILURE),
    }
}

/// The provider `[app]` names, resolved.
fn configured(config: &Config) -> Result<ProviderSetup, CommandError> {
    ProviderSetup::resolve(&config.app.provider, config)
        .map_err(|error| CommandError::NotConfigured(error.to_string()))
}

/// `plainly providers discover`: what is listening on the common ports, and what
/// each runtime says it serves.
///
/// Nothing found is a *configuration* answer rather than a crash: the message
/// says where Plainly looked and the two ways to fix it — start a runtime there,
/// or name the endpoint — because guessing a port is exactly what this command
/// exists not to do (tickets/06).
fn discover_local() -> Result<u8, CommandError> {
    let paths = Paths::discover().map_err(|error| CommandError::Failed(error.to_string()))?;
    let config = ConfigFile::open(paths.config_file())?.config()?;

    let candidates = discover::candidates(&config);
    // The listing is the product, so it goes to stdout, but a scan of several
    // ports can take a moment on a port that accepts a connection and says
    // nothing: announcing it is the difference between waiting and a hang.
    announce_scan(&candidates);
    let scan = discover::scan(&reader(), &candidates);

    if scan.runtimes.is_empty() {
        return Err(CommandError::NotConfigured(nothing_found(
            "no local runtime answered at",
            &candidates,
            &scan.unanswered,
            "Start one there, or point Plainly at it with \
             `plainly config set providers.<name>.endpoint <url>`",
        )));
    }

    // A runtime that refused us is worth saying even when others answered: it is
    // there, and what it wants is not "start one".
    for refusal in refusals(&scan.unanswered) {
        eprintln!("plainly: {refusal}");
    }
    print_runtimes(&scan.runtimes, &config);
    Ok(exit::SUCCESS)
}

/// The scan's transport, with the process's credentials for the names it asks
/// about.
fn reader() -> CandidateReader {
    CandidateReader {
        reader: ModelReader::new(),
        secrets: Secrets::from_process_env(),
        reported: Cell::new(false),
    }
}

/// `plainly providers use <provider> <model>`: the model a local runtime serves,
/// written into that provider's section and made the provider a run uses.
///
/// The endpoint decides whether this is a local selection at all — "use" claims
/// the runtime in front of you serves the model, and a hosted provider has no
/// runtime in front of you — and the model is held to what the runtime lists,
/// because a model id nobody serves is a configuration that fails at the first
/// Passage instead of here.
fn use_model(provider: &str, model: &str) -> Result<u8, CommandError> {
    let paths = Paths::discover().map_err(|error| CommandError::Failed(error.to_string()))?;
    let mut file = ConfigFile::open(paths.config_file())?;
    let config = file.config()?;

    // Where the name points today: the user's own endpoint, or the preset's.
    // Resolved before a model exists, which is the whole point of asking.
    let endpoint = ProviderSetup::endpoint_of(provider, &config)
        .map_err(|error| CommandError::NotConfigured(error.to_string()))?;
    if !is_loopback(&endpoint) {
        return Err(CommandError::NotConfigured(format!(
            "providers.{provider}.endpoint is {endpoint}, which is not on this machine. \
             `providers use` selects a model of a local runtime; set a hosted model with \
             `plainly config set providers.{provider}.model {model}`."
        )));
    }

    let candidates = discover::candidates(&config);
    announce_scan(&candidates);
    let scan = discover::scan(&reader(), &candidates);
    let found = &scan.runtimes;

    // Which runtime the choice is about — and, when the name's own runtime does
    // not have the model, which one does. The rule is discovery's; the wording of
    // each outcome is here.
    let runtime = match discover::select(
        found,
        provider,
        model,
        &endpoint,
        endpoint_configured(&config, provider),
    ) {
        discover::Selection::Use(runtime) => runtime,
        discover::Selection::Silent(other) => {
            return Err(CommandError::NotConfigured(format!(
                "providers.{provider}.endpoint is {endpoint}, and it did not answer; \
                 a {provider} runtime did answer at {}.\n\
                 Point the provider at it with \
                 `plainly config set providers.{provider}.endpoint {}` if that is the one you meant",
                other.endpoint, other.endpoint,
            )));
        }
        discover::Selection::Elsewhere(other) => {
            return Err(CommandError::NotConfigured(format!(
                "providers.{provider}.endpoint is {endpoint}, and it does not serve {model:?}; \
                 a {provider} runtime at {} does.\n\
                 Point the provider at it with \
                 `plainly config set providers.{provider}.endpoint {}` if that is the one you meant",
                other.endpoint, other.endpoint,
            )));
        }
        discover::Selection::Unserved(named) => {
            return Err(CommandError::Usage(format!(
                "{} does not serve {model:?}: {}",
                named.endpoint,
                listed(&named.models)
            )));
        }
        discover::Selection::Unanswered => {
            return Err(CommandError::NotConfigured(nothing_found(
                &format!("no local runtime named {provider:?} answered at"),
                &candidates,
                &scan.unanswered,
                &format!(
                    "Start it there, or point Plainly at it with \
                     `plainly config set providers.{provider}.endpoint <url>`"
                ),
            )));
        }
    };

    // The listing flags a context too small for one Passage, and the choice is
    // where that flag has to be impossible to miss. It is a note rather than a
    // refusal: a runtime may serve more context than its list reports.
    let thinking = thinking_of(&config, provider);
    if let Some(entry) = runtime.model(model) {
        if let (Some(false), Some(length)) = (context_fits(entry, thinking), entry.context_length) {
            eprintln!(
                "plainly: note: {model} reports a context of {length}, below the {} one Passage \
                 needs at thinking {}; {}",
                min_context_length(thinking),
                thinking.as_str(),
                context_hint(provider),
            );
        }
    }

    // The choice, in the two places that make it real: the profile the run is
    // built from, and the provider `[app]` names. The endpoint is written only
    // when discovery found the runtime at a different service than the name
    // already pointed at — the name's own endpoint may be the preset's guess,
    // and a runtime it did not describe is written out in full. An unchanged
    // endpoint stays as the user spelled it, under whatever comments they keep.
    file.set("app.provider", provider)?;
    file.set(&format!("providers.{provider}.model"), model)?;
    if !same_service(&endpoint, &runtime.endpoint) {
        file.set(&format!("providers.{provider}.endpoint"), &runtime.endpoint)?;
    }
    file.save()?;

    eprintln!(
        "using {provider} {model} at {} (written to {})",
        runtime.endpoint,
        file.path().display()
    );
    Ok(exit::SUCCESS)
}

/// The listing a person reads before choosing: which runtime, where, and per
/// model what the runtime says about it.
///
/// The context flag is about the setting this provider is configured for — the
/// detailed mode's budget is several times the default's, so the same model can
/// fit one and not the other — and the configuration is what says which.
fn print_runtimes(runtimes: &[LocalRuntime], config: &Config) {
    println!("local runtimes:");
    for runtime in runtimes {
        let thinking = thinking_of(config, &runtime.provider);
        println!();
        println!("{} at {}", runtime.provider, runtime.endpoint);
        if runtime.models.is_empty() {
            println!("  no models listed");
            continue;
        }
        for model in &runtime.models {
            println!(
                "  {}  {}  {}",
                model.id,
                loaded_label(model.loaded),
                context_label(model, thinking),
            );
        }
    }
    println!();
    println!("choose one with: plainly providers use <provider> <model>");
}

/// The reasoning setting a run with `provider` would use: the user's, or the
/// default when the provider has no table yet.
fn thinking_of(config: &Config, provider: &str) -> Thinking {
    config
        .providers
        .get(provider)
        .map(|profile| profile.thinking)
        .unwrap_or_default()
}

/// What the runtime said about whether a model is loaded, in the two words the
/// spec uses and one for not having said.
fn loaded_label(loaded: Option<bool>) -> &'static str {
    match loaded {
        Some(true) => "loaded",
        Some(false) => "unloaded",
        None => "state not reported",
    }
}

/// A model's context, and — the reason discovery reads the list at all — whether
/// it can hold one Passage without ever loading the model (tickets/06).
fn context_label(model: &EndpointModel, thinking: Thinking) -> String {
    let Some(length) = model.context_length else {
        return "context not reported".to_string();
    };

    match context_fits(model, thinking) {
        Some(true) => format!("context {length}"),
        _ => format!(
            "context {length} — too small for one Passage at thinking {}",
            thinking.as_str()
        ),
    }
}

/// Where a runtime's context is raised, when it reports one too small.
///
/// Ollama has no per-request spelling: `num_ctx` lives in the model's Modelfile
/// or in `OLLAMA_CONTEXT_LENGTH`, so pointing at the environment beats advising a
/// flag it does not take (spec §7). Everywhere else the knob is the runtime's own.
fn context_hint(provider: &str) -> &'static str {
    match presets::preset(provider).map(|preset| preset.surface) {
        Some(Surface::Ollama) => {
            "raise OLLAMA_CONTEXT_LENGTH (or the model's Modelfile) if answers are cut off"
        }
        _ => "raise it on the runtime's side if answers are cut off",
    }
}

/// The models a runtime lists, for a message that has to say what *can* be
/// chosen — capped, because a runtime with a large catalogue would otherwise be
/// pasted into a terminal in full.
fn listed(models: &[EndpointModel]) -> String {
    if models.is_empty() {
        return "it lists no models".to_string();
    }

    let ids: Vec<&str> = models
        .iter()
        .take(LISTED_LIMIT)
        .map(|model| model.id.as_str())
        .collect();
    let more = models.len().saturating_sub(LISTED_LIMIT);

    match more {
        0 => format!("it lists {}", ids.join(", ")),
        _ => format!("it lists {} and {more} more", ids.join(", ")),
    }
}

/// How many model ids a "what it does serve" message names before it stops.
const LISTED_LIMIT: usize = 10;

/// Whether the user wrote an endpoint for this name, as opposed to the preset's
/// own being what a request would go to.
///
/// The distinction decides what a selection may do: a name the user pointed
/// somewhere is theirs, and a name that only carries a preset's guess is
/// discovery's to fill in (tickets/06).
fn endpoint_configured(config: &Config, provider: &str) -> bool {
    config
        .providers
        .get(provider)
        .and_then(|profile| profile.endpoint.as_deref())
        .is_some_and(|endpoint| !endpoint.trim().is_empty())
}

/// Where Plainly looked, for a message about finding nothing.
fn endpoints(candidates: &[discover::Candidate]) -> String {
    candidates
        .iter()
        .map(|candidate| candidate.endpoint.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a "nothing found" report says: where Plainly looked, every endpoint that
/// answered and would not serve us, and how to point Plainly at a runtime.
///
/// The middle part is why this is not one line. "No local runtime answered" is
/// true of a port with nothing on it and false of one that answered 401 — and the
/// fix for the second (a key) is not the fix for the first (start one), so a
/// report that offers only the first cannot be acted on.
fn nothing_found(
    opening: &str,
    candidates: &[discover::Candidate],
    unanswered: &[discover::Unanswered],
    closing: &str,
) -> String {
    let mut message = format!("{opening} {}.", endpoints(candidates));
    for refusal in refusals(unanswered) {
        message.push_str(&format!("\n  {refusal}"));
    }
    message.push_str(&format!("\n{closing}"));
    message
}

/// The failures worth naming: the endpoint answered and the answer was not a
/// model list.
///
/// A refused connection, a timeout or a 5xx is the ordinary "nothing usable
/// there" the rest of the report already covers. Anything else means something is
/// listening: a key it wants, a URL that is not a model server, a body that is
/// not JSON. The reason is quoted as the endpoint gave it, and the key is named
/// because that is what a loopback runtime asks for when it asks for anything
/// (spec §7's sidecar carries `--api-key`).
fn refusals(unanswered: &[discover::Unanswered]) -> Vec<String> {
    unanswered
        .iter()
        .filter(|candidate| candidate.error.kind() != ProviderErrorKind::Unavailable)
        .map(|candidate| match candidate.error.status() {
            // The one refusal with a single fix, and the one a loopback runtime
            // actually gives (spec §7's sidecar carries `--api-key`).
            Some(401 | 403) => format!(
                "{} — set {}",
                candidate.error,
                env_var_name(&candidate.provider),
            ),
            // Anything else answered with something that is not a model list:
            // the reason is the whole advice, and it already names the endpoint.
            _ => candidate.error.to_string(),
        })
        .collect()
}

/// Say where a scan is about to look, on stderr, before it spends its timeouts.
fn announce_scan(candidates: &[discover::Candidate]) {
    eprintln!(
        "plainly: looking for local runtimes at {}",
        endpoints(candidates)
    );
}

/// The report a person reads: who, where, what model, which thinking, and what
/// the endpoint is known to take.
///
/// `known` is `None` when nothing has been probed yet, which the contract line
/// says in as many words rather than showing a capability nobody established.
fn print_report(setup: &ProviderSetup, known: Option<&Capability>) {
    // A report says what a run would do, so a known capability is folded in
    // before anything is printed: showing the preset's starting guess next to a
    // probe's conclusion would describe a run that never happens.
    let active = match known {
        Some(capability) => setup.with_capability(capability.schema, capability.thinking),
        None => setup.clone(),
    };

    println!("provider   {} ({})", active.name, active.label);
    println!("endpoint   {}", active.endpoint);
    println!("model      {}", active.model);
    println!(
        "thinking   {}, switch {}",
        active.thinking.as_str(),
        match active.thinking_switch {
            ThinkingSwitch::Canonical => "canonical",
            ThinkingSwitch::Unsupported => "none known",
        }
    );

    match known {
        Some(capability) => {
            println!(
                "contract   {} — probed {}",
                tier(capability.schema),
                capability.probed_at.to_rfc3339()
            );
            match &capability.models {
                None => println!("models     could not be listed"),
                Some(models) if models.is_empty() => println!("models     none listed"),
                Some(models) => {
                    let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
                    println!("models     {}: {}", ids.len(), ids.join(", "));
                }
            }
        }
        None => {
            println!("contract   not probed yet — run `plainly providers probe`");
            println!("           until then: {}", tier(active.schema));
        }
    }
}

fn store_key(name: &str) -> Result<u8, CommandError> {
    let secret = read_secret()?;
    let secrets = Secrets::from_process_env();

    // A key in the environment outranks anything we store. Say so, rather than
    // letting the user believe the keyring now decides. This checks the
    // environment tier only: no keyring round trip for a warning.
    if secrets.environment_provides(name) {
        eprintln!(
            "note: {} is set and takes precedence over the keyring",
            env_var_name(name)
        );
    }

    match secrets.store(name, &secret) {
        Stored::Keyring => {
            eprintln!("stored the {name} key in the OS keyring");
            Ok(exit::SUCCESS)
        }
        Stored::Session { warning } => {
            // Nothing was persisted, and this process is about to exit. For a
            // one-shot command, reporting success here would be a lie.
            Err(CommandError::Failed(warning))
        }
    }
}

fn clear_key(name: &str) -> Result<u8, CommandError> {
    let secrets = Secrets::from_process_env();
    let cleared = secrets
        .clear(name)
        .map_err(|error| CommandError::Failed(error.to_string()))?;

    match cleared {
        Cleared::Keyring => {
            eprintln!("cleared the {name} key from the OS keyring");
            Ok(exit::SUCCESS)
        }
        Cleared::Nothing => {
            eprintln!(
                "no OS keyring in use, so nothing was stored persistently. \
                 Unset {} if it is set.",
                env_var_name(name)
            );
            Ok(exit::SUCCESS)
        }
    }
}

/// A key arrives on stdin, never in argv: an argument would land in shell
/// history and in `ps` output.
///
/// An API key is a single line, and this reads all of stdin rather than one
/// line so that a second line is an error instead of being silently dropped.
/// Storing the first line of a PEM block and calling it a key would fail later,
/// at authentication, with nothing to point at.
fn read_secret() -> Result<String, CommandError> {
    let mut input = String::new();
    std::io::stdin()
        .lock()
        .read_to_string(&mut input)
        .map_err(|error| {
            CommandError::Failed(format!("cannot read the key from stdin: {error}"))
        })?;

    let secret = input.trim_end_matches(['\n', '\r']);
    if secret.is_empty() {
        return Err(CommandError::Usage(
            "no key on stdin; pipe it in, for example: \
             printf %s \"$KEY\" | plainly providers key set deepseek"
                .to_string(),
        ));
    }
    if secret.contains(['\n', '\r']) {
        return Err(CommandError::Usage(
            "the key spans more than one line, and a secret is a single line. \
             Pipe the key alone, or use PLAINLY_<PROVIDER>_API_KEY for a value \
             that is not one line."
                .to_string(),
        ));
    }
    Ok(secret.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use plainly_core::{EnvSecrets, SecretError, SecretStore};

    /// A keyring that is present but cannot answer: a headless box with a session
    /// bus and no Secret Service behind it, or a locked collection.
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

    /// A tier that cannot answer is a failure to report, not a silent "no key":
    /// swallowing it is how a broken keyring turns into "no local runtime
    /// answered" (tickets/06). The scan still goes on, because a loopback runtime
    /// usually needs no key at all.
    #[test]
    fn a_keyring_that_cannot_answer_is_reported_and_the_scan_goes_on() {
        let secrets = Secrets::with_stores(EnvSecrets::empty(), Some(Box::new(BrokenKeyring)));
        let reported = Cell::new(false);

        assert_eq!(candidate_key(&secrets, "llamacpp", &reported), None);
        assert!(
            reported.get(),
            "the failure is reported, not taken for a name without a key"
        );
        assert!(
            !report_once(&reported),
            "and the next name in the scan is the same failure, not a new one"
        );
    }

    /// The warning is worth one line per scan, not one per port: the scan asks
    /// five names and all five fail for the same reason. This is the claim the
    /// `Cell<bool>` cannot make on its own — a flag that ends up set says nothing
    /// about how many lines were printed.
    #[test]
    fn a_broken_tier_is_reported_once_for_a_whole_scan() {
        let reported = Cell::new(false);

        let reports: Vec<bool> = (0..5).map(|_| report_once(&reported)).collect();

        assert_eq!(
            reports,
            [true, false, false, false, false],
            "one failure, one line"
        );
    }
}
