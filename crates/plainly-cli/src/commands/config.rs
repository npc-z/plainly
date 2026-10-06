//! `plainly config`: where the settings live, what they are, and how to change
//! one.
//!
//! stdout carries the product (`config path`, `config show`) and nothing else;
//! confirmations go to stderr so a pipeline stays clean.

use plainly_core::{ConfigFile, Paths};

use crate::cli::ConfigCommand;
use crate::commands::CommandError;
use crate::exit;

pub fn run(command: ConfigCommand) -> Result<u8, CommandError> {
    let paths = Paths::discover().map_err(|error| CommandError::Failed(error.to_string()))?;
    let mut file = ConfigFile::open(paths.config_file())?;

    match command {
        ConfigCommand::Path => {
            println!("{}", file.path().display());
            Ok(exit::SUCCESS)
        }
        ConfigCommand::Show => {
            if !file.exists() {
                eprintln!(
                    "no configuration file yet at {}; showing the shipped defaults",
                    file.path().display()
                );
            }
            print!("{}", file.effective_toml()?);
            Ok(exit::SUCCESS)
        }
        ConfigCommand::Set { key, value } => {
            file.set(&key, &value)?;
            file.save()?;
            // The value is echoed as it was written, not as it was typed, so
            // `set app.level a2` does not look like it stored "a2".
            eprintln!("set {key} in {}", file.path().display());
            Ok(exit::SUCCESS)
        }
    }
}
