//! The per-profile data outside the game folder: the copy list, the user-data sets, their owner files, and the state that
//! switches, captures and restores both stores together. It also keeps the list history (the kept states for undo and
//! redo) and the bisect search file. Deployment, launch, status and the profile use cases all read this.

pub mod bisect;
mod copy;
pub mod data;
pub mod history;
mod lock;
pub mod owner;
pub mod state;
mod store;
pub mod user_data;

use crate::context::Data;
use crate::context::Settings;
use crate::domain::profiles::Profiles;
use crate::error::AppError;
use crate::games::GameDef;
use std::path::PathBuf;

pub use lock::lock_profiles;

/// The folder that holds every profile's state layers. The diagnostics walk it to find layers left behind by a removed
/// profile or a set the game no longer defines; callers do not need to know the on-disk name.
pub fn state_root(data: &Data) -> PathBuf {
    data.dir.join("state")
}

/// Whether a state folder has no profile in the current set, so its layers are orphaned.
pub fn is_orphan_profile(profile: &str, profiles: &Profiles) -> bool {
    !profiles.profiles.iter().any(|known| known.id == profile)
}

/// Whether a user-data set folder is not defined by the game, so its layer is orphaned.
pub fn is_orphan_set(id: &str, game: &GameDef) -> bool {
    !game.user_data.iter().any(|set| set.id == id)
}

/// Restores both profile stores into the real folders when `profile` owns them. A redirected set is already live in its
/// upper layer; a profile that does not own the data sees the restore on its next switch. The owner check belongs here
/// because `restore_profile` only checks whether its store is active, not who owns the real folders.
pub fn restore_owned(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &str,
    remove_missing: bool,
) -> Result<(), AppError> {
    let owned = state::owned_by(data, profile)?;

    if owned.data {
        data::restore_profile(data, game, settings, profile, remove_missing)?;
    }

    if owned.user_data {
        user_data::restore_profile(data, game, settings, profile, remove_missing)?;
    }

    Ok(())
}
