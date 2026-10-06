//! `plainly providers`: the API-key half of the provider surface. Discovery,
//! capability probes and `providers probe` land with ticket 05.

use std::io::BufRead;

use plainly_core::{Cleared, KeySource, Secrets, Stored};

use crate::cli::{KeyCommand, ProviderCommand};
use crate::commands::CommandError;
use crate::exit;

pub fn run(command: ProviderCommand) -> Result<u8, CommandError> {
    match command {
        ProviderCommand::Key { command } => key(command),
    }
}

fn key(command: KeyCommand) -> Result<u8, CommandError> {
    match command {
        KeyCommand::Set { name } => {
            let secret = read_secret()?;
            let secrets = Secrets::from_process_env();

            // A key in the environment outranks anything we store. Say so,
            // rather than letting the user believe the keyring now decides.
            if secrets.key_source(&name) == Some(KeySource::Environment) {
                eprintln!(
                    "note: {} is set and takes precedence over the keyring",
                    plainly_core::env_var_name(&name)
                );
            }

            match secrets.store(&name, &secret) {
                Stored::Keyring => {
                    eprintln!("stored the {name} key in the OS keyring");
                    Ok(exit::SUCCESS)
                }
                Stored::Session { warning } => {
                    // Nothing was persisted: this process is about to exit. For
                    // a one-shot command, saying "success" here would be a lie.
                    Err(CommandError::Failed(warning))
                }
            }
        }
        KeyCommand::Clear { name } => {
            let secrets = Secrets::from_process_env();
            match secrets
                .clear(&name)
                .map_err(|error| CommandError::Failed(error.to_string()))?
            {
                Cleared::Keyring => {
                    eprintln!("cleared the {name} key from the OS keyring");
                    Ok(exit::SUCCESS)
                }
                Cleared::Nothing => {
                    eprintln!(
                        "no OS keyring in use; nothing to clear. \
                         Unset {} if it is set.",
                        plainly_core::env_var_name(&name)
                    );
                    Ok(exit::SUCCESS)
                }
            }
        }
    }
}

/// A key arrives on stdin, never in argv: an argument would land in shell
/// history and in `ps` output.
fn read_secret() -> Result<String, CommandError> {
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|error| {
            CommandError::Failed(format!("cannot read the key from stdin: {error}"))
        })?;

    let secret = line.trim_end_matches(['\n', '\r']).to_string();
    if secret.is_empty() {
        return Err(CommandError::Usage(
            "no key on stdin; pipe it in, for example: printf %s \"$KEY\" | plainly providers key set deepseek"
                .to_string(),
        ));
    }
    Ok(secret)
}
