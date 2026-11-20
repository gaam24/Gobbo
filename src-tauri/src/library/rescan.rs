//! Scanning the library folder again and syncing the profiles with what the index found.

use crate::context::Data;
use crate::context::load_profiles;
use crate::context::profile::lock_profiles;
use crate::error::AppError;
use crate::library::LibraryView;
use crate::library::sync::sync_profiles;

use crate::context::edit_library_clean;

/// Scans the library folder again: adds versions that the index does not know, removes those that are gone,
/// falls back to another version when a profile selection disappeared, and cleans up unused archive copies.
pub fn rescan(root: &Data, game_id: &str) -> Result<(), AppError> {
    edit_library_clean(root, game_id, |data, game, library| {
        library.rescan(&data.library(), game)?;
        let view = LibraryView::new(library);
        let _profiles = lock_profiles();
        let mut profiles = load_profiles(data)?;

        if sync_profiles(&mut profiles, library, &view) {
            data.save_profiles(&profiles)?;
        }

        Ok(())
    })
}
