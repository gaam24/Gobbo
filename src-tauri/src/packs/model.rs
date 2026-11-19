//! The pack format: the manifest and the parts it holds, and the views the interface shows.

use crate::error::Message;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;

pub(super) const MANIFEST: &str = "pack.json";

/// Archives inside the pack: `archives/<sha256>/<original name>`.
pub(super) const ARCHIVES: &str = "archives/";

/// The manifest format written now. Version 3 adds source parts; version 2 packs are still read.
pub(super) const FORMAT: u32 = 3;

/// The manifest formats this build can read. Version 1 packs cannot be read.
pub(super) const SUPPORTED_FORMATS: &[u32] = &[2, 3];

/// The prefix of the folder where one import stages its archives before they are imported. The folder carries a unique
/// suffix and is removed when that import ends.
pub(super) const STAGING_DIR: &str = "pack-staging";

/// What the pack is: shown before an import, and kept with each profile made from it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, specta::Type)]
pub struct PackInfo {
    /// A stable id, the same in every version of the pack. Empty on export means one made from the name.
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
}

/// Which archives travel inside the pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PackArchives {
    /// None: every mod comes from an external provider.
    None,
    /// The mods that are not on an external provider, which the receiver could not get otherwise.
    Unlisted,
    /// All of them. Only with the permission of the mods' authors.
    All,
}

/// What to export.
#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct PackOptions {
    pub info: PackInfo,
    /// The profiles, by id, in the order to export them.
    pub profiles: Vec<String>,
    pub archives: PackArchives,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Manifest {
    pub(super) format: u32,
    pub(super) game: String,
    #[serde(flatten)]
    pub(super) info: PackInfo,
    pub(super) profiles: Vec<PackProfile>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PackProfile {
    pub(super) name: String,
    pub(super) mods: Vec<PackMod>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) separators: Vec<PackSeparator>,
    /// File winners: the path key and the index of the winning mod in `mods`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(super) winners: BTreeMap<String, usize>,
    /// Mod groups, with each member an index in `mods`. Older packs omit them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) groups: Vec<PackGroup>,
    /// The author's isolation choices, by user-data set id. Older packs omit it; the receiver then keeps the
    /// definition defaults. Set ids come from the game definition, so they travel without machine-specific paths.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(super) user_data: BTreeMap<String, crate::domain::user_data::Isolation>,
}

use crate::domain::pack::PackMod;

/// A separator above the mod at `before` (an index in `mods`), or at the end.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PackSeparator {
    pub(super) name: String,
    pub(super) before: Option<usize>,
}

/// A mod group of a pack. Members are indices in the profile's `mods`, like a separator's `before` and the winners,
/// so a group travels to a receiver that assigns different family ids. The author's local group id is not stored; the
/// receiver makes a fresh one.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PackGroup {
    pub(super) name: String,
    pub(super) kind: crate::domain::profiles::GroupKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) language: Option<String>,
    #[serde(default)]
    pub(super) members: Vec<usize>,
}

/// A mod of a pack that is not in the library and not inside the pack: the user gets it from its provider.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PackMissing {
    pub name: String,
    pub version: Option<String>,
    /// The provider page the user can fetch the mod from.
    pub source: Option<crate::domain::source::SourceRef>,
    /// The name of the file the author installed, to pick the right one on the mod's page.
    pub archive: Option<String>,
}

/// How an import changes a profile made from an earlier version of the same pack.
#[derive(Debug, Serialize, specta::Type)]
pub struct PackUpdate {
    pub profile: String,
    pub from: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub updated: Vec<String>,
}

/// What an import would do, before it does anything.
#[derive(Debug, Serialize, specta::Type)]
pub struct PackPreview {
    pub info: PackInfo,
    pub profiles: Vec<String>,
    /// Mods in all profiles of the pack, each counted once.
    pub total: u32,
    /// Of them, the ones already in the library.
    pub have: u32,
    /// The ones whose archive is inside the pack.
    pub bundled: u32,
    pub missing: Vec<PackMissing>,
    /// Profiles that an earlier version of this pack made, and what changes in them.
    pub updates: Vec<PackUpdate>,
}

/// What importing a pack did: the archives it added and the profiles it created or updated.
#[derive(Debug, Serialize, specta::Type)]
pub struct PackImportView {
    /// Archives from inside the pack that went into the library.
    pub imported: u32,
    /// Archives from inside the pack that could not be imported, by name.
    pub failed: Vec<String>,
    pub profiles_created: u32,
    pub profiles_updated: u32,
    /// Mods the library lacks. They wait in their profiles with their place and state, and join the list once the
    /// library has them.
    pub missing: Vec<PackMissing>,
}

/// How many mods and archives a pack export wrote.
#[derive(Debug, Serialize, specta::Type)]
pub struct PackExportView {
    pub mods: u32,
    pub archives: u32,
}

/// Something to look at before a pack goes out. An error makes the pack impossible to rebuild.
#[derive(Debug, Serialize, specta::Type)]
pub struct PackIssue {
    pub error: bool,
    pub message: Message,
}
