//! Profiles: saving, creating, renaming, switching and comparing them, importing lists, the list history and the bisect
//! search.

use std::path::Path;

use tauri::State;

use crate::commands::{change_patch, for_game};
use crate::context::Data;
use crate::context::profile::history::{self, HistoryEntry};
use crate::domain::profiles::{ModGroup, Row};
use crate::error::{AppError, io_at};
use crate::profiles::bisect;
use crate::profiles::community::{CommunityProfileImport, CommunityProfilePreview};
use crate::profiles::compare::ProfileCompare;
use crate::profiles::history::preview::HistoryPreview;
use crate::profiles::mo2::Mo2Import;
use crate::profiles::{actions, compare};
use crate::status::views::{KnownRows, Status};

crate::commands::game_commands! {
    change create_profile(name: String, copy_from: Option<String>, copy_data: bool) =
        |root, game| actions::create_profile(root, game, &name, copy_from.as_deref(), copy_data);

    /// Copies another profile's list and data into this one. The target's earlier data is kept as a snapshot.
    change copy_profile_data(source: String, target: String) =
        |root, game| actions::copy_profile_data(root, game, &source, &target);

    change rename_profile(id: String, name: String) = |root, game| actions::rename_profile(root, game, &id, &name);

    change switch_profile(id: String) = |root, game| actions::switch_profile(root, game, &id);

    /// Sets whether the profile keeps a user-data set of the game separate.
    change set_profile_user_data(profile: String, set: String, isolated: bool) =
        |root, game| actions::set_profile_user_data(root, game, &profile, &set, isolated);

    /// Moves an isolated redirect set back into the game's folder and makes it shared again.
    change move_profile_user_data(profile: String, set: String) =
        |root, game| actions::move_profile_user_data(root, game, &profile, &set);

    /// The size of the profile's own data for one user-data set, in whole kilobytes.
    query user_data_size(profile: String, set: String) -> u32 =
        |root, game| actions::user_data_size(root, game, &profile, &set);

    change delete_profile(id: String) = |root, game| actions::delete_profile(root, game, &id);

    /// Saves the active profile's mod groups. Stale members and groups left with fewer than two members are dropped.
    change save_mod_groups(groups: Vec<ModGroup>) =
        |root, game| actions::save_mod_groups(root, game, &groups);

    /// Moves the block containing `id` (a page run or a stored group) directly above or below the block containing
    /// `target`. `side` is `above` or `below`.
    change move_mod(id: String, target: String, side: String) =
        |root, game| crate::profiles::edits::move_mod(root, game, &id, &target, &side);

    /// Enables or disables every part of the page or group header `id`.
    change set_mod_enabled(id: String, enabled: bool) =
        |root, game| crate::profiles::edits::set_mod_enabled(root, game, &id, enabled);

    /// Sorts the active list alphabetically by mod name, moving groups as one block.
    change sort_list_by_name() = crate::profiles::order::sort_by_name;

    /// Sorts the active list so every mod stands above the mods it overrides, using the same rules as `sort_by_rules`.
    change sort_list() = crate::profiles::order::sort_by_rules;

    /// Creates one group from the listed mod ids. A selection of fewer than two mods is refused.
    change group_mods(ids: Vec<String>) =
        |root, game| crate::profiles::edits::group_mods(root, game, &ids);

    /// Removes a group, leaving its mods where they are on the list.
    change ungroup_mods(group_id: String) =
        |root, game| crate::profiles::edits::ungroup_mods(root, game, &group_id);

    /// Adds a separator above the block containing `id`, or renames the separator `id`.
    change set_separator(id: String, name: String) =
        |root, game| crate::profiles::edits::set_separator(root, game, &id, &name);

    /// Puts a mod into a group or takes it out of every group with `None`.
    change set_mod_group(id: String, group_id: Option<String>) =
        |root, game| crate::profiles::edits::set_mod_group(root, game, &id, group_id.as_deref());

    /// Compares two profiles of the game: the mods that differ between them and how many are the same.
    query compare_profiles(first: String, second: String) -> ProfileCompare =
        |root, game| compare::compare_profiles(root, game, &first, &second);

    /// What sorting by the rules would move. Nothing is written.
    query preview_sort() -> crate::profiles::order::SortPreview = crate::profiles::order::preview_sort;

    /// Sorts the active list so every mod stands above the mods it overrides. The rest keeps its order. The previous
    /// order is one history entry, so the sort is one undo.
    change sort_by_rules() = crate::profiles::order::sort_by_rules;

    /// The earlier states of the active list, newest first.
    query list_history() -> Vec<HistoryEntry> = history::list_history;

    /// What restoring an earlier state of the active list would change. Nothing is written.
    query preview_restore(id: u32) -> HistoryPreview =
        |root, game| crate::profiles::history::preview::preview_restore(root, game, id);

    /// Takes the last change of the active list back.
    change undo_list() = history::undo_list;

    /// Brings back the change the last undo took away.
    change redo_list() = history::redo_list;

    /// Restores an earlier state of the active list. The restore itself can be undone.
    change restore_list(id: u32) = |root, game| history::restore_list(root, game, id);

    /// Starts the search for the mod that breaks the game: half of the enabled mods go off for the first test.
    change bisect_start() = bisect::bisect_start;

    /// Answers a test of the search: whether the problem was there with this half of the mods.
    change bisect_answer(broken: bool) = |root, game| bisect::bisect_answer(root, game, broken);

    /// Ends the search and puts the list back. With `disable_found`, the mod found stays off.
    change bisect_stop(disable_found: bool) = |root, game| bisect::bisect_stop(root, game, disable_found);
}

/// Saves the active list. When the window sends the row revisions it holds, the answer carries only the rows that
/// changed.
#[tauri::command]
#[specta::specta]
pub async fn save_profile(
    data: State<'_, Data>,
    game: String,
    rows: Vec<Row>,
    known: Option<KnownRows>,
) -> Result<Status, AppError> {
    change_patch(data, game, known, move |root, game| {
        actions::save_profile(root, game, &rows)
    })
    .await
}

/// Makes the active profile's list the one of a Mod Organizer 2 `modlist.txt` at `path`. The answer names what was left
/// out.
#[tauri::command]
#[specta::specta]
pub async fn import_mo2_list(
    data: State<'_, Data>,
    game: String,
    path: String,
) -> Result<Mo2Import, AppError> {
    for_game(data, game, move |root, game| {
        let bytes = std::fs::read(&path).map_err(io_at(Path::new(&path)))?;
        crate::profiles::mo2::import_mo2_list(root, game, &String::from_utf8_lossy(&bytes))
    })
    .await
}

/// Reads an r2modman or Gale profile (`mods.yml`) and matches its packages against the library. Nothing is written.
#[tauri::command]
#[specta::specta]
pub async fn inspect_community_profile(
    data: State<'_, Data>,
    game: String,
    path: String,
) -> Result<CommunityProfilePreview, AppError> {
    for_game(data, game, move |root, game| {
        crate::profiles::community::inspect(root, game, &path)
    })
    .await
}

/// Creates a normal profile from an r2modman or Gale profile. The new profile becomes active.
#[tauri::command]
#[specta::specta]
pub async fn import_community_profile(
    data: State<'_, Data>,
    game: String,
    path: String,
    name: String,
) -> Result<CommunityProfileImport, AppError> {
    for_game(data, game, move |root, game| {
        crate::profiles::community::import(root, game, &path, &name)
    })
    .await
}
