//! Renaming, switching and deleting a profile, and removing a profile's state folder.

use crate::context::Data;
use crate::context::data::JOURNAL_FILE;
use crate::context::load_profiles;
use crate::context::load_settings;
use crate::context::profile::history::forget_profile;
use crate::context::profile::lock_profiles;
use crate::context::profile::state as profile_state;
use crate::context::running::game_running;
use crate::deploy::target::journal_roots;
use crate::deploy::undo;
use crate::deploy::undo_blocked_while_running;
use crate::error::AppError;
use crate::error::game_running as game_is_running_error;
use crate::error::io_at;
use crate::games;

/// Removes the profile's state together with its mod layer. A missing folder means there is nothing to remove.
pub(super) fn remove_profile_state(data: &Data, id: &str) -> Result<(), AppError> {
    let state = data.profile_state(id);
    crate::infra::fs::remove_dir_if_exists(&state).map_err(io_at(&state))
}

/// Renames a profile. The id stays the same, so the state folder and a running game keep their profile.
pub fn rename_profile(root: &Data, game_id: &str, id: &str, name: &str) -> Result<(), AppError> {
    crate::context::edit_profiles(root, game_id, |_, _, profiles| profiles.rename(id, name))
}

/// Switches the profile shown and edited. It changes nothing on disk: a deployment of another profile stays until the
/// viewed profile is applied or launched.
pub fn switch_profile(root: &Data, game_id: &str, id: &str) -> Result<(), AppError> {
    crate::context::edit_profiles(root, game_id, |_, _, profiles| profiles.switch(id))
}

/// Removes a profile. A prepared deployment of a profile that is not running is undone and its state folder
/// removed first. The last profile cannot be deleted.
pub fn delete_profile(root: &Data, game_id: &str, id: &str) -> Result<(), AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    let _profiles = lock_profiles();
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let settings = load_settings(&data);
    let mut profiles = load_profiles(&data)?;
    profiles.profile(id)?;

    if profiles.profiles.len() == 1 {
        return Err(AppError::user("profiles.last_profile"));
    }

    if !game_running(root, game_id)? {
        let next = if profiles.active == id {
            profiles
                .profiles
                .iter()
                .find(|profile| profile.id != id)
                .map_or_else(|| profiles.active.clone(), |profile| profile.id.clone())
        } else {
            profiles.active.clone()
        };
        profile_state::hand_over(&data, &game, &settings, id, profiles.profile(&next)?)?;
    }

    crate::snapshots::before_delete(root, &game, &settings, id)?;
    let state = data.profile_state(id);

    if state.join(JOURNAL_FILE).is_file() {
        if game_running(root, game_id)?
            && undo_blocked_while_running(&state, id, profiles.last_launched.as_deref())
        {
            return Err(game_is_running_error());
        }

        let roots = journal_roots(&data, &game, &settings, state)?;
        undo(&roots).map_err(AppError::from)?;
    }

    profiles.delete(id)?;
    forget_profile(&data, id)?;
    remove_profile_state(&data, id)?;

    if profiles.last_launched.as_deref() == Some(id) {
        profiles.last_launched = None;
    }

    data.save_profiles(&profiles)
}
