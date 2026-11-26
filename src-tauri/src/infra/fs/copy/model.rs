//! Copy modes and the errors a tree copy can report.

use std::io;
use std::path::PathBuf;

/// What `copy_tree` does with a target that already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyMode {
    /// Replace the target whole. The new tree is staged first and the old target is restored on failure; on success the
    /// previous generation is removed, so a user folder is not left with a `.old` sibling.
    Replace,
    /// Replace the target and keep one previous generation as `<name>.old`. Used for a profile store, where the
    /// previous copy is the recovery point.
    ReplaceKeepPrevious,
    /// Keep existing target entries and create only the missing ones.
    FillMissing,
    /// Copy the source over the target: a same-named file is overwritten and a target entry the source does not name
    /// stays. A path that is a file on one side and a folder on the other stops the copy with a [`MergeConflict`].
    #[cfg_attr(
        not(windows),
        allow(
            dead_code,
            reason = "moving profile data back is a Windows overlay path; the tests still cover the merge"
        )
    )]
    Merge,
}

/// Whether a path is a file or a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Folder,
}

impl std::fmt::Display for EntryKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::File => "file",
            Self::Folder => "folder",
        })
    }
}

/// A merge stopped because the source and the target disagree on whether a path is a file or a folder. Nothing is
/// deleted to make the two sides fit.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "cannot copy a {} over a {} at {}",
    source_kind,
    target_kind,
    path.display()
)]
pub struct MergeConflict {
    /// The path inside the target that has the other kind.
    pub path: PathBuf,
    /// What the source has at the path.
    pub source_kind: EntryKind,
    /// What the target has at the path.
    pub target_kind: EntryKind,
}

/// Why a tree copy failed.
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    /// The file system refused a read or a write.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// A merge would have to replace a file with a folder or a folder with a file.
    #[error(transparent)]
    Conflict(#[from] MergeConflict),
}

impl From<CopyError> for io::Error {
    fn from(error: CopyError) -> Self {
        match error {
            CopyError::Io(error) => error,
            CopyError::Conflict(conflict) => io::Error::new(io::ErrorKind::AlreadyExists, conflict),
        }
    }
}
