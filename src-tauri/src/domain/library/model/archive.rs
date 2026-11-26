use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::source::ProviderId;

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

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if takes a reference"
)]
const fn is_zero(value: &u32) -> bool {
    *value == 0
}
