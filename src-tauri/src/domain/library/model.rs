//! What the library keeps about one mod: its folder, its info, its family and its versions.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::domain::source::{ProviderId, SourceMod, SourceRequirement, SourceSync};
use crate::domain::version::IgnoredVersion;

/// Mod data from the `info.json` in the archive. Without the file, the name is the folder name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct ModInfo {
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
}

/// The name, version and author the user typed for one version. A field that is `None` keeps the value from
/// `info.json`. The edit is stored apart from the scanned info, so a rescan that reads the file again keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ModInfoEdit {
    pub name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
}

impl ModInfo {
    /// The name, version and author without the description. The list row keeps the short fields; the description is
    /// read by `mod_extras` when a column or the inspector needs it.
    pub fn without_description(&self) -> Self {
        Self {
            name: self.name.clone(),
            version: self.version.clone(),
            author: self.author.clone(),
            description: None,
        }
    }
}

impl ModInfoEdit {
    pub const fn is_empty(&self) -> bool {
        self.name.is_none() && self.version.is_none() && self.author.is_none()
    }

    /// Trims the fields. An empty field becomes `None`, which means the value from the file.
    pub fn cleaned(self) -> Self {
        let clean = |field: Option<String>| {
            field
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
        };
        Self {
            name: clean(self.name),
            version: clean(self.version),
            author: clean(self.author),
        }
    }

    /// Puts the typed values over `info`.
    pub fn apply(&self, info: &mut ModInfo) {
        if let Some(name) = &self.name {
            info.name.clone_from(name);
        }

        if let Some(version) = &self.version {
            info.version = Some(version.clone());
        }

        if let Some(author) = &self.author {
            info.author = Some(author.clone());
        }
    }
}

/// Why an archive entry was not unpacked into the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// No rule of the game definition took it.
    NoRule,
    /// Documentation, source or an optional variant folder.
    Noise,
    /// The FOMOD definition or its metadata.
    Metadata,
    /// An entry the FOMOD choices did not select.
    NotSelected,
}

/// One archive entry that was not unpacked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct SkippedFile {
    /// The entry name in the archive, with `/` as the separator.
    pub path: String,
    pub reason: SkipReason,
}

/// The name and version a provider read from an archive's file name. Stored on the archive once, so no reader parses
/// the file name again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveLabel {
    /// The name the author gave the mod in the file name. `None` when the provider's file names carry only a package
    /// slug (Thunderstore), so the mod keeps the name its `info.json` gives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub version: String,
}

/// An archive saved in `archives/` under its SHA-256 sum. The copy has the name of the original file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Archive {
    pub original_name: String,
    pub file: String,
    /// What the archive's provider read from its file name, when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub named: Option<ArchiveLabel>,
    /// The file id on each provider, when the archive came from one.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_files: BTreeMap<ProviderId, String>,
    /// The persistent part on each provider, when the archive came from one.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_parts: BTreeMap<ProviderId, String>,
    /// Entries no rule took when the archive was imported. Other reasons are recomputed by the archive report.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<SkippedFile>,
    /// How many entries no rule took, also when `skipped` is capped.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub skipped_total: u32,
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if takes a reference"
)]
const fn is_zero(value: &u32) -> bool {
    *value == 0
}

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

/// A checksum of an installed mod's files, recorded when the archive was unpacked. The digest covers every relative
/// path and that file's own SHA-256, so a change of content, name or number of files changes it. Paths are stored as
/// `/`-separated relative paths, which makes the value portable between Windows and Linux.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Integrity {
    /// Hex SHA-256 over the sorted `path\0file-hash\0` pairs.
    pub digest: String,
    /// Number of files the digest covers.
    pub files: u32,
    /// Sum of the sizes of the files the digest covers.
    pub bytes: u64,
}

/// One installed version in the library: the folder and what we know about it without scanning again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModVersion {
    /// Folder in `library/`; the key of the versions map.
    pub folder: String,
    /// Id of the family this version belongs to.
    pub family: String,
    /// Slot id. Empty in the `mirror` layout.
    pub slot: String,
    /// Files relative to the mod folder, sorted. Shared, because every status and every plan reads the same list.
    pub files: Arc<[String]>,
    pub size_bytes: u64,
    pub info: ModInfo,
    /// What the user typed for name, version and author. It wins over `info`.
    #[serde(default, skip_serializing_if = "ModInfoEdit::is_empty")]
    pub edit: ModInfoEdit,
    /// SHA-256 sum of the archive the version was unpacked from. Empty for a folder found by a rescan.
    pub archive: Option<String>,
    /// Source ids parsed from the archive name, per provider. The family may override them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parsed_sources: BTreeMap<ProviderId, String>,
    /// Persistent parts the version belongs to, per provider.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parsed_parts: BTreeMap<ProviderId, String>,
    /// Unix time in seconds of the import.
    pub installed_at: u32,
    /// The choices made in the archive's FOMOD installer, so a pack can install it the same way.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fomod: Vec<crate::domain::fomod::Choice>,
    /// The archive has a FOMOD installer. `fomod` may then be empty when the installer offered nothing to pick.
    #[serde(default)]
    pub fomod_installer: bool,
    /// Checksum of the installed files, taken at import. Missing for mods imported before this field existed and for
    /// folders found by a rescan; verification records the current files as the baseline in that case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrity: Option<Integrity>,
}

impl ModVersion {
    /// The FOMOD choices to install this version with again: `Some` when the archive had an installer or choices
    /// were kept, even an empty list, because the installer still chooses the files then.
    pub fn installer_choices(&self) -> Option<&[crate::domain::fomod::Choice]> {
        (self.fomod_installer || !self.fomod.is_empty()).then_some(self.fomod.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::{AdoptPolicy, ArchiveLabel, Family, ModVersion};
    use crate::domain::source::{ProviderId, SourceRef, SourceRequirement, SourceSync};
    use crate::domain::version::IgnoredVersion;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn a_label_saved_with_a_name_still_loads_with_it() {
        let label: ArchiveLabel =
            serde_json::from_str(r#"{"name":"Example Pack","version":"2.0"}"#).unwrap();
        assert_eq!(
            label,
            ArchiveLabel {
                name: Some("Example Pack".into()),
                version: "2.0".into(),
            }
        );
    }

    /// An older `library.json` has no integrity field; it must still load and save without one.
    #[test]
    fn an_old_mod_version_loads_without_an_integrity_field() {
        let old = r#"{
            "folder":"m","family":"m","slot":"","files":[],"size_bytes":0,
            "info":{"name":"M","version":null,"author":null,"description":null},
            "archive":null,"installed_at":0
        }"#;
        let version: ModVersion = serde_json::from_str(old).unwrap();
        assert_eq!(version.integrity, None);
        assert!(
            !serde_json::to_string(&version)
                .unwrap()
                .contains("integrity")
        );
    }

    #[test]
    fn a_label_without_a_name_is_saved_without_the_key() {
        let label = ArchiveLabel {
            name: None,
            version: "1.0.0".into(),
        };
        let text = serde_json::to_string(&label).unwrap();
        assert_eq!(text, r#"{"version":"1.0.0"}"#);
        let back: ArchiveLabel = serde_json::from_str(&text).unwrap();
        assert_eq!(back, label);
    }

    /// A non-default source family with every field each policy cares about, so the exact field outcomes below pin
    /// the asymmetry instead of relying on an empty fixture.
    fn source_family() -> Family {
        let provider = ProviderId::new("example-provider");
        Family {
            id: "example-family".into(),
            name: "Example Mod".into(),
            tags: BTreeSet::from(["example-tag".to_string()]),
            notes: "Source notes".into(),
            instructions: "Source instructions".into(),
            requires: vec!["source-dependency".into()],
            requirements: vec![SourceRequirement {
                source: SourceRef::new("example-provider", "10").with_file("20"),
                name: "Source requirement".into(),
                from_source: true,
                group: Some(2),
                ignored: false,
            }],
            incompatible: vec!["source-conflict".into()],
            overrides: vec!["source-override".into()],
            source_dlcs: BTreeMap::from([(provider.clone(), vec![vec!["Example DLC".into()]])]),
            source_sync: BTreeMap::from([(
                provider.clone(),
                SourceSync {
                    file_id: "20".into(),
                    version_id: "30".into(),
                    checked_at: 100,
                    mod_level: true,
                    older_version: Some("1.0".into()),
                },
            )]),
            latest: BTreeMap::from([(provider.clone(), "2.0".into())]),
            latest_file_ids: BTreeMap::from([(provider.clone(), "40".into())]),
            ignored_versions: BTreeMap::from([(
                provider,
                IgnoredVersion {
                    version: "1.5".into(),
                    file_id: Some("15".into()),
                },
            )]),
            ..Family::default()
        }
    }

    #[test]
    fn the_two_adopt_policies_cover_different_fields() {
        let source = source_family();
        let provider = ProviderId::new("example-provider");
        let remap = |id: &str| format!("remapped:{id}");

        // Hardcoded outcomes, not values read back from `source`: a shallow copy, a field copied from the wrong place
        // or a missed remap cannot pass either policy.
        let tags = BTreeSet::from(["example-tag".to_string()]);
        let requirements = vec![SourceRequirement {
            source: SourceRef::new("example-provider", "10").with_file("20"),
            name: "Source requirement".into(),
            from_source: true,
            group: Some(2),
            ignored: false,
        }];
        let source_dlcs = BTreeMap::from([(provider.clone(), vec![vec!["Example DLC".into()]])]);
        let source_sync = BTreeMap::from([(
            provider.clone(),
            SourceSync {
                file_id: "20".into(),
                version_id: "30".into(),
                checked_at: 100,
                mod_level: true,
                older_version: Some("1.0".into()),
            },
        )]);
        let latest = BTreeMap::from([(provider.clone(), "2.0".into())]);
        let latest_file_ids = BTreeMap::from([(provider.clone(), "40".into())]);
        let ignored_versions = BTreeMap::from([(
            provider,
            IgnoredVersion {
                version: "1.5".into(),
                file_id: Some("15".into()),
            },
        )]);

        let mut merged = Family::default();
        super::adopt_fields(&mut merged, &source, AdoptPolicy::Merge, remap);
        assert_eq!(merged.tags, tags);
        assert_eq!(merged.notes, "Source notes");
        assert_eq!(merged.instructions, "Source instructions");
        assert_eq!(merged.requires, ["remapped:source-dependency"]);
        assert_eq!(merged.requirements, requirements);
        assert_eq!(merged.incompatible, ["remapped:source-conflict"]);
        assert!(merged.overrides.is_empty(), "Merge never takes overrides");
        assert_eq!(merged.source_dlcs, source_dlcs);
        assert_eq!(merged.source_sync, source_sync);
        assert_eq!(merged.latest, latest);
        assert_eq!(merged.latest_file_ids, latest_file_ids);
        assert_eq!(merged.ignored_versions, ignored_versions);

        let mut filled = Family::default();
        super::adopt_fields(&mut filled, &source, AdoptPolicy::FillEmpty, remap);
        assert_eq!(filled.tags, tags);
        assert_eq!(filled.notes, "Source notes");
        assert_eq!(filled.instructions, "Source instructions");
        assert_eq!(filled.requires, ["remapped:source-dependency"]);
        assert_eq!(filled.requirements, requirements);
        assert_eq!(filled.incompatible, ["remapped:source-conflict"]);
        assert_eq!(filled.overrides, ["remapped:source-override"]);
        assert_eq!(filled.source_dlcs, source_dlcs);
        assert!(
            filled.source_sync.is_empty()
                && filled.latest.is_empty()
                && filled.latest_file_ids.is_empty()
                && filled.ignored_versions.is_empty(),
            "FillEmpty leaves all four update-metadata fields alone"
        );
    }
}
