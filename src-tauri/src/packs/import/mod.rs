//! Opening a pack and importing it into the library.

mod bundled;

pub(super) use bundled::import_bundled;

use super::PACK_CANCELLED;
use super::adopt::adopt_family_data;
use super::inspect::{made_from, missing, open_pack};
use super::model::PackImportView;
use super::model::STAGING_DIR;
use crate::context::Data;
use crate::context::load_library;
use crate::context::load_profiles;
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::domain::library::LibraryView;
use crate::domain::pack::WaitingMod;
use crate::domain::profiles::ListedMod;
use crate::domain::profiles::ModGroup;
use crate::domain::profiles::PackOrigin;
use crate::domain::profiles::Separator;
use crate::error::AppError;
use crate::error::io_at;
use crate::games;
use crate::infra::store;
use crate::launch::steps::load_mod_steps;
use crate::launch::steps::save_mod_steps;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// One archive inside a pack may not exceed this; a pack is a collection of mods, not a disk image.
pub const MAX_PACK_ARCHIVE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// The archives of one pack may not exceed this together.
pub const MAX_PACK_TOTAL_BYTES: u64 = 32 * 1024 * 1024 * 1024;

/// The staging folder of one pack import. Its name carries a unique suffix, so two imports never share it and a drop
/// removes only this import's folder.
struct Staging(PathBuf);

impl Staging {
    fn create(data: &Data) -> Result<Self, AppError> {
        let path = data
            .dir
            .join(format!("{STAGING_DIR}-{}", store::unique_suffix()));
        fs::create_dir_all(&path).map_err(io_at(&path))?;
        Ok(Self(path))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A pack opened for reading: its manifest and the archives inside it, by SHA-256 sum.
pub fn import_pack(root: &Data, game_id: &str, path: &Path) -> Result<PackImportView, AppError> {
    import_pack_with_progress(root, game_id, path, &mut |_, _| {}, None)
}

/// Imports a pack and reports the import stages of the archives inside it, as the import batch does. `cancel` stops
/// between batches; a batch already committed stays in the library.
pub fn import_pack_with_progress(
    root: &Data,
    game_id: &str,
    path: &Path,
    progress: &mut dyn FnMut(crate::import::model::ImportStage, u32),
    cancel: Option<&AtomicBool>,
) -> Result<PackImportView, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let mut opened = open_pack(game_id, path)?;
    let mut view = PackImportView {
        imported: 0,
        failed: Vec::new(),
        profiles_created: 0,
        profiles_updated: 0,
        missing: Vec::new(),
    };
    let imported = import_bundled(
        root,
        game_id,
        &data,
        &mut opened,
        &mut view,
        progress,
        cancel,
    )?;

    let manifest = opened.manifest;
    // The index is read and saved under its lock. The archives above took it one by one, each for its own import.
    let _index = crate::context::library::lock_index();
    let _profiles = lock_profiles();
    let mut library = load_library(&data, &game)?;
    let mut steps = load_mod_steps(&data)?;
    let mut profiles = load_profiles(&data)?;
    let mut seen_missing: rustc_hash::FxHashSet<&str> = rustc_hash::FxHashSet::default();

    for pack in &manifest.profiles {
        // One set of indexes per profile: the data adopted below changes them, and the next profile must see it.
        let indexes = LibraryView::new(&library);
        let resolved: Vec<Option<(String, String)>> = pack
            .mods
            .iter()
            .map(|entry| {
                indexes
                    .find(entry, Some(&imported))
                    .and_then(|found| found.entry())
            })
            .collect();
        // The next pack mod that resolves to the library, for every position. Scanning the rest of the list for
        // every waiting mod was quadratic.
        let mut next_found: Vec<Option<&str>> = vec![None; resolved.len()];
        let mut next: Option<&str> = None;

        for (at, found) in resolved.iter().enumerate().rev() {
            next_found[at] = next;

            if let Some((family, _)) = found {
                next = Some(family.as_str());
            }
        }
        // The author's family ids name the same mods here by the ids of this library.
        let families: BTreeMap<&str, &str> = pack
            .mods
            .iter()
            .zip(&resolved)
            .filter_map(|(entry, found)| Some((entry.family.as_str(), found.as_ref()?.0.as_str())))
            .collect();
        let mut state = crate::domain::profiles::ListState {
            mods: Vec::new(),
            separators: Vec::new(),
            winners: BTreeMap::new(),
            groups: Vec::new(),
            waiting: Vec::new(),
        };
        // The families already in this list. A linear lookup for each mod of the pack was quadratic.
        let mut kept: rustc_hash::FxHashSet<&str> = rustc_hash::FxHashSet::default();
        let mut waiting: rustc_hash::FxHashSet<&str> = rustc_hash::FxHashSet::default();

        for (at, (entry, found)) in pack.mods.iter().zip(&resolved).enumerate() {
            let Some((family, folder)) = found else {
                if seen_missing.insert(entry.family.as_str()) {
                    view.missing.push(missing(entry));
                }
                // The mod waits above the next mod of the pack that is in the list, with its place and state.
                if waiting.insert(entry.family.as_str()) {
                    let before = next_found[at].map(str::to_string);
                    state.waiting.push(WaitingMod {
                        before,
                        entry: entry.clone(),
                    });
                }

                continue;
            };

            if !kept.insert(family.as_str()) {
                continue;
            }

            state.mods.push(ListedMod {
                id: family.clone(),
                folder: folder.clone(),
                enabled: entry.enabled,
                priority: entry.priority,
            });
            adopt_family_data(&mut library, family, entry, &families);

            if let Some(phases) = &entry.steps {
                steps
                    .mods
                    .entry(folder.clone())
                    .or_insert_with(|| phases.clone());
            }
        }
        // The list key of each mod of the pack: its family here, or its family in the pack while the library lacks it.
        // Separators, file winners and groups all name a mod by this key, so a waiting mod keeps its place in them.
        let key_of = |index: usize| {
            let entry = pack.mods.get(index)?;
            let found = resolved.get(index)?;
            Some(
                found
                    .as_ref()
                    .map_or_else(|| entry.family.clone(), |(family, _)| family.clone()),
            )
        };
        state.separators = pack
            .separators
            .iter()
            .enumerate()
            .map(|(number, separator)| Separator {
                id: format!("sep{}", number + 1),
                name: separator.name.clone(),
                // A separator above a missing mod names that mod, which waits in the list until the library has it.
                before: separator.before.and_then(key_of),
            })
            .collect();
        state.winners = pack
            .winners
            .iter()
            .filter_map(|(path, at)| Some((path.clone(), key_of(*at)?)))
            .collect();
        // Member indices become this library's list keys. A group that loses all but one of its members carries no
        // useful information, so it is dropped; the rest get fresh local ids, like the separators above.
        state.groups = pack
            .groups
            .iter()
            .enumerate()
            .filter_map(|(number, group)| {
                let members: Vec<String> = group
                    .members
                    .iter()
                    .filter_map(|&index| key_of(index))
                    .collect();
                if members.len() < 2 {
                    return None;
                }

                Some(ModGroup {
                    id: format!("grp{}", number + 1),
                    name: group.name.clone(),
                    members,
                    kind: group.kind,
                    language: group.language.clone(),
                })
            })
            .collect();

        let origin = PackOrigin {
            id: manifest.info.id.clone(),
            version: manifest.info.version.clone(),
            profile: pack.name.clone(),
        };
        let existing =
            made_from(&profiles, &manifest.info, &pack.name).map(|profile| profile.id.clone());
        let (id, is_new) = if let Some(id) = existing {
            view.profiles_updated += 1;
            (id, false)
        } else {
            view.profiles_created += 1;
            (profiles.create_unique(&pack.name)?, true)
        };
        let profile = profiles.profile_mut(&id)?;
        profile.restore_list(state);
        // An older pack has no choices: a new profile keeps the definition defaults and an existing one keeps its
        // local choices. A pack that carries them reproduces the author's profile. Set ids the receiver's game no
        // longer defines are ignored, so a changed definition cannot restore a stale choice.
        if !pack.user_data.is_empty() {
            let mut choices = game.user_data_defaults();

            for (set, isolation) in &pack.user_data {
                if choices.contains_key(set) {
                    choices.insert(set.clone(), *isolation);
                }
            }

            profile.user_data = choices;
        } else if is_new {
            profile.user_data = game.user_data_defaults();
        }
        // A pack owns the list; a collection origin of an earlier list no longer describes it.
        profile.collections.clear();
        profile.pack = Some(origin);
    }

    data.save_library(library)?;
    save_mod_steps(&data, &steps)?;
    save_profiles(&data, &profiles)?;
    Ok(view)
}

/// A stopped batch reports the pack's own stop, whose message says that the archives already imported stay.
pub(super) fn pack_stop(error: AppError) -> AppError {
    if error.is_cancelled() {
        AppError::user(PACK_CANCELLED)
    } else {
        error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn two_staging_folders_do_not_collide() {
        let base = scratch!("pack-staging");
        let data = Data::new(base.join("data"));
        let first = Staging::create(&data).unwrap();
        let second = Staging::create(&data).unwrap();
        assert_ne!(first.0, second.0);
        let first_path = first.0.clone();
        let second_path = second.0.clone();
        fs::write(first_path.join("first.zip"), b"one").unwrap();
        fs::write(second_path.join("second.zip"), b"two").unwrap();

        drop(first);
        assert!(!first_path.exists());
        assert!(second_path.join("second.zip").is_file());

        drop(second);
        assert!(!second_path.exists());
    }
}
