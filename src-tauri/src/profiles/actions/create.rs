//! Creating and copying profiles, including copying a profile's data.

use crate::context::Data;
use crate::context::load_library;
use crate::context::load_profiles;
use crate::context::load_settings;
use crate::context::profile::history;
use crate::context::profile::lock_profiles;
use crate::context::profile::state as profile_state;
use crate::context::running::game_running;
use crate::error::AppError;
use crate::error::game_running as game_is_running_error;
use crate::games;

use super::lifecycle::remove_profile_state;

/// A new profile, empty or as a copy of the given one. With `copy_data`, the source's saves, settings and overwrite
/// layer are copied too. It does not change what is deployed, so it also works during a deployment.
pub fn create_profile(
    root: &Data,
    game_id: &str,
    name: &str,
    copy_from: Option<&str>,
    copy_data: bool,
) -> Result<(), AppError> {
    // Read the library before the profile lock: a change holds PROFILES from its read to its save, and taking the
    // index lock under it would reverse the order every library change uses.
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = load_library(&data, &game)?;
    let view = crate::domain::library::LibraryView::new(&library);

    if !copy_data {
        return crate::context::edit_profiles(root, game_id, |_, game, profiles| {
            let id = profiles.create(name, copy_from)?;
            // A profile copied from another one keeps its choices. A new profile starts from the definition's defaults.
            if copy_from.is_none() {
                profiles.profile_mut(&id)?.user_data = game.user_data_defaults();
            }
            // The new profile lists the library at once, disabled, like every other profile.
            crate::library::sync::sync_profiles(profiles, &library, &view);
            Ok(())
        });
    }

    let Some(copy_from) = copy_from else {
        return Err(AppError::user("profiles.copy_source_missing"));
    };
    let _state = crate::context::lock_state(&root.dir)?;

    if game_running(root, game_id)? {
        return Err(game_is_running_error());
    }

    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    let source = profiles.profile(copy_from)?.clone();
    let settings = load_settings(&data);
    // The source may own the real data; capture it first so the copy is the state the user sees.
    profile_state::capture_current(&data, &game, &settings)?;
    let id = profiles.create(name, Some(copy_from))?;
    let target = profiles.profile(&id)?.clone();
    let copy =
        crate::profiles::transfer::copy_profile_data(&data, &game, &settings, &source, &target);

    if let Err(error) = copy {
        let _ = profiles.delete(&id);
        let _ = remove_profile_state(&data, &id);
        return Err(error);
    }

    crate::library::sync::sync_profiles(&mut profiles, &library, &view);

    if let Err(error) = data.save_profiles(&profiles) {
        let _ = remove_profile_state(&data, &id);
        return Err(error);
    }

    Ok(())
}

/// Copies one profile's list and data into another profile. The target's earlier data is kept as a `BeforeCopy`
/// snapshot. The source stays as it is.
pub fn copy_profile_data(
    root: &Data,
    game_id: &str,
    source: &str,
    target: &str,
) -> Result<(), AppError> {
    if source == target {
        return Err(AppError::user("profiles.copy_same"));
    }

    let _state = crate::context::lock_state(&root.dir)?;

    if game_running(root, game_id)? {
        return Err(game_is_running_error());
    }

    let _profiles = lock_profiles();
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let settings = load_settings(&data);
    let mut profiles = load_profiles(&data)?;
    let source_profile = profiles.profile(source)?.clone();
    let target_profile = profiles.profile(target)?.clone();
    // Save the owner's real files first, so the snapshot and the copy both see the current state.
    profile_state::capture_current(&data, &game, &settings)?;
    crate::snapshots::before_copy(root, &game, &settings, &target_profile)?;
    crate::profiles::transfer::copy_profile_data(
        &data,
        &game,
        &settings,
        &source_profile,
        &target_profile,
    )?;
    let editable = profiles.profile_mut(target)?;
    editable.restore_list(source_profile.list_state());
    editable.user_data = source_profile.user_data.clone();
    editable.pack.clone_from(&source_profile.pack);
    editable.collections.clone_from(&source_profile.collections);
    history::save_profiles(&data, &profiles)?;
    crate::context::profile::restore_owned(&data, &game, &settings, target, true)?;
    Ok(())
}
