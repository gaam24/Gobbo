//! Saving the active profile's list, priorities and separators, and switching a mod's version.

use std::collections::{BTreeMap, BTreeSet};

use crate::context::Data;
use crate::context::Loaded;
use crate::context::library::not_in_library;
use crate::context::load_game;
use crate::context::load_library;
use crate::context::load_profiles;
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::domain::profiles::Row;
use crate::domain::profiles::load_order::{PriorityRange, assign_priorities};
use crate::domain::profiles::{ListedMod, Separator, normalize_groups};
use crate::domain::resolver::is_orderable;
use crate::error::AppError;
use crate::games;

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
        entries.push(ListedMod::new(
            record.family.clone(),
            row.folder.clone(),
            row.enabled,
            priority,
        ));
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

/// Saves the load order of the active profile: the mods in `order` get the game's priorities in that order, and every
/// other mod loses its priority, so the game orders it itself. The game's settings file gets a row only for the mods
/// that have one. The game must have a load order, the range must have a number for every mod in `order`, and every
/// mod in `order` must have a folder of its own, since the game reads no number for any other (see `is_orderable`).
pub fn set_load_order(root: &Data, game_id: &str, order: &[String]) -> Result<(), AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let Some(load_order) = &game.load_order else {
        return Err(AppError::user("profile.load_order_unsupported"));
    };
    let range = PriorityRange {
        min: load_order.priority_min,
        max: load_order.priority_max,
    };
    // The library is read before the profile lock, as every change to the profiles does (see `create_profile`).
    let library = load_library(&data, &game)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    // The family ids of the mods the game reads in order: the same list and planner inputs the status shows.
    let orderable = {
        let listed = crate::library::active_mods(&profiles, &library, &game);
        let inputs = library.inputs(&listed);
        listed
            .iter()
            .zip(&inputs)
            .filter(|(_, input)| is_orderable(&game.deploy, input))
            .map(|(entry, _)| entry.id.clone())
            .collect::<BTreeSet<String>>()
    };
    assign_priorities(&mut profiles.active_mut().mods, order, range, |id| {
        orderable.contains(id)
    })?;
    save_profiles(&data, &profiles)
}
