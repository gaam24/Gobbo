use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::domain::source::{ProviderId, SourceMod, SourceRequirement, SourceSync};
use crate::domain::version::IgnoredVersion;

use super::version::ModVersion;

/// One mod across all installed versions. Tags, notes and the provider pages belong to the family, not to one version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Family {
    pub id: String,
    pub name: String,
    /// The mod on each provider, with the page metadata when it was read. The family id is derived from the first
    /// source when the mod came from one.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<ProviderId, SourceMod>,
    #[serde(default)]
    pub tags: BTreeSet<String>,
    #[serde(default)]
    pub notes: String,
    /// How to install or set up the mod, for the players who get it in a mod list. Shared by all versions.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub instructions: String,
    /// Families this mod needs to work. A missing or disabled one is a warning, not a block.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    /// Mods on external providers this one needs that may not be in the library.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requirements: Vec<SourceRequirement>,
    /// The DLCs each provider lists for the newest file, one entry per requirement.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_dlcs: BTreeMap<ProviderId, Vec<Vec<String>>>,
    /// Where the requirements above came from, per provider, so an unchanged file is not read again.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_sync: BTreeMap<ProviderId, SourceSync>,
    /// The newest version the provider listed for this part, when a check read it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub latest: BTreeMap<ProviderId, String>,
    /// The provider's id of the newest file it listed for this part, when the check read a file chain. It is exact
    /// where a version string is not, so a hotfix such as `1.1.3a` is not lost to `1.1.3`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub latest_file_ids: BTreeMap<ProviderId, String>,
    /// The exact version the user chose not to be reminded about, per provider. A check that finds another version
    /// drops the entry, so the reminder returns.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub ignored_versions: BTreeMap<ProviderId, IgnoredVersion>,
    /// Families that do not work together with this one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub incompatible: Vec<String>,
    /// Families this mod overrides: it must stand above them in the list, so its copy of a shared file wins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<String>,
}

/// How the user data of one family is taken over by another. A move into a family that already exists adds what the
/// target lacks without dropping what it has; a pack that fills a local copy only supplies fields the copy has none of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdoptPolicy {
    /// Add missing entries to lists and maps, keeping every value the target already has.
    Merge,
    /// Take a field from the source only when the target's field is empty.
    FillEmpty,
}

/// Takes over the user data of `source` into `target` under `policy`. `remap` translates the family ids of relations
/// from the source's namespace into the target's, and is the identity when both use the same ids. This is the one rule
/// for tags, notes, instructions, relations and the provider data that travels with a family; the provider pages
/// themselves have their own rule at each caller.
///
/// The policies deliberately cover different fields. [`AdoptPolicy::Merge`] adds source entries to `tags`, `requires`,
/// `requirements`, `incompatible`, `source_dlcs`, `source_sync`, `latest`, `latest_file_ids` and `ignored_versions`,
/// keeping what the target had, and fills only empty `notes` and `instructions`. [`AdoptPolicy::FillEmpty`] copies
/// `tags`, `notes`, `instructions`, `requires`, `requirements`, `incompatible`, `overrides` and `source_dlcs` only
/// when the target has none. It leaves the four update-metadata fields (`source_sync`, `latest`, `latest_file_ids`
/// and `ignored_versions`) untouched, and never merges `overrides`: the shipped pack-adoption flow relies on a local
/// copy keeping its own saved checks and override order.
pub fn adopt_fields(
    target: &mut Family,
    source: &Family,
    policy: AdoptPolicy,
    remap: impl Fn(&str) -> String,
) {
    let relations = |ids: &[String]| -> Vec<String> { ids.iter().map(|id| remap(id)).collect() };

    match policy {
        AdoptPolicy::Merge => {
            target.tags.extend(source.tags.iter().cloned());

            if target.notes.is_empty() {
                target.notes.clone_from(&source.notes);
            }

            if target.instructions.is_empty() {
                target.instructions.clone_from(&source.instructions);
            }

            for other in relations(&source.requires) {
                if !target.requires.contains(&other) {
                    target.requires.push(other);
                }
            }

            for other in relations(&source.incompatible) {
                if !target.incompatible.contains(&other) {
                    target.incompatible.push(other);
                }
            }

            for requirement in &source.requirements {
                if !target
                    .requirements
                    .iter()
                    .any(|kept| kept.source == requirement.source)
                {
                    target.requirements.push(requirement.clone());
                }
            }

            for (provider, dlcs) in &source.source_dlcs {
                target
                    .source_dlcs
                    .entry(provider.clone())
                    .or_insert_with(|| dlcs.clone());
            }

            for (provider, sync) in &source.source_sync {
                target
                    .source_sync
                    .entry(provider.clone())
                    .or_insert_with(|| sync.clone());
            }

            for (provider, latest) in &source.latest {
                target
                    .latest
                    .entry(provider.clone())
                    .or_insert_with(|| latest.clone());
            }

            for (provider, latest_file) in &source.latest_file_ids {
                target
                    .latest_file_ids
                    .entry(provider.clone())
                    .or_insert_with(|| latest_file.clone());
            }

            for (provider, ignored) in &source.ignored_versions {
                target
                    .ignored_versions
                    .entry(provider.clone())
                    .or_insert_with(|| ignored.clone());
            }
        }
        AdoptPolicy::FillEmpty => {
            if target.tags.is_empty() {
                target.tags.clone_from(&source.tags);
            }

            if target.notes.is_empty() {
                target.notes.clone_from(&source.notes);
            }

            if target.instructions.is_empty() {
                target.instructions.clone_from(&source.instructions);
            }

            if target.requires.is_empty() {
                target.requires = relations(&source.requires);
            }

            if target.requirements.is_empty() {
                target.requirements.clone_from(&source.requirements);
            }

            if target.incompatible.is_empty() {
                target.incompatible = relations(&source.incompatible);
            }

            if target.overrides.is_empty() {
                target.overrides = relations(&source.overrides);
            }

            if target.source_dlcs.is_empty() {
                target.source_dlcs.clone_from(&source.source_dlcs);
            }
        }
    }
}

impl Family {
    /// The family of a newly found version: named after the version, or after its folder when it has no name.
    pub fn of(version: &ModVersion) -> Self {
        let name = if version.info.name.is_empty() {
            &version.folder
        } else {
            &version.info.name
        };
        Self {
            id: version.family.clone(),
            name: name.clone(),
            sources: version
                .parsed_sources
                .iter()
                .map(|(provider, id)| {
                    (
                        provider.clone(),
                        SourceMod {
                            mod_id: id.clone(),
                            part_id: version.parsed_parts.get(provider).cloned(),
                            part_name: None,
                            part_category: None,
                            meta: None,
                        },
                    )
                })
                .collect(),
            ..Self::default()
        }
    }

    /// The mod on one provider, when the family has one.
    pub fn source(&self, provider: &str) -> Option<&SourceMod> {
        self.sources.get(provider)
    }

    /// Removes the source the family has on one provider with the check data tied to it: the newest version and file
    /// and an ignored version. The DLCs, the last sync and the requirements read from that source stay, so clearing a
    /// source the user typed does not hide them until the next refresh.
    pub fn clear_source(&mut self, provider: &ProviderId) {
        self.sources.remove(provider);
        self.latest.remove(provider);
        self.latest_file_ids.remove(provider);
        self.ignored_versions.remove(provider);
    }

    /// Forgets everything the family knows from one provider: the mod on it with its page, the newest version and file,
    /// an ignore, the DLCs and sync of its requirements, and the requirements it listed for its newest file. The
    /// requirements the user typed stay. Used when the family moves to another page of that provider, so no data of
    /// the old page shows for the new one.
    pub fn forget_provider(&mut self, provider: &ProviderId) {
        self.clear_source(provider);
        self.source_dlcs.remove(provider);
        self.source_sync.remove(provider);
        self.requirements.retain(|requirement| {
            !(requirement.from_source && requirement.source.provider == *provider)
        });
    }
}
