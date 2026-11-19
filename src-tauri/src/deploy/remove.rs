//! Removing or reinstalling a mod while it may be deployed. The library removes or replaces the folders; this module
//! undoes the prepared deployments that hold the mod first, because their journals name the files that change.

use crate::context::Data;
use crate::context::load_settings;
use crate::context::running::game_running_at;
use crate::deploy::state_is_overlay;
use crate::deploy::target::journal_roots;
use crate::deploy::undo;
use crate::domain::profiles::Entry;
use crate::domain::profiles::Profiles;
use crate::error::AppError;
use crate::error::game_running as game_is_running_error;
use crate::games;
use crate::library;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// Removes a whole mod family, see `library::delete_mod`. Prepared deployments that contain the mod are undone first; a
/// running profile that uses it blocks the removal.
pub fn delete_mod(root: &Data, game_id: &str, id: &str) -> Result<(), AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    library::delete_mod(root, game_id, id, |data, profiles| {
        undo_prepared_holding(game_id, data, profiles, |entry| entry.id == id)
    })
}

/// Removes one installed version, see `library::delete_mod_version`. Prepared deployments that hold the version are
/// undone first; a running profile that uses it blocks the removal.
pub fn delete_version(
    root: &Data,
    game_id: &str,
    folder: &str,
    replacement: Option<&str>,
) -> Result<(), AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    library::delete_mod_version(root, game_id, folder, replacement, |data, profiles| {
        undo_prepared_holding(game_id, data, profiles, |entry| entry.folder == folder)
    })
}

/// Removes every installed version that no profile selects, see `library::delete_unused_versions`. A prepared
/// deployment that still holds one of them is undone first; a running game blocks that undo.
pub fn delete_unused_versions(root: &Data, game_id: &str) -> Result<u32, AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    library::delete_unused_versions(root, game_id, |data, profiles, folders| {
        let wanted: HashSet<&str> = folders.iter().map(String::as_str).collect();
        undo_prepared_holding(game_id, data, profiles, |entry| {
            wanted.contains(entry.folder.as_str())
        })
    })
}

/// Removes every part of the source page the family `id` belongs to. The page is resolved from the index under the
/// library index lock, so a part that another change added before the edit goes with it. Prepared deployments that hold
/// any part are undone first; a running profile that uses one blocks the removal.
pub fn delete_page(root: &Data, game_id: &str, id: &str) -> Result<(), AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    library::delete_page(root, game_id, id, |data, profiles, ids| {
        let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
        undo_prepared_holding(game_id, data, profiles, |entry| {
            wanted.contains(entry.id.as_str())
        })
    })
}

/// Unpacks one mod again from its archive copy, see `import::reinstall_mod`. A prepared deployment that holds this
/// version is undone first, as a removal does, so the game keeps none of its files until the next apply. A running
/// profile that uses it blocks the reinstall.
pub fn reinstall_mod(
    root: &Data,
    game_id: &str,
    folder: &str,
    choices: Option<&[crate::import::fomod::Choice]>,
) -> Result<(), AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    crate::import::reinstall_mod_with(root, game_id, folder, choices, |data, profiles| {
        undo_prepared_holding(game_id, data, profiles, |entry| entry.folder == folder)
    })
}

/// Undoes the prepared deployments of the profiles that hold an entry `holds` takes. Overlay layers keep their
/// `overwrite` folder; only the mod layer is removed. A non-overlay deployment touches the game folder and therefore
/// waits for a closed game.
fn undo_prepared_holding(
    game_id: &str,
    data: &Data,
    profiles: &mut Profiles,
    holds: impl Fn(&Entry) -> bool,
) -> Result<(), AppError> {
    let states = data.deployment_states();
    let prepared = states.prepared;
    let incomplete = states.incomplete;
    if prepared.is_empty() && incomplete.is_empty() {
        return Ok(());
    }
    // The game definition and the settings resolve the roots the undo may touch; the running check reads the game
    // folder from the same settings.
    let game = games::load(game_id)?;
    let settings = load_settings(data);
    let running = game_running_at(game_id, &settings.game_root);
    let last_launched = profiles.last_launched.clone();
    let mut cleaned = Vec::new();
    // An interrupted apply has no ledger entry that names its mods, so it is undone for every removal. Its files are
    // not trusted, and leaving them behind would make the next apply see a foreign file.
    let states: Vec<(&String, &PathBuf, bool)> = incomplete
        .iter()
        .map(|(id, state)| (id, state, true))
        .chain(prepared.iter().map(|(id, state)| (id, state, false)))
        .collect();
    for (profile_id, state, force) in states {
        let Some(profile) = profiles
            .profiles
            .iter()
            .find(|profile| &profile.id == profile_id)
        else {
            continue;
        };
        if !force && !profile.deployed_mods.iter().any(&holds) {
            continue;
        }
        let overlay = state_is_overlay(state);
        if running {
            if last_launched.as_deref() == Some(profile_id) {
                return Err(game_is_running_error());
            }
            if !overlay {
                return Err(game_is_running_error());
            }
        }
        let roots = journal_roots(&game, &settings, state.clone())?;
        undo(&roots).map_err(AppError::from)?;
        if overlay {
            let _ = fs::remove_dir_all(state.join("mods"));
            let _ = fs::remove_dir_all(state.join(".work"));
        }
        cleaned.push(profile_id.clone());
    }
    for profile in &mut profiles.profiles {
        if cleaned.contains(&profile.id) {
            profile.mark_undeployed();
        }
    }
    // The launch memory belongs to the set of profiles, not to one profile. A cleaned profile that was the last
    // launch must not protect an overlay layer that no longer exists.
    if profiles
        .last_launched
        .as_deref()
        .is_some_and(|launched| cleaned.iter().any(|id| id.as_str() == launched))
    {
        profiles.last_launched = None;
    }
    Ok(())
}
