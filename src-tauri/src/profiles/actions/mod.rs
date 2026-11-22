//! Changing profiles: saving the list, priorities, separators, and creating, renaming, switching or deleting a profile.
//! The user-data move lives in `user_data.rs`, next to the list and lifecycle actions it belongs to.

mod groups;
mod user_data;

pub use groups::save_mod_groups;
pub use user_data::{move_profile_user_data, set_profile_user_data, user_data_size};

use crate::context::Data;
use crate::context::Loaded;
use crate::context::data::JOURNAL_FILE;
use crate::context::library::not_in_library;
use crate::context::load_game;
use crate::context::load_library;
use crate::context::load_profiles;
use crate::context::load_settings;
use crate::context::profile::history::save_profiles;
use crate::context::profile::history::{self, forget_profile};
use crate::context::profile::lock_profiles;
use crate::context::profile::state as profile_state;
use crate::context::running::game_running;
use crate::deploy::target::journal_roots;
use crate::deploy::undo;
use crate::deploy::undo_blocked_while_running;
use crate::domain::profiles::Row;
use crate::domain::profiles::{Entry as ProfileEntry, Separator, normalize_groups};
use crate::error::game_running as game_is_running_error;
use crate::error::{AppError, io_at};
use crate::games;
use std::collections::{BTreeMap, BTreeSet};

/// Removes the profile's state together with its mod layer. A missing folder means there is nothing to remove.
pub(super) fn remove_profile_state(data: &Data, id: &str) -> Result<(), AppError> {
    let state = data.profile_state(id);
    crate::infra::fs::remove_dir_if_exists(&state).map_err(io_at(&state))
}

/// Saves the active profile's list from the view. While a deployment is active the changes wait as a draft,
/// and applying deploys the draft over the old deployment. Refuses when a version is not in the library.
/// The view has no priorities, so each mod keeps the one it had.
pub fn save_profile(root: &Data, game_id: &str, rows: &[Row]) -> Result<(), AppError> {
    let Loaded { data, library, .. } = load_game(root, game_id)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    let mut seen = BTreeSet::new();
    let mut entries = Vec::with_capacity(rows.len());
    let mut separators = Vec::new();
    // The folders on disk once, from the read-through listing: a `is_dir` per row cost one disk query for every mod of
    // a large list on each save. The list is sorted; the check is a binary search.
    let disk_folders = data.library_folders()?;
    // Every id the profile already has stays reserved for its own row, even when a new separator comes before it.
    let mut ids = SeparatorIds::new(&profiles.active().separators);
    // The priority of every family, read once. A scan of the active list for every row made a save quadratic.
    let mut priorities: rustc_hash::FxHashMap<&str, Option<u32>> = rustc_hash::FxHashMap::default();

    for entry in &profiles.active().mods {
        // The first entry wins, matching the linear lookup this map replaced.
        priorities
            .entry(entry.id.as_str())
            .or_insert(entry.priority);
    }
    // Separators wait for the next mod row: they sit above it.
    let mut waiting: Vec<Separator> = Vec::new();

    for row in rows {
        if let Some(name) = &row.separator {
            waiting.push(separator(row, name, &mut ids)?);
            continue;
        }

        let record = library
            .versions
            .get(&row.folder)
            .ok_or_else(|| not_in_library(&row.folder))?;
        if disk_folders.binary_search(&row.folder).is_err() {
            return Err(not_in_library(&row.folder));
        }

        if !seen.insert(record.family.as_str()) {
            return Err(AppError::user("profile.duplicate_mod").with("folder", &row.folder));
        }

        separators.extend(
            std::mem::take(&mut waiting)
                .into_iter()
                .map(|separator| Separator {
                    before: Some(record.family.clone()),
                    ..separator
                }),
        );
        let priority = priorities.get(record.family.as_str()).copied().flatten();
        entries.push(ProfileEntry {
            id: record.family.clone(),
            folder: row.folder.clone(),
            enabled: row.enabled,
            priority,
        });
    }

    separators.append(&mut waiting);
    // A separator above a mod that still waits for the library stays above it, as the list shows it at the end.
    let awaited: BTreeMap<String, String> = profiles
        .active()
        .separators
        .iter()
        .filter_map(|separator| {
            let before = separator.before.as_ref()?;
            let waits = profiles
                .active()
                .waiting
                .iter()
                .any(|waiting| waiting.key() == before);
            waits.then(|| (separator.id.clone(), before.clone()))
        })
        .collect();
    for separator in &mut separators {
        if separator.before.is_none()
            && let Some(family) = awaited.get(&separator.id)
        {
            separator.before = Some(family.clone());
        }
    }

    let active = profiles.active_mut();
    active.mods = entries;
    active.separators = separators;
    // The window sends mods and separators but not groups, so prune the groups against the list it just saved:
    // stale members, duplicate membership and groups left with fewer than two mods never reach the disk.
    normalize_groups(active);
    save_profiles(&data, &profiles)
}

/// The ids one save uses for separators. An existing separator keeps the id in its row; a new one gets the next
/// `sepN` above every id the profile has or this save already claimed, so a separator after an insertion never
/// renumbers. A removed id is not reused while a higher one is still in use.
pub(super) struct SeparatorIds {
    existing: BTreeSet<String>,
    used: BTreeSet<String>,
    next: u64,
}

impl SeparatorIds {
    pub(super) fn new(existing: &[Separator]) -> Self {
        let existing: BTreeSet<String> = existing
            .iter()
            .map(|separator| separator.id.clone())
            .collect();
        let next = existing
            .iter()
            .filter_map(|id| id.strip_prefix("sep"))
            // A hand-edited number beyond `u32` is left out of the counter; `existing` still keeps it taken.
            .filter_map(|number| number.parse::<u32>().ok())
            .max()
            .map_or(1, |number| u64::from(number) + 1);
        Self {
            existing,
            used: BTreeSet::new(),
            next,
        }
    }

    fn claim(&mut self, wanted: &str) -> String {
        if !wanted.is_empty() && self.used.insert(wanted.to_string()) {
            return wanted.to_string();
        }

        // `first_free` starts at one; offset it past every id the profile had, so a removed id is not reused while a
        // higher one exists. The predicate keeps both the ids on the list and the ones this save already claimed.
        let first = self.next;
        let mut claimed = first;
        let id = crate::domain::naming::first_free(
            |number| {
                claimed = first + u64::from(number) - 1;
                format!("sep{claimed}")
            },
            |id| !self.existing.contains(id) && self.used.insert(id.to_string()),
        );
        self.next = claimed + 1;
        id
    }
}

/// A separator from a row: a trimmed, non-empty name, and its id. A row that carries an id keeps it; an empty one
/// gets a fresh id that no other separator has.
pub(super) fn separator(
    row: &Row,
    name: &str,
    ids: &mut SeparatorIds,
) -> Result<Separator, AppError> {
    let name = name.trim();

    if name.is_empty() || name.chars().count() > crate::domain::profiles::MAX_NAME_CHARS {
        return Err(AppError::user("profile.separator_name"));
    }

    Ok(Separator {
        id: ids.claim(&row.folder),
        name: name.to_string(),
        before: None,
    })
}

/// Switches the version of one family in the active profile.
pub fn set_mod_version(root: &Data, game_id: &str, id: &str, folder: &str) -> Result<(), AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = load_library(&data, &game)?;
    library.version_of_family(folder, id)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    profiles.active_entry_mut(id)?.folder = folder.to_string();
    save_profiles(&data, &profiles)
}

/// Saves the priority the user set for a mod in the active profile, or clears it with `None`. The game's settings
/// file gets a row only for mods with a priority. The value must be inside the game's `priority_min..=priority_max`.
pub fn set_mod_priority(
    root: &Data,
    game_id: &str,
    id: &str,
    priority: Option<u32>,
) -> Result<(), AppError> {
    let _profiles = lock_profiles();
    let game = games::load(game_id)?;
    let data = root.game(game_id);

    if let Some(value) = priority {
        let Some(order) = &game.load_order else {
            return Err(AppError::user("profile.priority_unsupported"));
        };

        if !(order.priority_min..=order.priority_max).contains(&value) {
            return Err(AppError::user("profile.priority_out_of_range")
                .with("min", order.priority_min)
                .with("max", order.priority_max));
        }
    }

    let mut profiles = load_profiles(&data)?;
    profiles.active_entry_mut(id)?.priority = priority;
    save_profiles(&data, &profiles)
}

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
                profiles.get_mut(&id)?.user_data = game.user_data_defaults();
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
    let source = profiles.get(copy_from)?.clone();
    let settings = load_settings(&data);
    // The source may own the real data; capture it first so the copy is the state the user sees.
    profile_state::capture_current(&data, &game, &settings)?;
    let id = profiles.create(name, Some(copy_from))?;
    let target = profiles.get(&id)?.clone();
    let copy = super::transfer::copy_profile_data(&data, &game, &settings, &source, &target);

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
    let source_profile = profiles.get(source)?.clone();
    let target_profile = profiles.get(target)?.clone();
    // Save the owner's real files first, so the snapshot and the copy both see the current state.
    profile_state::capture_current(&data, &game, &settings)?;
    crate::snapshots::before_copy(root, &game, &settings, &target_profile)?;
    super::transfer::copy_profile_data(&data, &game, &settings, &source_profile, &target_profile)?;
    let editable = profiles.get_mut(target)?;
    editable.restore_list(source_profile.list_state());
    editable.user_data = source_profile.user_data.clone();
    editable.pack.clone_from(&source_profile.pack);
    editable.collections.clone_from(&source_profile.collections);
    history::save_profiles(&data, &profiles)?;
    crate::context::profile::restore_owned(&data, &game, &settings, target, true)?;
    Ok(())
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
    profiles.get(id)?;

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
        profile_state::hand_over(&data, &game, &settings, id, profiles.get(&next)?)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use crate::test_support::write;
    use std::fs;

    fn row(folder: &str) -> Row {
        crate::test_support::row(folder, false)
    }

    #[test]
    fn removing_a_profile_state_removes_its_folder_and_a_missing_one_is_fine() {
        let base = scratch!("remove-state");
        let data = Data::new(base);
        let state = data.profile_state("p2");
        fs::create_dir_all(state.join("inner")).unwrap();

        remove_profile_state(&data, "p2").unwrap();
        assert!(!state.exists());
        remove_profile_state(&data, "p2").unwrap();
    }

    #[test]
    fn a_profile_state_that_is_not_a_folder_is_an_error() {
        let base = scratch!("state-not-folder");
        let data = Data::new(base);
        write(&data.profile_state("p3"), "not a folder");

        assert!(remove_profile_state(&data, "p3").is_err());
    }

    fn separator_row(id: &str) -> Separator {
        Separator {
            id: id.to_string(),
            name: id.to_string(),
            before: None,
        }
    }

    #[test]
    fn a_separator_name_is_trimmed_and_at_most_sixty_characters() {
        let mut ids = SeparatorIds::new(&[]);
        assert!(separator(&row(""), "   ", &mut ids).is_err());
        assert!(separator(&row(""), &"x".repeat(61), &mut ids).is_err());
        let kept = separator(&row(""), &format!("  {}  ", "x".repeat(60)), &mut ids).unwrap();
        assert_eq!(kept.name, "x".repeat(60));
    }

    #[test]
    fn a_new_separator_above_the_others_does_not_take_their_ids() {
        let existing = [separator_row("sep1"), separator_row("sep2")];
        let mut ids = SeparatorIds::new(&existing);
        // The list comes top first, so the new separator is claimed before the two that already exist.
        assert_eq!(separator(&row(""), "New", &mut ids).unwrap().id, "sep3");
        assert_eq!(
            separator(&row("sep1"), "First", &mut ids).unwrap().id,
            "sep1"
        );
        assert_eq!(
            separator(&row("sep2"), "Second", &mut ids).unwrap().id,
            "sep2"
        );
        // A row that carries an id the profile did not have keeps it.
        assert_eq!(
            separator(&row("modB"), "Named", &mut ids).unwrap().id,
            "modB"
        );
    }

    #[test]
    fn a_removed_separator_id_is_not_reused_while_a_higher_one_exists() {
        let existing = [separator_row("sep1"), separator_row("sep3")];
        let mut ids = SeparatorIds::new(&existing);
        assert_eq!(separator(&row(""), "New", &mut ids).unwrap().id, "sep4");
    }

    #[test]
    fn two_rows_with_the_same_id_get_different_ones() {
        let mut ids = SeparatorIds::new(&[]);
        assert_eq!(separator(&row("sep1"), "One", &mut ids).unwrap().id, "sep1");
        assert_eq!(
            separator(&row("sep1"), "Again", &mut ids).unwrap().id,
            "sep2"
        );
    }
}
