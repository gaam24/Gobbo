//! The data a mod pack keeps for one mod. The profile stores a mod of a pack while the library lacks it, so this type
//! lives in the domain; the pack reader and writer in `packs` fill and consume it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::fomod::Choice;
use crate::domain::mod_steps::Phases;
use crate::domain::source::{ProviderId, SourceMod, SourceRequirement};

/// A mod of a pack: what the author's library had, plus the choices and data that travel with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackMod {
    pub(crate) name: String,
    pub(crate) version: Option<String>,
    /// The family id in the author's library. Relations between mods of the pack use these ids.
    pub(crate) family: String,
    pub(crate) enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) priority: Option<u32>,
    /// The mod on each provider, with the page metadata the author had. The receiver's own check brings the picture
    /// and sets the check time.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) sources: BTreeMap<ProviderId, SourceMod>,
    /// The exact file the author installed, per provider.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) source_files: BTreeMap<ProviderId, String>,
    /// The persistent part the installed file belongs to, per provider.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) source_parts: BTreeMap<ProviderId, String>,
    /// The part's display name, when the author's provider named it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) part_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) archive: Option<PackArchive>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) fomod: Vec<Choice>,
    /// The archive has a FOMOD installer; `fomod` may then be empty when there was nothing to pick.
    #[serde(default)]
    pub(crate) fomod_installer: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) tags: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) instructions: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) requires: Vec<String>,
    /// Mods on external providers this one needs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) requirements: Vec<SourceRequirement>,
    /// The DLCs each provider lists for the requirements above, one list per requirement.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) source_dlcs: BTreeMap<ProviderId, Vec<Vec<String>>>,
    /// The author's note about the mod.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) incompatible: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) overrides: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) steps: Option<Phases>,
}

impl PackMod {
    /// The reference to the mod on one provider, with the file id the pack carries.
    pub fn source_ref(&self, provider: &ProviderId) -> Option<crate::domain::source::SourceRef> {
        let source = self.sources.get(provider)?;
        let mut reference = source.source(provider);

        if let Some(part_id) = self.source_parts.get(provider) {
            reference = reference.with_part(part_id.clone());
        }

        if let Some(file_id) = self.source_files.get(provider) {
            reference = reference.with_file(file_id.clone());
        }

        Some(reference)
    }

    /// The source the pack's archive most likely came from: the first provider with a file id, else the first one.
    pub fn archive_source(&self) -> Option<crate::domain::source::SourceRef> {
        self.source_files
            .keys()
            .find_map(|provider| self.source_ref(provider))
            .or_else(|| {
                self.sources
                    .keys()
                    .find_map(|provider| self.source_ref(provider))
            })
    }

    /// The FOMOD choices to import the mod's archive with, by the same rule as `ModVersion::installer_choices`.
    pub fn installer_choices(&self) -> Option<&[Choice]> {
        (self.fomod_installer || !self.fomod.is_empty()).then_some(self.fomod.as_slice())
    }

    /// Whether the mod names any external source.
    pub fn has_source(&self) -> bool {
        !self.sources.is_empty()
    }

    /// Whether two descriptions stand for the same mod, for a waiting entry against a pack mod or a collection member.
    /// This answers "the same mod?": when both name an exact file of a shared provider, the file decides — two files of
    /// one page are two mods. Else the same family key, the same page and part, or (for mods without a source) the same
    /// name decides. A shared display name never matches two mods that both have a source.
    ///
    /// The other two questions have their own owners: "the same provider file?" is
    /// `crate::domain::source::SourceRef::same_file`, and "which library family does this wanted mod match?" is
    /// `crate::domain::library::LibraryView::find`. The requirement rule is deliberately different again: any file of a
    /// page satisfies it, so `library::metadata`'s page matcher excludes the file id.
    pub fn is_same_mod(&self, other: &Self) -> bool {
        for (provider, source) in &self.sources {
            let Some(theirs) = other.sources.get(provider) else {
                continue;
            };

            if source.mod_id != theirs.mod_id {
                continue;
            }

            if let (Some(file), Some(their_file)) = (
                self.source_files.get(provider),
                other.source_files.get(provider),
            ) {
                return file == their_file;
            }

            let part = source
                .part_id
                .as_ref()
                .or_else(|| self.source_parts.get(provider));
            let their_part = theirs
                .part_id
                .as_ref()
                .or_else(|| other.source_parts.get(provider));
            if part.is_some() && part == their_part {
                return true;
            }
        }

        self.family == other.family
            || (!(self.has_source() && other.has_source()) && self.name == other.name)
    }
}

/// The exact file a mod was installed from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackArchive {
    pub(crate) name: String,
    pub(crate) sha256: String,
    pub(crate) size: u64,
    /// The archive is inside the pack.
    #[serde(default)]
    pub(crate) bundled: bool,
}

/// A mod of the pack that the library lacked when the pack was imported. It waits in the profile with its place and
/// state, and joins the list once the library has it. `before` is the family of the mod it sits above, from the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitingMod {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) before: Option<String>,
    pub(crate) entry: PackMod,
}

impl WaitingMod {
    /// The family the pack names the mod by. Separators and the list use it for the mod while it waits.
    pub fn key(&self) -> &str {
        &self.entry.family
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(json: &str) -> PackMod {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn two_files_of_one_page_are_two_mods() {
        let first = entry(
            r#"{"name":"A","version":null,"family":"p:1","enabled":true,"sources":{"p":{"mod_id":"1"}},"source_files":{"p":"10"}}"#,
        );
        let second = entry(
            r#"{"name":"B","version":null,"family":"p:1","enabled":true,"sources":{"p":{"mod_id":"1"}},"source_files":{"p":"20"}}"#,
        );
        assert!(!first.is_same_mod(&second));
        assert!(first.is_same_mod(&first.clone()));
    }

    #[test]
    fn a_mod_without_a_file_matches_by_family_and_a_sourceless_one_by_name() {
        let known = entry(
            r#"{"name":"Example","version":null,"family":"p:1","enabled":true,"sources":{"p":{"mod_id":"1"}}}"#,
        );
        let same_family = entry(r#"{"name":"Other","version":null,"family":"p:1","enabled":true}"#);
        let same_name =
            entry(r#"{"name":"Example","version":null,"family":"local:x","enabled":true}"#);
        let stranger = entry(
            r#"{"name":"Example","version":null,"family":"p:2","enabled":true,"sources":{"p":{"mod_id":"2"}}}"#,
        );
        assert!(known.is_same_mod(&same_family));
        assert!(known.is_same_mod(&same_name));
        assert!(
            !known.is_same_mod(&stranger),
            "two sourced mods are not matched by name"
        );
    }
}
