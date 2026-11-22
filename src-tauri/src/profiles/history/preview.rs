//! What restoring a kept state would change. The history list says what a change did; this says what going back to
//! the state before it changes now, before anything is written.

use crate::context::Data;
use crate::context::Loaded;
use crate::context::load_game;
use crate::context::profile::history::{read_index, read_state};
use crate::domain::library::Library;
use crate::domain::profiles::Entry;
use crate::domain::profiles::diff::diff;
use crate::domain::profiles::history::separator_changes;
use crate::domain::profiles::history::waiting_changes;
use crate::domain::profiles::history::winner_changes;
use crate::error::AppError;
use crate::games;
use rustc_hash::FxHashSet;
use serde::Serialize;

/// One mod of the current list or a kept state, with the name and version the library gives it.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct HistoryMod {
    /// The family id.
    pub id: String,
    /// The family name, or the version name when the family is gone, or the id as a last fallback.
    pub name: String,
    /// The selected version folder.
    pub folder: String,
    /// The version as the library spells it, when it has one.
    pub version: Option<String>,
    /// Whether the list that holds this mod has it switched on.
    pub enabled: bool,
    /// The kept state lists a version the library no longer has.
    pub missing: bool,
}

/// One side of a version change.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct HistorySide {
    /// The version folder.
    pub folder: String,
    /// The version as the library spells it, when it has one.
    pub version: Option<String>,
    /// Whether this side of the change has the mod switched on.
    pub enabled: bool,
    /// The library no longer has this version folder.
    pub missing: bool,
}

/// A mod both lists have with another version.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct HistoryVersionChange {
    pub id: String,
    pub name: String,
    /// The version in the current list.
    pub from: HistorySide,
    /// The version in the kept state, which a restore selects.
    pub to: HistorySide,
}

/// What restoring a kept state changes in the active list. Nothing is written.
#[derive(Debug, Serialize, specta::Type)]
pub struct HistoryPreview {
    /// The kept state's id.
    pub id: u32,
    /// When the list left the kept state.
    pub at: u32,
    /// Mods in the kept state, and how many of them are on.
    pub total: u32,
    /// Mods switched on in the kept state.
    pub enabled: u32,
    /// Mods the restore brings back, in the state's order. A `missing` one no longer exists in the library.
    pub adds: Vec<HistoryMod>,
    /// Mods the restore removes, in the current order.
    pub removes: Vec<HistoryMod>,
    pub versions: Vec<HistoryVersionChange>,
    /// Mods the restore switches on.
    pub switches_on: Vec<HistoryMod>,
    /// Mods the restore switches off.
    pub switches_off: Vec<HistoryMod>,
    /// Mods whose place in the list changes.
    pub moved: u32,
    /// Mods whose load priority changes.
    pub priority: u32,
    /// Separators added, removed, renamed or moved.
    pub groups: u32,
    /// File winner choices changed.
    pub winners: u32,
    /// Pack mods waiting for the library changed.
    pub waiting: u32,
}

/// Compares the active list with the kept state `id`. The state must belong to the active profile and still be kept
/// for undo; a state that has been undone away or restored from before cannot be previewed.
pub fn preview_restore(root: &Data, game_id: &str, id: u32) -> Result<HistoryPreview, AppError> {
    games::load(game_id)?;
    let data = root.game(game_id);
    let Loaded {
        library, profiles, ..
    } = load_game(root, game_id)?;
    let library = library.as_ref();
    let active = profiles.active();
    let index = read_index(&data);
    let mark = index
        .undo
        .iter()
        .rev()
        .find(|mark| mark.id == id && mark.profile == active.id)
        .ok_or_else(|| AppError::user("history.unknown"))?;
    let state = read_state(&data, id)?;
    let difference = diff(&active.mods, &state.mods);
    let updated: FxHashSet<&str> = difference
        .updated
        .iter()
        .map(|(before, _)| before.id.as_str())
        .collect();
    let mod_of = |entry: &Entry| HistoryMod {
        id: entry.id.clone(),
        name: name_of(library, entry),
        folder: entry.folder.clone(),
        version: crate::library::version_of(library, entry),
        enabled: entry.enabled,
        missing: is_missing(library, entry),
    };
    let side = |entry: &Entry| HistorySide {
        folder: entry.folder.clone(),
        version: crate::library::version_of(library, entry),
        enabled: entry.enabled,
        missing: is_missing(library, entry),
    };

    Ok(HistoryPreview {
        id: mark.id,
        at: mark.at,
        total: mark.total,
        enabled: mark.enabled,
        adds: difference.added.iter().map(|entry| mod_of(entry)).collect(),
        removes: difference
            .removed
            .iter()
            .map(|entry| mod_of(entry))
            .collect(),
        versions: difference
            .updated
            .iter()
            .map(|(before, after)| HistoryVersionChange {
                id: after.id.clone(),
                name: name_of(library, after),
                from: side(before),
                to: side(after),
            })
            .collect(),
        switches_on: difference
            .enabled_on
            .iter()
            .filter(|entry| !updated.contains(entry.id.as_str()))
            .map(|entry| mod_of(entry))
            .collect(),
        switches_off: difference
            .enabled_off
            .iter()
            .filter(|entry| !updated.contains(entry.id.as_str()))
            .map(|entry| mod_of(entry))
            .collect(),
        moved: difference.moved,
        priority: difference.priority_changed,
        groups: separator_changes(&active.separators, &state.separators),
        winners: winner_changes(&active.winners, &state.winners),
        waiting: waiting_changes(&active.waiting, &state.waiting),
    })
}

fn name_of(library: &Library, entry: &Entry) -> String {
    library
        .families
        .get(&entry.id)
        .map(|family| family.name.clone())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            library
                .versions
                .get(&entry.folder)
                .map(|version| version.info.name.clone())
                .filter(|name| !name.is_empty())
        })
        .unwrap_or_else(|| entry.id.clone())
}

fn is_missing(library: &Library, entry: &Entry) -> bool {
    !library.families.contains_key(&entry.id) || !library.versions.contains_key(&entry.folder)
}
