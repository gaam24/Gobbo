//! The saves that keep the read-through caches warm with the value just written.

use super::{Data, library::Library};
use crate::domain::profiles::Profiles;
use crate::error::AppError;
use std::sync::Arc;

impl Data {
    /// Saves `profiles.json` and keeps the read-through cache warm with what was written. Every save goes through here:
    /// the cache knows a file by its length and modification time, and two quick saves of the same length could share
    /// both, so the value written must replace the cached one.
    pub fn save_profiles(&self, profiles: &Profiles) -> Result<(), AppError> {
        let path = self.profiles_file();
        profiles.save(&path)?;

        if let Some(cache) = self.profiles_cache() {
            cache.store(&path, Arc::new(profiles.clone()));
        }

        Ok(())
    }

    /// Saves the index and keeps the read-through cache warm, so the status after an import, an edit or a deployment
    /// does not parse the file that was just written. The library moves into the cache instead of being cloned.
    pub fn save_library(&self, mut library: Library) -> Result<Arc<Library>, AppError> {
        let index = self.index();
        library.prune_layouts();
        library.save(&index)?;
        let library = Arc::new(library);

        if let Some(cache) = self.library_cache() {
            cache.store(&index, Arc::clone(&library));
        }

        Ok(library)
    }
}
