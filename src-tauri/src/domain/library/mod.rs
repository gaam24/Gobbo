//! The library data model: the families, versions and archive copies the index stores, and the read-only view built
//! from them. The disk lives in `context::library`; the features that change the library live in `library`.

pub mod model;
pub mod view;

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::layout::LayoutProfile;

pub use model::{
    AdoptPolicy, Archive, ArchiveLabel, Family, Integrity, ModInfo, ModInfoEdit, ModVersion,
    SkipReason, SkippedFile, adopt_fields,
};
pub use view::LibraryView;

/// How the library index and the folders in `library/` disagree.
pub struct FolderCheck {
    /// Folders the index has and the library folder does not.
    pub missing: Vec<String>,
    /// Folders in the library folder that the index does not know.
    pub unindexed: Vec<String>,
    /// Every folder in the library folder, for the checks of a profile's list.
    pub on_disk: rustc_hash::FxHashSet<String>,
}

/// The library index: every family, every installed version and every archive copy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    /// The on-disk format. An older file is read as the current version; a newer one is refused.
    #[serde(default = "crate::domain::state_schema")]
    pub schema_version: u32,
    /// Mods by family id.
    pub families: BTreeMap<String, Family>,
    /// Versions by library folder.
    pub versions: BTreeMap<String, ModVersion>,
    /// Archives by SHA-256 sum.
    pub archives: BTreeMap<String, Archive>,
    /// Per version, the files a summary can materialize. It depends on the game rules, so it is built on load and
    /// after a scan, and never written to `library.json`.
    #[serde(skip)]
    pub(crate) layouts: rustc_hash::FxHashMap<String, LayoutProfile>,
}

impl Default for Library {
    fn default() -> Self {
        Self {
            schema_version: crate::domain::STATE_SCHEMA,
            families: BTreeMap::new(),
            versions: BTreeMap::new(),
            archives: BTreeMap::new(),
            layouts: rustc_hash::FxHashMap::default(),
        }
    }
}

impl PartialEq for Library {
    fn eq(&self, other: &Self) -> bool {
        self.families == other.families
            && self.versions == other.versions
            && self.archives == other.archives
    }
}

impl Eq for Library {}

impl Library {
    /// The version string shown by the picker: the value from `info.json`, or the one parsed from the archive name.
    /// The common case borrows the string from the index instead of cloning it for every sort key.
    pub(crate) fn version_string<'v>(&self, version: &'v ModVersion) -> Option<Cow<'v, str>> {
        if let Some(text) = version.info.version.as_deref() {
            return Some(Cow::Borrowed(text));
        }

        self.archive_label_of(version)
            .map(|label| Cow::Owned(label.version.clone()))
    }

    /// The name and version a provider read from the archive file name of a version, when one did.
    pub fn archive_label_of(&self, version: &ModVersion) -> Option<&ArchiveLabel> {
        let archive = self.archives.get(version.archive.as_deref()?)?;
        archive.named.as_ref()
    }
}
