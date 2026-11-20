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
use crate::error::AppError;
use crate::games::GameDef;

pub use lock::lock_profiles;

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
