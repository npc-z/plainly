//! Which provider a trigger talks to — and, for ⟳, which one it would rather.
//!
//! The type is `ProviderChoice` rather than `Provider` on purpose: the glossary
//! reserves "provider" for the profile the Lookup Key is made of, the domain
//! already has `ProviderSetup` for the resolved profile, and a third name meaning
//! "the provider this trigger picked" would be the kind of collision that makes
//! two modules each other's business.
//!
//! The panel reads its provider out of `[app].provider` like every other surface
//! (spec §8), so the ordinary trigger needs nothing here. ⟳ is the exception, and
//! it is the whole reason this module exists: the note the panel puts under a
//! locally hosted model tells the person to press ⟳ "换成云端重新生成" (spec §12),
//! and a button that re-ran the same local model would make that sentence a lie.
//!
//! "换成云端" needs two answers, and neither is guessable: which cloud provider,
//! and whether it can be used *now*. The choice is the first shipped preset whose
//! endpoint is not on this machine and whose key requirement is satisfied —
//! deterministic, and always one of the providers the product ships rather than
//! whichever name happens to sit in the file. When nothing satisfies it, there is
//! no cloud alternative and ⟳ is honestly just a regeneration with the same model;
//! the note under the content says which of the two it is, so the promise and the
//! behaviour cannot drift apart.

use plainly_core::Secrets;
use plainly_core::presets::{self, KeyRequirement};
use plainly_core::setup::is_loopback;

use crate::strings;

/// The provider a trigger will talk to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderChoice {
    pub name: String,
    /// Whether the endpoint is on this machine, which decides the note and the
    /// chip (spec §7, §12).
    pub local: bool,
}

impl ProviderChoice {
    /// Whether the provider named in the configuration can be used at all.
    ///
    /// The same rule the settings screen applies and the CLI enforces: a key that
    /// must be found but is not is not a provider this panel can send to, whatever
    /// the file says (spec §8).
    fn usable(name: &str, secrets: &Secrets) -> bool {
        match preset_requirement(name) {
            Some(KeyRequirement::Required) => {
                matches!(secrets.resolve(name), Ok(Some(_)))
            }
            // An endpoint that may authenticate nothing is usable without a key:
            // its own refusal is a better answer than a panel that never sends.
            _ => true,
        }
    }

    /// The cloud provider ⟳ would move to, when there is one.
    ///
    /// A shipped preset, not a configured name: "cloud" is a fact about the
    /// endpoint, and the presets are where endpoints come from.
    ///
    /// The *preset's* endpoint is what decides, not the configured one: what the
    /// move needs is a service that is not this machine, and a preset the user has
    /// pointed at their own box is precisely the case the shipped endpoint rules
    /// out. The configured override applies to `[providers.<name>]` of the
    /// provider in effect, and a name being a preset at all is what makes it a
    /// candidate for the button.
    fn cloud_alternative(secrets: &Secrets) -> Option<Self> {
        presets::PRESETS
            .iter()
            .filter(|preset| !is_loopback(preset.endpoint))
            .find(|preset| Self::usable(preset.name, secrets))
            .map(|preset| Self {
                name: preset.name.to_string(),
                local: false,
            })
    }

    /// The provider in effect, and (if ⟳ would move) the cloud one beside it.
    ///
    /// One read of the configuration and one of the secret stores, because the two
    /// answers are about the same moment: a note that promised a cloud rerun while
    /// the key had not been entered yet would be worse than one that stayed quiet.
    pub fn of(config: &plainly_core::Config, secrets: &Secrets) -> (Self, Option<Self>) {
        let name = config.app.provider.clone();
        // The endpoint decides locality rather than the name, so a known provider
        // pointed somewhere else is judged by where it actually goes (spec §6).
        //
        // `endpoint_of` rather than `resolve`, because a local preset ships no
        // model: it cannot be *resolved* until the user picks one (tickets/06), and
        // "which model" has nothing to do with "where the model runs". The
        // desktop's own `is_local` asks the same question the same way.
        let local = plainly_core::ProviderSetup::endpoint_of(&name, config)
            .map(|endpoint| plainly_core::setup::is_loopback(&endpoint))
            .unwrap_or(false);

        (Self { name, local }, Self::cloud_alternative(secrets))
    }
}

/// Whether a shipped preset for this name demands a key.
fn preset_requirement(name: &str) -> Option<KeyRequirement> {
    presets::preset(name).map(|preset| preset.key)
}

/// The note under the content, for a locally hosted model.
///
/// Two sentences because there are two situations, and the sentence is the button's
/// instructions: with a cloud provider to move to, ⟳ is that move; without one it
/// is an ordinary regeneration, and saying "换成云端" would send the person looking
/// for a provider they do not have (spec §12).
pub fn local_note(cloud: Option<&ProviderChoice>) -> &'static str {
    match cloud {
        Some(_) => strings::LOCAL_NOTE,
        None => strings::LOCAL_NOTE_NO_CLOUD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plainly_core::{Config, EnvSecrets, Secrets};

    /// A secret store that answers exactly the names it was given a key for.
    ///
    /// Through the environment tier, which is the one tier a test can populate
    /// without a keyring — and the same way the desktop's own tests spell it, so
    /// that "a key for this provider" means one thing across the workspace.
    fn secrets_with(names: &[&str]) -> Secrets {
        Secrets::with_stores(
            EnvSecrets::from_pairs(
                names
                    .iter()
                    .map(|name| (plainly_core::env_var_name(name), "test-key")),
            ),
            None,
        )
    }

    #[test]
    fn a_configured_local_provider_is_local_and_the_cloud_one_is_deepseek() {
        let config = Config::default();
        let (provider, cloud) = ProviderChoice::of(&config, &secrets_with(&["deepseek"]));

        assert_eq!(provider.name, "deepseek");
        assert!(!provider.local, "the shipped default is a hosted service");
        assert_eq!(
            cloud.map(|provider| provider.name),
            Some("deepseek".to_string())
        );
    }

    #[test]
    fn a_local_provider_with_a_cloud_key_offers_the_cloud_move() {
        let config = crate::support::local_config();
        let (provider, cloud) = ProviderChoice::of(&config, &secrets_with(&["deepseek"]));

        assert!(provider.local);
        assert_eq!(
            cloud,
            Some(ProviderChoice {
                name: "deepseek".to_string(),
                local: false
            })
        );
    }

    /// No key for any hosted provider means no cloud move, and the note says so
    /// rather than instructing the person to press a button that would not do it.
    #[test]
    fn a_local_provider_with_no_cloud_key_offers_nothing() {
        let config = crate::support::local_config();
        let (provider, cloud) = ProviderChoice::of(&config, &secrets_with(&[]));

        assert!(provider.local);
        assert_eq!(cloud, None);
        assert_ne!(local_note(cloud.as_ref()), local_note(Some(&provider)));
    }

    /// A hosted provider pointed at this machine is local: the endpoint decides,
    /// not the name (`spec §6`).
    #[test]
    fn a_hosted_name_pointed_at_loopback_is_local() {
        let config = Config {
            providers: [(
                "deepseek".to_string(),
                plainly_core::ProviderProfile {
                    endpoint: Some("http://127.0.0.1:9999/v1".to_string()),
                    model: Some("deepseek-chat".to_string()),
                    thinking: plainly_core::Thinking::Off,
                },
            )]
            .into_iter()
            .collect(),
            ..Config::default()
        };
        let (provider, _) = ProviderChoice::of(&config, &secrets_with(&["deepseek"]));

        assert!(provider.local, "the endpoint is on this machine");
    }

    /// A name that does not resolve is not claimed to be local: nothing can say
    /// where it points, so the warning that rests on "on this machine" stays off.
    #[test]
    fn a_name_that_does_not_resolve_is_not_local() {
        let config = Config {
            app: plainly_core::App {
                provider: "nobody-ships-this".to_string(),
                ..plainly_core::App::default()
            },
            ..Config::default()
        };
        let (provider, _) = ProviderChoice::of(&config, &secrets_with(&["deepseek"]));

        assert!(!provider.local);
    }
}
