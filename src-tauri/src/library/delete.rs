//! Deleting mods and versions: the folders, the index entries, the archive copies and the profile entries.

use crate::context::Data;
use crate::context::library::not_in_library;
use crate::context::load_profiles;
use crate::context::profile::lock_profiles;
use crate::domain::library::{Library, LibraryView};
use crate::domain::profiles::{Profiles, ProfilesView};
use crate::error::AppError;
use std::collections::HashSet;

use super::remove_version_folder;
use crate::context::edit_library_clean;

/// Removes a whole mod family: every version folder, the index entries, the archive copies that nothing uses any more,
/// and the entries in all profiles. `undo_deployments` runs first, with the profiles, so the prepared deployments that
/// hold the mod are undone before its files go; the deploy module supplies it (`deploy::remove::delete_mod`).
pub fn delete_mod(
    root: &Data,
    game_id: &str,
    id: &str,
    undo_deployments: impl FnOnce(&Data, &mut Profiles) -> Result<(), AppError>,
) -> Result<(), AppError> {
    delete_families(root, game_id, &[id.to_string()], undo_deployments)
}

/// Removes every part of the source page the family `id` belongs to: the family itself and every family with the same
/// provider and mod id. The page is resolved from the index read under the index lock, so a part that a change added
/// just before the delete is removed with its page. The folders, index entries, unused archive copies and profile
/// entries of every family go together; `undo_deployments` runs once before them, with the resolved family ids.
pub fn delete_page(
    root: &Data,
    game_id: &str,
    id: &str,
    undo_deployments: impl FnOnce(&Data, &mut Profiles, &[String]) -> Result<(), AppError>,
) -> Result<(), AppError> {
    edit_library_clean(root, game_id, |data, _, library| {
        let ids = page_families(library, id)?;
        delete_locked(data, library, &ids, |data, profiles| {
            undo_deployments(data, profiles, &ids)
        })
    })
}

/// Removes several families at once. The folders, index entries, unused archive copies and profile entries of every
/// family go together; `undo_deployments` runs once before them.
fn delete_families(
    root: &Data,
    game_id: &str,
    ids: &[String],
    undo_deployments: impl FnOnce(&Data, &mut Profiles) -> Result<(), AppError>,
) -> Result<(), AppError> {
    edit_library_clean(root, game_id, |data, _, library| {
        delete_locked(data, library, ids, undo_deployments)
    })
}

/// The families of the source page the family `id` belongs to, in family-id order. A family without a provider source
/// is a page of its own. `library.mod_missing` when the index has no such family. Call it only with the index read
/// under the index lock: a page resolved while another change can run is stale by the time the delete runs.
fn page_families(library: &Library, id: &str) -> Result<Vec<String>, AppError> {
    // The page used to be resolved by `deploy::remove::delete_page` before it took the index lock, so a part added in
    // between survived the delete. This assertion is the regression guard: it fails if the resolution moves out of
    // the locked edit again. The old window is gone, so a test cannot place a change in it any more.
    #[cfg(test)]
    assert!(
        crate::context::library::index_is_held(),
        "a page is resolved only from the index read under the index lock"
    );
    let family = library.families.get(id).ok_or_else(|| not_in_library(id))?;
    let Some((provider, source)) = family.sources.iter().next() else {
        return Ok(vec![id.to_string()]);
    };
    Ok(library
        .families
        .values()
        .filter(|candidate| {
            candidate
                .sources
                .get(provider)
                .is_some_and(|other| other.mod_id == source.mod_id)
        })
        .map(|candidate| candidate.id.clone())
        .collect())
}

/// The body of a multi-family delete, run under the index lock with the library it removes from.
fn delete_locked(
    data: &Data,
    library: &mut Library,
    ids: &[String],
    undo_deployments: impl FnOnce(&Data, &mut Profiles) -> Result<(), AppError>,
) -> Result<(), AppError> {
    let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let folders: Vec<String> = library
        .versions
        .values()
        .filter(|version| wanted.contains(version.family.as_str()))
        .map(|version| version.folder.clone())
        .collect();
    let known = ids.iter().any(|id| library.families.contains_key(id));
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(data)?;
    let folder_set: HashSet<&str> = folders.iter().map(String::as_str).collect();
    let view = ProfilesView::new(&profiles);
    let listed = ids.iter().any(|id| !view.with_mod(id).is_empty())
        || profiles.profiles.iter().any(|profile| {
            profile
                .mods
                .iter()
                .any(|entry| folder_set.contains(entry.folder.as_str()))
        });
    if !known && folders.is_empty() && !listed {
        return Err(not_in_library(ids.first().map_or("", String::as_str)));
    }
    undo_deployments(data, &mut profiles)?;
    for folder in &folders {
        remove_version_folder(data, folder)?;
        library.versions.remove(folder);
    }
    for id in ids {
        library.families.remove(id);
        profiles.remove_mod(id);
    }
    data.save_profiles(&profiles)
}

/// Removes one installed version. When a profile still selects it, `replacement` must name another version of the
/// same family; deleting the only version removes the whole family. `undo_deployments` runs first, with the profiles,
/// so the prepared deployments that hold the version are undone before its files go.
pub fn delete_mod_version(
    root: &Data,
    game_id: &str,
    folder: &str,
    replacement: Option<&str>,
    undo_deployments: impl FnOnce(&Data, &mut Profiles) -> Result<(), AppError>,
) -> Result<(), AppError> {
    edit_library_clean(root, game_id, |data, _, library| {
        let family = library.version(folder)?.family.clone();
        let _profiles = lock_profiles();
        let mut profiles = load_profiles(data)?;
        let used = !ProfilesView::new(&profiles)
            .using_version(folder)
            .is_empty();
        let last = LibraryView::new(library).versions_of(&family).len() == 1;
        // A replacement is only possible when another version stays. Deleting the last version removes the family,
        // so the profile entries go with it.
        let replacement = if used && !last {
            let in_use = || AppError::user("library.version_in_use").with("folder", folder);
            let next = replacement
                .filter(|next| *next != folder)
                .ok_or_else(in_use)?;
            library
                .versions
                .get(next)
                .filter(|version| version.family == family)
                .ok_or_else(|| AppError::user("library.version_unknown").with("folder", next))?;
            Some(next)
        } else {
            None
        };
        undo_deployments(data, &mut profiles)?;
        if let Some(next) = replacement {
            profiles.replace_version(folder, next);
        }
        remove_version_folder(data, folder)?;
        library.versions.remove(folder);
        if last {
            library.families.remove(&family);
            profiles.remove_mod(&family);
        }
        if used || last {
            data.save_profiles(&profiles)?;
        }
        Ok(())
    })
}

/// Removes every version that no profile selects. Returns the number of removed versions. A prepared deployment can
/// still hold such a version when the profile list changed after the deployment, so `undo_deployments` runs before the
/// folders go.
pub fn delete_unused_versions(
    root: &Data,
    game_id: &str,
    undo_deployments: impl FnOnce(&Data, &mut Profiles, &[String]) -> Result<(), AppError>,
) -> Result<u32, AppError> {
    edit_library_clean(root, game_id, |data, _, library| {
        let _profiles = lock_profiles();
        let mut profiles = load_profiles(data)?;
        let used: HashSet<&str> = profiles
            .profiles
            .iter()
            .flat_map(|profile| profile.mods.iter().map(|entry| entry.folder.as_str()))
            .collect();
        let unused: Vec<String> = library
            .versions
            .keys()
            .filter(|folder| !used.contains(folder.as_str()))
            .cloned()
            .collect();
        if !unused.is_empty() {
            undo_deployments(data, &mut profiles, &unused)?;
            for folder in &unused {
                remove_version_folder(data, folder)?;
                library.versions.remove(folder);
            }
            data.save_profiles(&profiles)?;
        }
        library.retain_families_of_versions();
        Ok(u32::try_from(unused.len()).unwrap_or(u32::MAX))
    })
}
