//! Groups of related mods on a profile list. A group holds family ids, so it survives a version switch.

use serde::{Deserialize, Serialize};

/// What a group holds together. The serde container name keeps the TypeScript export distinct from the FOMOD
/// importer's `GroupKind`; variant values are still `bundle` and `translation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename = "ModGroupKind", rename_all = "snake_case")]
pub enum GroupKind {
    Bundle,
    Translation,
}

/// A named set of families on a profile list. `members` holds family ids, not version folder names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ModGroup {
    pub id: String,
    pub name: String,
    pub members: Vec<String>,
    pub kind: GroupKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}
