//! Nexus Mods records used by the adapter and its update engine. They are plain data: the API client fills them, and
//! the conversions below map them to the provider-neutral types in `domain::source`. The rest of the app stores only
//! those generic types.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::ID;
use crate::domain::source::{
    ProviderId, SourceMeta, SourceRef, SourceRequirement, SourceStats, SourceSync,
};
use crate::error::AppError;

/// Data from the mod's page on Nexus Mods, as the API returns it. The update engine maps it to `SourceMeta`; the
/// version itself stays in `info`, because it says what is installed, not what the page shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct NexusMeta {
    pub name: Option<String>,
    /// Who made the mod, as the page credits them.
    pub author: Option<String>,
    pub uploader: Option<String>,
    pub category: Option<String>,
    /// One line about the mod.
    pub summary: Option<String>,
    /// The page's description as plain text.
    pub description: Option<String>,
    /// The page's main picture on Nexus, and the copy of it in the game's `images/` folder (a file name).
    pub picture_url: Option<String>,
    pub picture: Option<String>,
    pub endorsements: Option<u32>,
    pub downloads: Option<u32>,
    /// The page is marked as adult content: the panel hides its picture until asked.
    pub adult: bool,
    /// Unix time in seconds of the last successful check. It fits in `u32` until 2106.
    pub checked_at: Option<u32>,
    /// Unix time in seconds of the page's last update and of its creation on Nexus.
    pub updated_at: Option<u32>,
    pub created_at: Option<u32>,
    pub unique_downloads: Option<u32>,
    /// The page's status on Nexus: `published`, `hidden`, `removed` and so on. Only `published` is a live page.
    pub status: Option<String>,
    /// The tags the author put on the page, as Nexus names them.
    pub tags: Vec<String>,
}

/// A file on Nexus Mods: the mod, the exact file version when it is known, and the stable mod file group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct NexusSource {
    pub mod_id: u32,
    pub file_id: Option<u32>,
    /// The persistent `mod_file_id`, the update chain of one installable file.
    pub part_id: Option<u32>,
}

/// A mod on Nexus Mods that another mod needs. It may not be in the library; then the requirement is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct NexusRequirement {
    pub mod_id: u32,
    /// The mod's name, as the user typed it. Shown while the mod is not in the library.
    pub name: String,
    /// Nexus lists this mod as needed by the newest downloaded file of the mod. Each update check fills these in, and
    /// the user cannot remove them.
    #[serde(default)]
    pub from_nexus: bool,
    /// The requirements that share a group are alternatives: one of them is enough. Groups are numbered per file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
    /// The user chose to ignore this requirement. It stays in the list but is not counted and produces no warning.
    #[serde(default)]
    pub ignored: bool,
}

/// What the requirements of a family's newest file were read from, and when. A read of the same file within a week
/// is not repeated, and the file version's id saves the lookup of that version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct NexusRequiresSync {
    pub file_id: u32,
    pub version_id: String,
    /// Unix time in seconds of the read.
    pub checked_at: u32,
    /// Whether the read included the requirements the mod lists for itself, not only its file's. A read made before
    /// that was added is repeated.
    #[serde(default)]
    pub mod_level: bool,
    /// The version of an older file the list came from, when the newest file lists none.
    #[serde(default)]
    pub older_version: Option<String>,
}

/// The same reference as a Nexus source, when the provider is Nexus and the ids are numbers. The mapping belongs to
/// this adapter, not to the provider-neutral `SourceRef`.
pub(super) fn source_of(source: &SourceRef) -> Option<NexusSource> {
    if source.provider.as_str() != ID {
        return None;
    }

    Some(NexusSource {
        mod_id: source.mod_id.parse().ok()?,
        file_id: source.file_id.as_deref().and_then(|id| id.parse().ok()),
        part_id: source.part_id.as_deref().and_then(|id| id.parse().ok()),
    })
}

impl TryFrom<SourceRef> for NexusSource {
    type Error = AppError;

    fn try_from(source: SourceRef) -> Result<Self, Self::Error> {
        source_of(&source).ok_or_else(|| AppError::unexpected("the source is not a Nexus mod"))
    }
}

impl From<NexusSource> for SourceRef {
    fn from(source: NexusSource) -> Self {
        Self {
            provider: ProviderId::new(ID),
            mod_id: source.mod_id.to_string(),
            file_id: source.file_id.map(|id| id.to_string()),
            part_id: source.part_id.map(|id| id.to_string()),
            part_name: None,
            part_category: None,
        }
    }
}

impl From<NexusRequiresSync> for SourceSync {
    fn from(sync: NexusRequiresSync) -> Self {
        Self {
            file_id: sync.file_id.to_string(),
            version_id: sync.version_id,
            checked_at: sync.checked_at,
            mod_level: sync.mod_level,
            older_version: sync.older_version,
        }
    }
}

impl TryFrom<SourceSync> for NexusRequiresSync {
    type Error = AppError;

    fn try_from(sync: SourceSync) -> Result<Self, Self::Error> {
        Ok(Self {
            file_id: sync
                .file_id
                .parse()
                .map_err(|_| AppError::unexpected("the sync is not a Nexus file"))?,
            version_id: sync.version_id,
            checked_at: sync.checked_at,
            mod_level: sync.mod_level,
            older_version: sync.older_version,
        })
    }
}

impl From<NexusMeta> for SourceMeta {
    fn from(meta: NexusMeta) -> Self {
        Self {
            name: meta.name,
            author: meta.author,
            uploader: meta.uploader,
            category: meta.category,
            summary: meta.summary,
            description: meta.description,
            picture_url: meta.picture_url,
            picture: meta.picture,
            version: None,
            stats: SourceStats {
                downloads: meta.downloads,
                unique_downloads: meta.unique_downloads,
                endorsements: meta.endorsements,
                subscribers: None,
            },
            adult: meta.adult,
            checked_at: meta.checked_at,
            updated_at: meta.updated_at,
            created_at: meta.created_at,
            status: meta.status,
            tags: meta.tags,
            extra: BTreeMap::new(),
        }
    }
}

impl From<NexusRequirement> for SourceRequirement {
    fn from(requirement: NexusRequirement) -> Self {
        Self {
            source: SourceRef::new(ID, requirement.mod_id.to_string()),
            name: requirement.name,
            from_source: requirement.from_nexus,
            group: requirement.group,
            ignored: requirement.ignored,
        }
    }
}

impl TryFrom<SourceRequirement> for NexusRequirement {
    type Error = AppError;

    fn try_from(requirement: SourceRequirement) -> Result<Self, Self::Error> {
        let mod_id = requirement
            .source
            .mod_id
            .parse()
            .map_err(|_| AppError::unexpected("the requirement is not a Nexus mod"))?;
        if requirement.source.provider.as_str() != ID {
            return Err(AppError::unexpected("the requirement is not a Nexus mod"));
        }

        Ok(Self {
            mod_id,
            name: requirement.name,
            from_nexus: requirement.from_source,
            group: requirement.group,
            ignored: requirement.ignored,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nexus_source_round_trips_through_the_generic_reference() {
        let nexus = NexusSource {
            mod_id: 7001,
            file_id: Some(42),
            part_id: Some(7),
        };
        let source = SourceRef::from(nexus);
        assert_eq!(source.provider.as_str(), "nexus");
        assert_eq!(source.mod_id, "7001");
        assert_eq!(source.file_id.as_deref(), Some("42"));
        assert_eq!(source.part_id.as_deref(), Some("7"));
        assert_eq!(NexusSource::try_from(source).unwrap(), nexus);
    }

    #[test]
    fn a_generic_reference_that_is_not_nexus_cannot_become_a_nexus_source() {
        let source = SourceRef::new("modio", "123");
        assert!(NexusSource::try_from(source).is_err());
    }

    #[test]
    fn nexus_page_data_becomes_common_metadata() {
        let meta = NexusMeta {
            name: Some("Example".to_string()),
            downloads: Some(10),
            endorsements: Some(3),
            tags: vec!["tag".to_string()],
            ..NexusMeta::default()
        };
        let source = SourceMeta::from(meta);
        assert_eq!(source.name.as_deref(), Some("Example"));
        assert_eq!(source.stats.downloads, Some(10));
        assert_eq!(source.stats.endorsements, Some(3));
        assert_eq!(source.tags, ["tag"]);
    }

    #[test]
    fn a_nexus_requirement_becomes_a_generic_requirement() {
        let requirement = NexusRequirement {
            mod_id: 7,
            name: "Required".to_string(),
            from_nexus: true,
            group: Some(1),
            ignored: false,
        };
        let source = SourceRequirement::from(requirement);
        assert_eq!(source.source.provider.as_str(), "nexus");
        assert_eq!(source.source.mod_id, "7");
        assert_eq!(source.group, Some(1));
        assert_eq!(NexusRequirement::try_from(source).unwrap().mod_id, 7);
    }
}
