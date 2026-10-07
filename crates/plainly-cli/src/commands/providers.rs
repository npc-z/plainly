//! `plainly providers`: what Plainly is talking to, what that endpoint takes,
//! and the API keys.
//!
//! The status report goes to stdout because it *is* the product of the command —
//! the same reason `config show` prints there — while progress and complaints go
//! to stderr.
//!
//! Only `probe` touches the network. Reporting reads the cache a probe wrote and
//! says "not probed yet" when there is none: a display that reaches out to an
//! endpoint would turn `plainly providers` into a request nobody asked for.

use std::io::Read;

use plainly_core::{
    Capability, ChatCompletions, Cleared, Config, ConfigFile, KeyRequirement, Origin, Paths,
    ProviderSetup, Secrets, Stored, ThinkingSwitch, env_var_name, probe,
};

use crate::cli::{KeyCommand, ProviderCommand};
use crate::commands::{CommandError, cache, now, resolve_key, tier};
use crate::exit;

pub fn run(command: Option<ProviderCommand>) -> Result<u8, CommandError> {
    match command {
        // `plainly providers` on its own is the report, not a usage error.
        None => status(),
        Some(ProviderCommand::Probe { name }) => probe_one(name),
        Some(ProviderCommand::Key { command }) => match command {
            KeyCommand::Set { name } => store_key(&name),
            KeyCommand::Clear { name } => clear_key(&name),
        },
    }
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
