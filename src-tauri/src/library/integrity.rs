//! Checking an installed mod against the checksum recorded when it was imported.

use serde::Serialize;

use crate::context::{Data, load_library};
use crate::domain::naming::is_plain_folder;
use crate::error::{AppError, io_at};
use crate::library::{Integrity, lock_index, not_in_library};

/// What one verification found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub enum IntegrityStatus {
    /// The installed files still match the checksum from the import.
    Matches,
    /// At least one file changed, was added or was removed since the import.
    Changed,
    /// The version had no checksum yet; the current files are now the baseline.
    Recorded,
}

/// Compares one installed version with its recorded checksum. A version imported before checksums existed, or found by
/// a rescan, gets a baseline on the first check, so older libraries can use the same verification. The index lock is
/// held from the read to the optional save; hashing happens before it is taken.
pub fn verify(root: &Data, game_id: &str, folder: &str) -> Result<IntegrityStatus, AppError> {
    if !is_plain_folder(folder) {
        return Err(AppError::user("library.invalid_folder").with("folder", folder));
    }

    let game = root.definition(game_id)?;
    let data = root.game(game_id);
    let path = data.library().join(folder);
    let (digest, files, bytes) = crate::infra::fs::hash_tree(&path).map_err(io_at(&path))?;
    let current = Integrity {
        digest,
        files,
        bytes,
    };

    let _index = lock_index();
    let mut library = load_library(&data, &game)?;
    let Some(version) = library.versions.get_mut(folder) else {
        return Err(not_in_library(folder));
    };

    match &version.integrity {
        Some(baseline) if *baseline == current => Ok(IntegrityStatus::Matches),
        Some(_) => Ok(IntegrityStatus::Changed),
        None => {
            version.integrity = Some(current);
            data.save_library(library)?;
            Ok(IntegrityStatus::Recorded)
        }
    }
}
