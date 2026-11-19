//! The library index as the shared context reads it: loading and saving `library.json`, scanning the library folder,
//! the folder helpers and the index lock. The pure data model and its view live in `domain::library`; the features that
//! change the library live in `library`.

mod folders;
mod index;
mod lock;

pub use crate::domain::library::{FolderCheck, Library};
pub use folders::{
    family_id_from, mod_folders, mod_info, page_id_of, scan_mod, version_from_import,
};
pub use index::ArchiveLabels;
#[cfg(test)]
pub use lock::index_is_held;
pub use lock::lock_index;

use crate::error::{AppError, ErrorKind, MOD_MISSING};

pub const INDEX_FILE: &str = "library.json";

/// A mod family that is not in the library index. The interface should not offer it, but a stale list can.
pub fn not_in_library(id: &str) -> AppError {
    AppError::new(MOD_MISSING)
        .with("folder", id)
        .kind(ErrorKind::Permanent)
}
