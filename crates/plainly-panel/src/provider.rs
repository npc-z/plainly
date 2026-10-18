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
//! **configured** endpoint is not on this machine and which core's readiness says
//! is usable — deterministic, and always one of the providers the product ships
//! rather than whichever name happens to sit in the file. When nothing satisfies
//! it, there is no cloud alternative and ⟳ is honestly just a regeneration with
//! the same model; the note under the content says which of the two it is, so the
//! promise and the behaviour cannot drift apart.
//!
//! Both answers come from core: [`plainly_core::local_of`] for "is this machine",
//! and [`plainly_core::Readiness`] for "could it be used right now" — the same
//! values the run path is judged by, so a preset pointed at another host is
//! offered under the requirement that host actually makes.

use plainly_core::presets;
use plainly_core::{Readiness, Secrets, local_of};

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
    /// The cloud provider ⟳ would move to, when there is one.
    ///
    /// A shipped preset, not a configured name: "cloud" is a fact about the
    /// endpoint, and the presets are where endpoints come from.
    ///
    /// The *configured* endpoint is what decides, not the one the preset ships:
    /// a name pointed at the user's own box is not a move to the cloud, and a name
    /// pointed at another remote service is judged by the requirement that
    /// endpoint makes. Both facts — locality and usability — are core's, so the
    /// candidate the button offers is one the run path would also accept.
    fn cloud_alternative(config: &plainly_core::Config, secrets: &Secrets) -> Option<Self> {
        presets::PRESETS.iter().find_map(|preset| {
            let local = local_of(preset.name, config);
            (!local && Readiness::of(preset.name, config, secrets).usable()).then(|| Self {
                name: preset.name.to_string(),
                // Derived from the same predicate, never assumed: the filter above
                // is what makes it false today, and it is not a second rule.
                local,
            })
        })
    }

    /// The provider in effect, and (if ⟳ would move) the cloud one beside it.
    ///
    /// One read of the configuration and one of the secret stores, because the two
    /// answers are about the same moment: a note that promised a cloud rerun while
    /// the key had not been entered yet would be worse than one that stayed quiet.
    pub fn of(config: &plainly_core::Config, secrets: &Secrets) -> (Self, Option<Self>) {
        let name = config.app.provider.clone();
        // "On this machine" is the configured endpoint's fact, and core answers it
        // for every surface the same way (spec §6). It does not need a resolved
        // setup: a local preset with no model chosen yet is still local
        // (tickets/06), and "which model" has nothing to do with "where it runs".
        let local = local_of(&name, config);

        (
            Self { name, local },
            Self::cloud_alternative(config, secrets),
        )
    }
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
    use plainly_core::{Config, EnvSecrets, SecretError, SecretStore, Secrets};

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

    /// A keyring that is present but cannot answer: a locked collection, or a
    /// session bus with no Secret Service behind it.
    struct BrokenKeyring;

    impl SecretStore for BrokenKeyring {
        fn get(&self, _provider: &str) -> Result<Option<String>, SecretError> {
            Err(SecretError::Unavailable(
                "the keyring is locked".to_string(),
            ))
        }

        fn set(&self, _provider: &str, _secret: &str) -> Result<(), SecretError> {
            Ok(())
        }

        fn delete(&self, _provider: &str) -> Result<(), SecretError> {
            Ok(())
        }
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

    /// A preset whose *configured* endpoint points at this machine is not a cloud
    /// move: "cloud" is a fact about where the request would go, not about the
    /// preset's shipped endpoint. Offering it would promise a switch that stays on
    /// this box, so the next usable shipped preset is the honest answer — even
    /// though this one has a key.
    #[test]
    fn a_shipped_preset_configured_to_loopback_is_not_a_cloud_candidate() {
        let config = Config {
            providers: [(
                "deepseek".to_string(),
                plainly_core::ProviderProfile {
                    endpoint: Some("http://127.0.0.1:9999/v1".to_string()),
                    model: Some("deepseek-flash".to_string()),
                    thinking: plainly_core::Thinking::Off,
                },
            )]
            .into_iter()
            .collect(),
            ..Config::default()
        };
        let (_, cloud) = ProviderChoice::of(&config, &secrets_with(&["deepseek", "openai"]));

        assert_eq!(
            cloud,
            Some(ProviderChoice {
                name: "openai".to_string(),
                local: false
            }),
            "the loopback preset is skipped and openai is offered"
        );
    }

    /// A preset pointed at another remote service keeps the endpoint-filtered
    /// requirement — `Optional`, because what a preset knows about a key belongs
    /// to the service it names — so it is offered without one, exactly as the run
    /// path would judge it.
    #[test]
    fn a_shipped_preset_pointed_at_another_remote_host_is_offered_without_a_key() {
        let config = Config {
            providers: [(
                "deepseek".to_string(),
                plainly_core::ProviderProfile {
                    endpoint: Some("https://api.example.com/v1".to_string()),
                    model: None,
                    thinking: plainly_core::Thinking::Off,
                },
            )]
            .into_iter()
            .collect(),
            ..Config::default()
        };
        let (_, cloud) = ProviderChoice::of(&config, &secrets_with(&[]));

        assert_eq!(
            cloud,
            Some(ProviderChoice {
                name: "deepseek".to_string(),
                local: false
            }),
            "the requirement follows the configured endpoint, not the preset's name"
        );
    }

    /// A store that cannot answer is not a key, and it is not a reason to invent a
    /// sentence either: with no usable cloud candidate the panel stays quiet and
    /// the run-level notice reports what is wrong with the configured provider.
    #[test]
    fn a_broken_keyring_leaves_no_cloud_candidate_and_no_new_note() {
        let config = crate::support::local_config();
        let broken = Secrets::with_stores(EnvSecrets::empty(), Some(Box::new(BrokenKeyring)));
        let (provider, cloud) = ProviderChoice::of(&config, &broken);

        assert!(provider.local);
        assert_eq!(cloud, None);
        assert_eq!(
            local_note(cloud.as_ref()),
            crate::strings::LOCAL_NOTE_NO_CLOUD,
            "the existing no-cloud sentence, not a new explanation"
        );
    }
}
