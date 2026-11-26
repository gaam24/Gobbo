//! The references a family stores: the mod, its exact file and the part it belongs to.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::id::ProviderId;
use super::page::SourceMeta;

/// The family key of a provider page. The one rule shared by every place that names a family from a provider and a
/// mod id: a waiting entry, a saved source id, a status row, a collection key and a plan key.
pub fn family_key(provider: &str, mod_id: &str) -> String {
    format!("{provider}:{mod_id}")
}

/// A mod on a provider, the exact file when one is known, and the stable part it belongs to.
///
/// `file_id` names one version; `part_id` names the provider's update chain, for example a Nexus `mod_file_id`.
/// A part is the installable unit: its versions replace each other, while two parts of the same page stay separate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
pub struct SourceRef {
    /// The service that owns the ids.
    pub provider: ProviderId,
    /// The mod id in the provider's namespace. Text because providers use numbers, slugs or base62.
    pub mod_id: String,
    /// The file id in the provider's namespace, when the reference names a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    /// The stable part of the page, when the provider has more than one installable file group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_id: Option<String>,
    /// The part's display name, when the provider named it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_name: Option<String>,
    /// The provider's category for the part, for example `main` or `optional`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_category: Option<String>,
}

impl SourceRef {
    /// A reference to a mod, without a file.
    pub fn new(provider: impl Into<ProviderId>, mod_id: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            mod_id: mod_id.into(),
            file_id: None,
            part_id: None,
            part_name: None,
            part_category: None,
        }
    }

    /// The same reference with an exact file.
    pub fn with_file(mut self, file_id: impl Into<String>) -> Self {
        self.file_id = Some(file_id.into());
        self
    }

    /// The same reference with the stable part of the page.
    pub fn with_part(mut self, part_id: impl Into<String>) -> Self {
        self.part_id = Some(part_id.into());
        self
    }

    /// The same reference with the part's display name and category.
    pub fn with_part_meta(
        mut self,
        name: Option<impl Into<String>>,
        category: Option<impl Into<String>>,
    ) -> Self {
        self.part_name = name.map(Into::into);
        self.part_category = category.map(Into::into);
        self
    }

    /// Whether both references name the same provider file: the same provider, mod and file id, with the part id
    /// standing in for a missing file id. The part's display name and category do not change the file.
    ///
    /// This answers "the same provider file?", the question the download queue's dedup and `free_download_name` ask.
    /// It is not "the same mod?" (`crate::domain::pack::PackMod::is_same_mod`) nor "which library family?" (
    /// `crate::domain::library::LibraryView::find`); those are the other two owners.
    pub fn same_file(&self, other: &Self) -> bool {
        self.provider == other.provider
            && self.mod_id == other.mod_id
            && self.file_id == other.file_id
            && (self.file_id.is_some() || self.part_id == other.part_id)
    }
}

/// One mod on a provider, as a family stores it: the id, its part and the page metadata when it was read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct SourceMod {
    pub mod_id: String,
    /// The stable part of the page, when the provider has more than one installable file group.
    pub part_id: Option<String>,
    /// The part's display name, when the provider named it.
    pub part_name: Option<String>,
    /// The provider's category for the part, for example `main` or `optional`.
    pub part_category: Option<String>,
    pub meta: Option<SourceMeta>,
}

impl SourceMod {
    /// The reference to this mod, without a file.
    pub fn source(&self, provider: &ProviderId) -> SourceRef {
        let mut source = SourceRef::new(provider.clone(), self.mod_id.clone());
        source.part_id.clone_from(&self.part_id);
        source.part_name.clone_from(&self.part_name);
        source.part_category.clone_from(&self.part_category);
        source
    }

    /// Fills the part fields this record lacks. Each field is independent: a record with a part id but no display name
    /// still takes the name. Returns whether anything changed.
    pub fn fill_missing_part(
        &mut self,
        part_id: Option<&str>,
        name: Option<&str>,
        category: Option<&str>,
    ) -> bool {
        let mut changed = self.part_id.is_none() && part_id.is_some();

        if changed {
            self.part_id = part_id.map(str::to_string);
        }

        let name_changed = self.part_name.is_none() && name.is_some();

        if name_changed {
            self.part_name = name.map(str::to_string);
        }

        let category_changed = self.part_category.is_none() && category.is_some();

        if category_changed {
            self.part_category = category.map(str::to_string);
        }

        changed |= name_changed || category_changed;
        changed
    }

    /// Fills every field this record lacks: the mod id, the part and the page metadata. The user's values stay.
    pub fn adopt_missing(&mut self, other: &Self) -> bool {
        let mut changed = self.mod_id.is_empty() && !other.mod_id.is_empty();

        if changed {
            self.mod_id.clone_from(&other.mod_id);
        }

        changed |= self.fill_missing_part(
            other.part_id.as_deref(),
            other.part_name.as_deref(),
            other.part_category.as_deref(),
        );

        if self.meta.is_none() && other.meta.is_some() {
            self.meta.clone_from(&other.meta);
            changed = true;
        }

        changed
    }

    /// Sets the part fields, replacing what the record had. `None` clears a field.
    pub fn set_part_fields(
        &mut self,
        part_id: Option<&str>,
        name: Option<&str>,
        category: Option<&str>,
    ) {
        self.part_id = part_id.map(str::to_string);
        self.part_name = name.map(str::to_string);
        self.part_category = category.map(str::to_string);
    }
}

/// What the requirements of a provider's newest file were read from, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourceSync {
    pub file_id: String,
    pub version_id: String,
    pub checked_at: u32,
    #[serde(default)]
    pub mod_level: bool,
    #[serde(default)]
    pub older_version: Option<String>,
}

/// A mod on another provider that this mod needs. A missing one is a warning, not a block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourceRequirement {
    pub source: SourceRef,
    /// The mod's name, as the user typed it or as the provider lists it.
    pub name: String,
    /// The provider lists this requirement for the mod's newest file, so the user cannot remove it.
    #[serde(default)]
    pub from_source: bool,
    /// Requirements that share a group are alternatives: one of them is enough.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
    /// The user chose to ignore this requirement.
    #[serde(default)]
    pub ignored: bool,
}

/// A parsed deep link. The download key is the short-lived key a provider puts in the link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SourceLink {
    pub source: SourceRef,
    /// The provider's game key, for example the Nexus domain.
    pub game_key: Option<String>,
    pub download_key: Option<String>,
    pub expires: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_metadata_does_not_change_the_file_identity() {
        let link = SourceRef::new("nexus", "225").with_file("7");
        let stored = SourceRef::new("nexus", "225")
            .with_file("7")
            .with_part("100")
            .with_part_meta(Some("Addon"), Some("main"));
        assert!(link.same_file(&stored));
        assert!(!link.same_file(&SourceRef::new("nexus", "225").with_file("8")));
    }
}
