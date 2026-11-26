use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::domain::source::ProviderId;

use super::mod_info::{ModInfo, ModInfoEdit};

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
