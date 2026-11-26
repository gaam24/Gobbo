//! The values an import carries from planning to extraction and placing.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::domain::library::{Integrity, SkippedFile};

/// A mod saved in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The folder in the library, for example `modSortEverything`.
    pub folder: String,
    /// The slot (its folder), empty for a mod that only mirrors game paths.
    pub slot: String,
    pub files: usize,
    /// Files relative to the library folder, in the order they were extracted. The caller can build the index
    /// record from these instead of walking the new folder again.
    pub paths: Vec<String>,
    /// Sum of the uncompressed sizes of the files.
    pub size_bytes: u64,
    /// Checksum of the extracted files, taken before the folder moved into the library.
    pub integrity: Option<Integrity>,
}

/// The mods an import placed in the library and the entries it skipped.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Import {
    pub mods: Vec<Installed>,
    /// Entries no rule takes, for example a readme in the root.
    pub skipped: usize,
    /// Every entry that was not unpacked, with its reason.
    pub skipped_files: Vec<SkippedFile>,
}

/// An import failed. Nothing is saved when this is returned.
pub(in crate::import) struct Placed {
    pub(in crate::import) index: usize,
    /// The mod inside the archive the entry belongs to: the name of its folder in the archive, or the archive's own
    /// name for files that belong to the archive as a whole. Each group becomes one library folder.
    pub(in crate::import) group: String,
    /// The slot (its folder). Empty for a file that mirrors a game path.
    pub(in crate::import) slot: String,
    /// Path relative to the library folder: inside the mod for a slot, the game path for a mirror.
    pub(in crate::import) rel: String,
}

/// What the library folders of an archive's mods are called.
#[derive(Debug, Clone, Copy)]
pub struct Naming<'a> {
    /// The archive copy's stem. A mod that is alone in the archive takes it.
    pub base_name: &'a str,
    /// SHA-256 sum of the archive. When the name is taken, the start of the digest goes into it.
    pub digest: &'a str,
    /// A reinstall: only the mod that is the library folder `only` is unpacked, under that name.
    pub only: Option<&'a str>,
}

/// The entries of an archive and the mod each one belongs to. Nothing is written yet.
pub struct Plan {
    pub(in crate::import) placed: Vec<Placed>,
    /// A file to write into a mod folder that a slot's `enable_file` names, by group. It is not an archive entry.
    pub(in crate::import) enable_files: BTreeMap<String, String>,
    /// Entries no rule takes, for example a readme in the root.
    pub(in crate::import) skipped: usize,
    /// Every entry that was not unpacked, with its reason.
    pub(in crate::import) skipped_files: Vec<SkippedFile>,
}

/// The mods of an archive, extracted into a working folder: mod `i` is in `dir/<i>` and waits for its library folder.
pub struct Staged {
    pub(in crate::import) dir: PathBuf,
    pub(in crate::import) mods: Vec<StagedMod>,
    pub(in crate::import) skipped: usize,
    pub(in crate::import) skipped_files: Vec<SkippedFile>,
}

/// A mod in the working folder.
pub(in crate::import) struct StagedMod {
    /// The mod inside the archive: the name of its folder in the archive, or the archive's own name for the files that
    /// belong to it as a whole.
    pub(in crate::import) group: String,
    pub(in crate::import) slot: String,
    pub(in crate::import) files: usize,
    pub(in crate::import) paths: Vec<String>,
    pub(in crate::import) size_bytes: u64,
    pub(in crate::import) integrity: Option<Integrity>,
}
