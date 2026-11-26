//! Copying the data a profile keeps for a game into another profile. The source stays untouched; the target's stores
//! are replaced whole. The profile lists live in `profiles.json`; this module only moves the folders below the state
//! folder (user data, the legacy copy store and the overwrite layer).

use std::fs;
use std::path::Path;

use crate::context::Data;
use crate::context::Settings;
use crate::context::profile::data as profile_data;
use crate::context::profile::state as profile_state;
use crate::context::profile::user_data;
use crate::domain::profiles::Profile;
use crate::error::{AppError, io_at};
use crate::games::GameDef;
use crate::infra::fs::{CopyMode, copy_tree, remove_dir_if_exists, remove_file_if_exists};

/// Replaces the target profile's data with the source profile's data. Both profiles must already exist and differ.
/// The caller holds the state lock, has refused a running game, and has captured the current owner first.
pub(super) fn copy_profile_data(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    source: &Profile,
    target: &Profile,
) -> Result<(), AppError> {
    let source_state = data.profile_state(&source.id);
    let target_state = data.profile_state(&target.id);
    let source_sets = user_data::sets(data, game, settings, source, &source_state)?;
    let target_sets = user_data::sets(data, game, settings, target, &target_state)?;

    // The target is replaced whole, so a set it no longer isolates cannot leave old files behind.
    let target_user_data = user_data::state_dir(&target_state);
    remove_dir_if_exists(&target_user_data).map_err(io_at(&target_user_data))?;
    let target_legacy = profile_state::legacy_data(&target_state);
    remove_dir_if_exists(&target_legacy).map_err(io_at(&target_legacy))?;
    let target_overwrite = profile_state::overwrite(&target_state);
    remove_dir_if_exists(&target_overwrite).map_err(io_at(&target_overwrite))?;

    for (source_set, target_set) in source_sets.iter().zip(&target_sets) {
        if !source_set.isolated {
            continue;
        }

        if source_set.is_redirected() {
            copy_dir(&source_set.upper, &target_set.upper)?;
            copy_file(
                &user_data::whiteouts(&source_set.upper),
                &user_data::whiteouts(&target_set.upper),
            )?;
            copy_file(
                &user_data::whiteouts_journal(&source_set.upper),
                &user_data::whiteouts_journal(&target_set.upper),
            )?;
            copy_file(
                &user_data::seeded(&source_set.upper),
                &user_data::seeded(&target_set.upper),
            )?;
        } else {
            copy_dir(&source_set.store, &target_set.store)?;
        }
    }

    let source_legacy = profile_state::legacy_data(&source_state);

    if source_legacy.is_dir() {
        copy_dir(&source_legacy, &target_legacy)?;
    }
    // The manifest names the paths the copied store belongs to. It lives outside `data`, so it is copied separately.
    let source_manifest = profile_data::manifest(&source_state);
    let target_manifest = profile_data::manifest(&target_state);
    remove_file_if_exists(&target_manifest).map_err(io_at(&target_manifest))?;
    copy_file(&source_manifest, &target_manifest)?;
    let source_overwrite = profile_state::overwrite(&source_state);

    if source_overwrite.is_dir() {
        copy_dir(&source_overwrite, &target_overwrite)?;
    }

    Ok(())
}

/// Copies one folder, replacing the target. A missing source is an empty set and leaves the target absent.
fn copy_dir(source: &Path, target: &Path) -> Result<(), AppError> {
    if !source.is_dir() {
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(io_at(parent))?;
    }

    copy_tree(source, target, CopyMode::Replace)
        .map_err(|error| io_at(target)(std::io::Error::from(error)))?;
    Ok(())
}

/// Copies one file next to a copied folder. A missing source (no whiteouts, no seed marker) is not an error.
fn copy_file(source: &Path, target: &Path) -> Result<(), AppError> {
    match fs::metadata(source) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_at(source)(error)),
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(io_at(parent))?;
    }

    fs::copy(source, target).map_err(io_at(source))?;
    Ok(())
}
