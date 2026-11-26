//! The errors of an import: an archive that cannot be read, a layout that no rule takes, and the files of the library.

use crate::error::{AppError, ErrorKind, FileError};
use crate::import::{ALREADY_IN_LIBRARY, IMPORT_CANCELLED};
use crate::infra::archive;

/// Why an import could not place an archive in the library.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("archive: {0}")]
    Archive(#[from] archive::Error),
    #[error(transparent)]
    Io(#[from] FileError),
    /// The archive writes into a protected game folder, for example `bin/`. Nothing was saved.
    #[error("the archive contains {root}/, and that folder overwrites game files; import rejected")]
    UnsupportedRoot { root: String },
    /// No entry matches a rule of the game. Nothing was saved.
    #[error("the archive layout is not recognised for this game")]
    UnknownLayout,
    /// An entry leaves the library (`..` or an absolute path). Nothing was saved.
    #[error("unsafe path in the archive: {name}")]
    UnsafeEntry { name: String },
    /// The folder is already in the library. Nothing was saved.
    #[error("mod {folder} is already in the library")]
    AlreadyInLibrary { folder: String },
    /// The archive would write more than the import limit. Nothing was saved.
    #[error("the archive expands to more than {limit} bytes")]
    TooLarge { limit: u64 },
    /// The archive holds more files than the import limit. Nothing was saved.
    #[error("the archive holds more than {limit} files")]
    TooManyFiles { limit: usize },
    /// The library folder does not have enough free space. Nothing was saved.
    #[error("the archive needs {needed_kb} KB, but only {free_kb} KB is free")]
    NoSpace { needed_kb: u64, free_kb: u64 },
    /// The user stopped the import. Nothing was committed.
    #[error("the import was cancelled")]
    Cancelled,
}

impl From<InstallError> for AppError {
    fn from(error: InstallError) -> Self {
        match error {
            InstallError::Archive(e) => {
                // A limit error carries the actual value and the limit, so it keeps its own code and text.
                if matches!(
                    &e,
                    archive::Error::TooManyEntries { .. } | archive::Error::TooLarge { .. }
                ) {
                    e.into()
                } else {
                    Self::new("install.bad_archive")
                        .with("detail", e)
                        .kind(ErrorKind::Permanent)
                }
            }
            InstallError::Io(file) => file.into(),
            InstallError::UnsupportedRoot { root } => Self::new("install.unsupported_root")
                .with("root", root)
                .kind(ErrorKind::Permanent),
            InstallError::UnknownLayout => {
                Self::new("install.unknown_layout").kind(ErrorKind::Permanent)
            }
            InstallError::UnsafeEntry { name } => Self::new("install.unsafe_entry")
                .with("name", name)
                .kind(ErrorKind::Permanent),
            InstallError::AlreadyInLibrary { folder } => Self::new(ALREADY_IN_LIBRARY)
                .with("folder", folder)
                .kind(ErrorKind::UserAction),
            InstallError::TooLarge { limit } => Self::new("install.too_large")
                .with("limit", limit)
                .kind(ErrorKind::Permanent),
            InstallError::TooManyFiles { limit } => Self::new("install.too_many_files")
                .with("limit", limit)
                .kind(ErrorKind::Permanent),
            InstallError::NoSpace { needed_kb, free_kb } => Self::new("install.no_space")
                .with("needed", needed_kb)
                .with("free", free_kb)
                .kind(ErrorKind::UserAction),
            InstallError::Cancelled => Self::user(IMPORT_CANCELLED),
        }
    }
}
