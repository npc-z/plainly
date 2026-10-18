//! Who produced an Explanation, as the chip in the panel's title bar.
//!
//! Three facts and no more: which provider answered, which model, and whether
//! that model is on this machine. The third is the one that changes what the text
//! is worth, so it is half of the chip rather than decoration beside the name.
//!
//! The value is built where the run is, not where the chip is drawn: the worker
//! thread holds the `ProviderSetup` the request actually went to, and deriving
//! "local" again in the panel from the provider's *name* would answer a different
//! question — an `ollama` pointed at another host, or a custom name pointed at
//! `127.0.0.1`, is exactly the pair of cases where the name lies.

use plainly_core::Artifact;

use crate::strings;

/// The chip's data: provider, model, and where the model runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub provider: String,
    pub model: String,
    /// Whether the endpoint that answered is on this machine, as
    /// [`plainly_core::ProviderSetup::is_local`] decides it.
    pub local: bool,
}

impl Provenance {
    /// The provenance of one answered Passage.
    ///
    /// The provider and the model come from the Artifact rather than from the
    /// setup: a Record recalled from the cache was written by whatever ran then,
    /// and its own provenance is the one that is true of it.
    pub fn of(artifact: &Artifact, local: bool) -> Self {
        Self {
            provider: artifact.provider.clone(),
            model: artifact.model.clone(),
            local,
        }
    }

    /// The line the chip shows.
    pub fn text(&self) -> String {
        strings::provenance(&self.provider, &self.model, self.local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support;

    fn artifact() -> Artifact {
        support::artifact("The manager had gone to ground.", "ollama", "llama3.2")
    }

    /// A cached answer keeps the provider that wrote it, not the one configured
    /// now: saying otherwise would misattribute it.
    #[test]
    fn the_chip_names_the_provider_the_artifact_was_written_by() {
        let provenance = Provenance::of(&artifact(), true);

        assert_eq!(provenance.provider, "ollama");
        assert_eq!(provenance.model, "llama3.2");
        assert_eq!(provenance.text(), "ollama · llama3.2 · 本地");
    }

    #[test]
    fn the_cloud_badge_is_the_absence_of_the_local_one() {
        assert_eq!(
            Provenance::of(&artifact(), false).text(),
            "ollama · llama3.2 · 云端"
        );
    }
}
