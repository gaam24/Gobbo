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
    use super::{ArchiveLabel, ModVersion};

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
}
