use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::domain::fomod::Choice;
use crate::domain::pack::PackMod;
use crate::domain::source::{ProviderId, SourceMeta, SourceMod};

/// The phase Nexus uses for optional members. It sorts after every normal phase.
pub const OPTIONAL_PHASE: u32 = 666;

/// The kind of source one collection member names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollectionSource {
    /// A file of a registered provider: a Nexus file or a Thunderstore package version. The provider downloads it.
    Provider {
        provider: String,
        mod_id: String,
        file_id: String,
        /// The persistent part id (a Nexus `mod_file_id`), when the manifest already names it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        part_id: Option<String>,
    },
    /// A folder inside the collection archive, under `bundled/<path>`.
    Bundle {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tag: Option<String>,
    },
    /// A direct URL outside Nexus.
    Direct { url: String },
    /// A page the user has to visit and download from.
    Browse { url: String },
    /// Instructions only; the user installs it.
    Manual,
    /// A source type this build does not know. It is shown and refused, never installed by guesswork.
    Unsupported { source_kind: String },
}

/// How a member matches an already installed version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CollectionUpdatePolicy {
    /// The pinned file is the one the collection was made with.
    #[default]
    Exact,
    /// Any version of the same mod satisfies the member; the pinned file is downloaded when none is installed.
    Latest,
    /// Use the pinned file when it is installed, any other version otherwise.
    Prefer,
}

/// One member of a collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CollectionItem {
    /// A stable key across revisions: `<provider>:<mod>`, `bundle:<tag>`, `direct:<url>` or `manual:<name>`.
    pub key: String,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub summary: String,
    pub picture_url: Option<String>,
    pub optional: bool,
    pub phase: u32,
    pub source: CollectionSource,
    pub file_name: Option<String>,
    pub size_kb: u32,
    pub md5: Option<String>,
    pub update_policy: CollectionUpdatePolicy,
    /// The installer choices the curator made. Several entries in one group mean a multi-select group.
    pub choices: Vec<Choice>,
    /// File path -> CRC32 of the file the patch expects, as the collection stores it.
    pub patches: BTreeMap<String, String>,
    pub instructions: String,
    pub adult: bool,
}

impl CollectionItem {
    /// The family key a waiting entry uses before the library knows the real one. It is stable for one item and is
    /// enough for separators and the order of the waiting list.
    pub fn predicted_family(&self) -> String {
        match &self.source {
            CollectionSource::Provider {
                provider, mod_id, ..
            } => format!("{provider}:{mod_id}"),
            _ => self.key.clone(),
        }
    }

    /// Converts the member into the `PackMod` it wants. Both owners read it: `LibraryView::find` resolves it against the
    /// library and `PackMod::is_same_mod` matches it against a waiting entry, so a collection member is found by the same
    /// rules as a pack mod.
    pub fn wanted(&self) -> PackMod {
        let mut sources = BTreeMap::new();
        let mut source_files = BTreeMap::new();
        let mut source_parts = BTreeMap::new();
        if let CollectionSource::Provider {
            provider,
            mod_id,
            file_id,
            part_id,
        } = &self.source
        {
            let provider = ProviderId::new(provider.as_str());
            source_files.insert(provider.clone(), file_id.clone());
            if let Some(part_id) = part_id {
                source_parts.insert(provider.clone(), part_id.clone());
            }
            sources.insert(
                provider,
                SourceMod {
                    mod_id: mod_id.clone(),
                    part_id: part_id.clone(),
                    part_name: None,
                    part_category: None,
                    meta: Some(SourceMeta {
                        name: Some(self.name.clone()),
                        author: self.author.clone(),
                        category: self.category.clone(),
                        summary: Some(self.summary.clone()).filter(|text| !text.is_empty()),
                        picture_url: self.picture_url.clone(),
                        adult: self.adult,
                        ..SourceMeta::default()
                    }),
                },
            );
        }
        PackMod {
            name: self.name.clone(),
            version: self.version.clone(),
            family: self.predicted_family(),
            enabled: true,
            priority: None,
            sources,
            source_files,
            source_parts,
            part_name: None,
            archive: None,
            fomod: self.choices.clone(),
            fomod_installer: !self.choices.is_empty(),
            tags: Vec::new(),
            instructions: self.instructions.clone(),
            requires: Vec::new(),
            requirements: Vec::new(),
            source_dlcs: BTreeMap::new(),
            notes: String::new(),
            incompatible: Vec::new(),
            overrides: Vec::new(),
            steps: None,
        }
    }

    /// The provider file a member names: the provider, the mod and the file. `None` for a bundle, a URL or a manual
    /// member.
    pub fn provider_file(&self) -> Option<(&str, &str, &str)> {
        match &self.source {
            CollectionSource::Provider {
                provider,
                mod_id,
                file_id,
                ..
            } => Some((provider, mod_id, file_id)),
            _ => None,
        }
    }

    /// The source path of a bundle inside the collection archive, with `/` separators and no leading slash.
    pub fn bundle_path(&self) -> Option<String> {
        match &self.source {
            CollectionSource::Bundle { path, .. } => Some(path.trim_matches('/').to_string()),
            _ => None,
        }
    }
}
