//! The game as the features read it: the data folder of each game and the loads that every view starts with.

use super::library;
use super::settings::CONFIG_FILE;
use crate::domain::profiles;
use crate::error::AppError;
use crate::games::{self, GameDef};
use crate::infra::store;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod cache;
mod deployment;
mod edit;
mod load;
mod save;

#[cfg(test)]
mod tests;

pub use deployment::DeploymentStates;
pub use edit::{edit_family, edit_library, edit_library_clean, edit_profiles, idle_data};
pub use load::{
    Loaded, load_game, load_library, load_library_shared, load_profiles, load_profiles_shared,
    load_stopped, open,
};

use cache::{Caches, cached};

pub use crate::infra::deployment::{DEPLOYMENT_FILE, JOURNAL_FILE};

/// The Gobbo data folder. Each game has its own subfolder in it.
#[derive(Clone)]
pub struct Data {
    pub dir: PathBuf,
    /// The app data root this handle was opened from. A game handle keeps it so paths stored at the root, such as the
    /// Proton prefix Gobbo owns, resolve the same way from a game handle and from the root handle.
    root: Option<PathBuf>,
    /// The app's read-through caches; a test or a direct `Data::new` has none. Every game handle shares them.
    caches: Option<Arc<Caches>>,
}

impl Data {
    /// A data folder handle with no read-through caches, for a test or a direct caller.
    pub const fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            root: None,
            caches: None,
        }
    }

    /// The app's data folder. Reading the library reuses the parsed index until the file changes.
    pub fn with_cache(dir: PathBuf) -> Self {
        Self {
            caches: Some(Arc::default()),
            ..Self::new(dir)
        }
    }

    /// The app data root, whether this is the root handle or one of its game handles.
    pub fn root(&self) -> &Path {
        self.root.as_deref().unwrap_or(&self.dir)
    }

    /// The definition of game `id`, read and checked again only when its `game.toml` changed. A loader preset is read
    /// at the same time; a change to a preset needs a restart, as shipped presets do not change while the app runs. The
    /// value is owned, for a caller that changes it; a read-only caller uses `definition_shared`.
    pub fn definition(&self, id: &str) -> Result<GameDef, AppError> {
        self.definition_shared(id).map(|game| (*game).clone())
    }

    /// The definition of game `id` as the shared handle: the process cache gives every reader the same value until
    /// `game.toml` changes.
    #[expect(
        clippy::unused_self,
        reason = "the definition cache is process-wide, not per handle, but the handle is the API the callers use"
    )]
    pub fn definition_shared(&self, id: &str) -> Result<Arc<GameDef>, AppError> {
        games::load_shared(id)
    }

    /// The data folder as text, for the settings page: the app settings and every game's folder live in it.
    pub fn folder_text(&self) -> String {
        self.dir.display().to_string()
    }

    /// The data folder of one game: library, archives, profiles, deployment state and settings. The index cache is
    /// shared with the handle it came from, so every game uses the same one-entry cache.
    pub fn game(&self, id: &str) -> Self {
        Self {
            dir: self.dir.join(crate::migrations::GAMES_DIR).join(id),
            root: Some(self.root().to_path_buf()),
            caches: self.caches.clone(),
        }
    }

    /// The library folder, which holds the installed mod folders.
    pub fn library(&self) -> PathBuf {
        self.dir.join("library")
    }

    /// The archive copies of the installed versions.
    pub fn archives(&self) -> PathBuf {
        self.dir.join(store::ARCHIVES_DIR)
    }

    /// The downloads waiting for their import, next to the game's data.
    pub fn downloads(&self) -> PathBuf {
        self.dir.join("downloads")
    }

    /// The cached collection recipes and their archives.
    pub fn collections(&self) -> PathBuf {
        self.dir.join("collections")
    }

    /// The cached pictures of mod pages.
    pub fn images(&self) -> PathBuf {
        self.dir.join("images")
    }

    /// The library index file, `library.json`.
    pub fn index(&self) -> PathBuf {
        self.dir.join(library::INDEX_FILE)
    }

    /// The profile list file, `profiles.json`.
    pub fn profiles_file(&self) -> PathBuf {
        self.dir.join(profiles::PROFILES_FILE)
    }

    /// The deployment state of one profile: the journal and the overlay layer. Each profile has its own folder, so
    /// switching profiles does not delete another profile's layer.
    pub fn profile_state(&self, profile: &str) -> PathBuf {
        self.dir.join("state").join(profile)
    }

    /// Id of the profile that is active in `profiles.json`, from the shared cache when the file did not change.
    pub fn active_profile_id(&self) -> String {
        load_profiles_shared(self)
            .map_or_else(|_| "p1".to_string(), |profiles| profiles.active.clone())
    }

    /// The game's settings file, `config.json`.
    pub fn config(&self) -> PathBuf {
        self.dir.join(CONFIG_FILE)
    }
}
