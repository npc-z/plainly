//! `plainly providers`: the API-key half of the provider surface.
//!
//! Discovery, capability probes and `providers probe` land with ticket 05.

use std::io::Read;

use plainly_core::{Cleared, Secrets, Stored};

use crate::cli::{KeyCommand, ProviderCommand};
use crate::commands::CommandError;
use crate::exit;

pub fn run(command: ProviderCommand) -> Result<u8, CommandError> {
    match command {
        ProviderCommand::Key { command } => match command {
            KeyCommand::Set { name } => store_key(&name),
            KeyCommand::Clear { name } => clear_key(&name),
        },
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
            plainly_core::env_var_name(name)
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
                plainly_core::env_var_name(name)
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
