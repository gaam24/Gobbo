//! The history of the mod lists: before every change to a profile's list, the list as it was is kept, so the change
//! can be undone and any earlier state restored. Each state is a file of its own under `history/`; a small index lists
//! them, so the status reads only the index.
//!
//! Undo takes the newest state of the profile back and keeps the current one for redo. Restoring a state from the list
//! keeps the current one as the newest state, so a restore can always be undone too.
//!
//! Every mark carries a [`HistorySummary`] of the change that left its state. The summary is written when the change
//! happens, so the history list reads only the index. The named preview of a restore is built on demand in
//! `preview.rs` from the kept state and the list as it is now.

use crate::context::Data;
use crate::context::load_profiles;
use crate::context::profile::lock_profiles;
use crate::domain::profiles::ListState;
use crate::domain::profiles::Profile;
use crate::domain::profiles::Profiles;
use crate::domain::profiles::history::HistorySummary;
use crate::domain::profiles::history::summarize;
use crate::error::AppError;
use crate::error::ErrorKind;
use crate::error::io_at;
use crate::games;
use crate::infra::clock::now_seconds;
use crate::infra::json::{self, ReadError};
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

const HISTORY_DIR: &str = "history";
const INDEX_FILE: &str = "index.json";
/// States kept for one profile. The oldest go first.
const MAX_STATES_PER_PROFILE: usize = 50;
/// States kept for all profiles of a game together. The oldest go first.
const MAX_STATES_TOTAL: usize = 150;

/// One kept state of a profile's list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mark {
    pub id: u32,
    /// Unix time in seconds when the list left this state.
    pub at: u32,
    pub profile: String,
    pub total: u32,
    pub enabled: u32,
    /// What the change that left this state did. Absent in indexes written before the summary existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<HistorySummary>,
}

/// The history index: the next state id and the undo and redo marks of every profile.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Index {
    pub next: u32,
    pub undo: Vec<Mark>,
    pub redo: Vec<Mark>,
}

/// A kept state in the view: when the list left it, and what the change that left it did.
#[derive(Debug, Serialize, specta::Type)]
pub struct HistoryEntry {
    pub id: u32,
    pub at: u32,
    pub total: u32,
    pub enabled: u32,
    /// What the change that left this state did. `None` for an entry written before summaries existed.
    pub summary: Option<HistorySummary>,
}

/// Whether the active profile has a change to undo or to redo.
#[derive(Debug, Default, Serialize, specta::Type)]
pub struct HistoryState {
    pub can_undo: bool,
    pub can_redo: bool,
}

fn dir(data: &Data) -> PathBuf {
    data.dir.join(HISTORY_DIR)
}

/// Reads the history index from disk, through the read-through cache when this handle has one.
pub fn read_index(data: &Data) -> Index {
    let path = dir(data).join(INDEX_FILE);
    let parse = || Ok::<Index, AppError>(json::read_json_or_default(&path));
    data.history_cache().map_or_else(
        || json::read_json_or_default(&path),
        |cache| cache.load_cloned(&path, parse).unwrap_or_default(),
    )
}

fn write_index(data: &Data, index: &Index) -> Result<(), AppError> {
    let path = dir(data).join(INDEX_FILE);
    json::write_json(&path, index)?;
    data.remember_history(&path, index);
    Ok(())
}

fn state_file(data: &Data, id: u32) -> PathBuf {
    dir(data).join(format!("{id}.json"))
}

/// Reads a kept list state by its mark id, or an error when the id is unknown or the file is damaged.
pub fn read_state(data: &Data, id: u32) -> Result<ListState, AppError> {
    let path = state_file(data, id);

    match json::read_json(&path) {
        Ok(Some(state)) => Ok(state),
        Ok(None) => Err(AppError::user("history.unknown")),
        Err(ReadError::Io(error)) => Err(io_at(&path)(error)),
        Err(ReadError::Parse(error)) => Err(AppError::new("history.invalid")
            .with("detail", error)
            .kind(ErrorKind::Permanent)),
    }
}

/// Keeps a list state and returns its mark with the summary of the change that leaves it.
fn keep(
    data: &Data,
    index: &mut Index,
    profile: &Profile,
    state: ListState,
    summary: HistorySummary,
) -> Result<Mark, AppError> {
    let mark = Mark {
        id: index.next,
        at: now_seconds(),
        profile: profile.id.clone(),
        total: u32::try_from(state.mods.len()).unwrap_or(u32::MAX),
        enabled: u32::try_from(state.mods.iter().filter(|entry| entry.enabled).count())
            .unwrap_or(u32::MAX),
        summary: Some(summary),
    };
    index.next += 1;
    fs::create_dir_all(dir(data)).map_err(io_at(&dir(data)))?;
    json::write_json(&state_file(data, mark.id), &state)?;
    Ok(mark)
}

fn forget(data: &Data, marks: impl IntoIterator<Item = Mark>) {
    for mark in marks {
        let _ = fs::remove_file(state_file(data, mark.id));
    }
}

/// Pushes a mark on the undo stack, dropping the oldest states over the per-profile and total limits.
fn push_undo(data: &Data, index: &mut Index, mark: Mark) {
    let profile = mark.profile.clone();
    index.undo.push(mark);
    let mut over = index
        .undo
        .iter()
        .filter(|mark| mark.profile == profile)
        .count()
        .saturating_sub(MAX_STATES_PER_PROFILE);
    if over > 0 {
        let mut dropped = Vec::new();
        index.undo.retain(|mark| {
            if over > 0 && mark.profile == profile {
                over -= 1;
                dropped.push(mark.clone());
                false
            } else {
                true
            }
        });
        forget(data, dropped);
    }

    let over = index.undo.len().saturating_sub(MAX_STATES_TOTAL);

    if over > 0 {
        let dropped: Vec<Mark> = index.undo.drain(..over).collect();
        forget(data, dropped);
    }
}

/// Takes every mark of one profile out of `marks`, keeping the rest in order.
fn take_profile(marks: &mut Vec<Mark>, profile: &str) -> Vec<Mark> {
    let (gone, kept): (Vec<Mark>, Vec<Mark>) =
        marks.drain(..).partition(|mark| mark.profile == profile);
    *marks = kept;
    gone
}

/// Drops the redo states of one profile: a new change makes them unreachable.
fn clear_redo(data: &Data, index: &mut Index, profile: &str) {
    forget(data, take_profile(&mut index.redo, profile));
}

/// Saves the profiles after a change to a list. Each profile whose list changed has its earlier list kept for undo.
pub fn save_profiles(data: &Data, profiles: &Profiles) -> Result<(), AppError> {
    let _profiles = lock_profiles();
    let before = Profiles::read(&data.profiles_file())?;
    let mut index = read_index(data);
    let mut changed = false;

    for old in before.iter().flat_map(|before| &before.profiles) {
        let Some(now) = profiles
            .profiles
            .iter()
            .find(|profile| profile.id == old.id)
        else {
            continue;
        };

        if now.same_list(old) {
            continue;
        }

        let before_state = old.list_state();
        let after_state = now.list_state();
        let summary = summarize(&before_state, &after_state);
        let mark = keep(data, &mut index, old, before_state, summary)?;
        push_undo(data, &mut index, mark);
        clear_redo(data, &mut index, &old.id);
        changed = true;
    }
    // The list is saved before the history mark. A failed list save must not leave a mark for a change that never
    // happened; a failed mark write only loses the undo entry for a change that is already on disk.
    data.save_profiles(profiles)?;

    if changed {
        write_index(data, &index)?;
    }

    Ok(())
}

/// Whether the active profile can undo or redo a change.
pub fn history_state(data: &Data, profile: &str) -> HistoryState {
    let index = read_index(data);
    HistoryState {
        can_undo: index.undo.iter().any(|mark| mark.profile == profile),
        can_redo: index.redo.iter().any(|mark| mark.profile == profile),
    }
}

/// The kept states of the active profile, newest first. The index carries the summary of each change, so the list
/// does not read the kept states themselves.
pub fn list_history(root: &Data, game_id: &str) -> Result<Vec<HistoryEntry>, AppError> {
    games::load(game_id)?;
    let data = root.game(game_id);
    let profiles = load_profiles(&data)?;
    let index = read_index(&data);
    Ok(index
        .undo
        .iter()
        .rev()
        .filter(|mark| mark.profile == profiles.active)
        .filter(|mark| state_file(&data, mark.id).is_file())
        .map(|mark| HistoryEntry {
            id: mark.id,
            at: mark.at,
            total: mark.total,
            enabled: mark.enabled,
            summary: mark.summary.clone(),
        })
        .collect())
}

/// Takes the active profile's list back one change. The current list waits for redo.
pub fn undo_list(root: &Data, game_id: &str) -> Result<(), AppError> {
    step(root, game_id, true)
}

/// Brings back the change the last undo took away.
pub fn redo_list(root: &Data, game_id: &str) -> Result<(), AppError> {
    step(root, game_id, false)
}

fn step(root: &Data, game_id: &str, back: bool) -> Result<(), AppError> {
    games::load(game_id)?;
    let data = root.game(game_id);
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    let mut index = read_index(&data);
    let active = profiles.active.clone();
    let from = if back {
        &mut index.undo
    } else {
        &mut index.redo
    };
    let Some(at) = from.iter().rposition(|mark| mark.profile == active) else {
        return Err(AppError::user("history.nothing"));
    };
    let mark = from.remove(at);
    let state = read_state(&data, mark.id)?;
    let current_state = profiles.active().list_state();
    let summary = summarize(&current_state, &state);
    let current = keep(&data, &mut index, profiles.active(), current_state, summary)?;
    profiles.active_mut().restore_list(state);
    // The list is saved first, as in `save_profiles`: a failed save keeps the mark and its state, so the step can be
    // tried again, and only drops the state just kept for the other direction.
    if let Err(error) = data.save_profiles(&profiles) {
        forget(&data, [current]);
        return Err(error);
    }

    if back {
        index.redo.push(current);
    } else {
        push_undo(&data, &mut index, current);
    }

    forget(&data, [mark]);
    write_index(&data, &index)
}

/// Restores a kept state of the active profile. The current list is kept first, so the restore can be undone.
pub fn restore_list(root: &Data, game_id: &str, id: u32) -> Result<(), AppError> {
    games::load(game_id)?;
    let data = root.game(game_id);
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    let index = read_index(&data);
    let known = index
        .undo
        .iter()
        .any(|mark| mark.id == id && mark.profile == profiles.active);
    if !known {
        return Err(AppError::user("history.unknown"));
    }

    let state = read_state(&data, id)?;
    profiles.active_mut().restore_list(state);
    save_profiles(&data, &profiles)
}

/// Drops every kept state of a profile. A deleted profile must not leave history behind, because a new profile can
/// reuse its id.
pub fn forget_profile(data: &Data, profile: &str) -> Result<(), AppError> {
    let mut index = read_index(data);
    let gone = take_profile(&mut index.undo, profile);
    let gone_redo = take_profile(&mut index.redo, profile);
    forget(data, gone.into_iter().chain(gone_redo));
    write_index(data, &index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use crate::test_support::write;

    fn mark(id: u32, profile: &str) -> Mark {
        Mark {
            id,
            at: 0,
            profile: profile.to_string(),
            total: 0,
            enabled: 0,
            summary: None,
        }
    }

    #[test]
    fn forgetting_marks_removes_only_their_state_files() {
        let base = scratch!("forget");
        let data = Data::new(base);
        write(&state_file(&data, 1), "{}");
        write(&state_file(&data, 2), "{}");

        forget(&data, [mark(1, "p1")]);

        assert!(!state_file(&data, 1).exists());
        assert!(state_file(&data, 2).exists());
    }

    #[test]
    fn a_profile_keeps_its_own_limit_and_the_oldest_go_first() {
        let base = scratch!("limit");
        let data = Data::new(base);
        let mut index = Index::default();
        let count = u32::try_from(MAX_STATES_PER_PROFILE).unwrap() + 2;

        for id in 0..count {
            write(&state_file(&data, id), "{}");
            push_undo(&data, &mut index, mark(id, "p1"));
        }

        assert_eq!(index.undo.len(), MAX_STATES_PER_PROFILE);
        assert!(index.undo.iter().all(|mark| mark.id >= 2));
        assert!(!state_file(&data, 0).exists());
        assert!(!state_file(&data, 1).exists());
        assert!(state_file(&data, 2).exists());
    }

    #[test]
    fn forgetting_a_profile_removes_only_its_states() {
        let base = scratch!("forget-profile");
        let data = Data::new(base);
        let index = Index {
            undo: vec![mark(1, "p1"), mark(2, "p2"), mark(3, "p1")],
            redo: vec![mark(4, "p1"), mark(5, "p2")],
            ..Index::default()
        };
        write_index(&data, &index).unwrap();

        for id in 1..=5 {
            write(&state_file(&data, id), "{}");
        }

        forget_profile(&data, "p1").unwrap();

        let index = read_index(&data);
        assert_eq!(index.undo.len(), 1);
        assert_eq!(index.undo[0].id, 2);
        assert_eq!(index.redo.len(), 1);
        assert_eq!(index.redo[0].id, 5);
        assert!(!state_file(&data, 1).exists());
        assert!(state_file(&data, 2).exists());
        assert!(!state_file(&data, 3).exists());
        assert!(!state_file(&data, 4).exists());
        assert!(state_file(&data, 5).exists());
    }

    #[test]
    fn a_failed_history_write_still_saves_the_new_list() {
        let base = scratch!("history-order");
        let data = Data::new(base);
        let mut after = Profiles::with_default(Vec::new());
        data.save_profiles(&after).unwrap();
        after
            .active_mut()
            .mods
            .push(crate::domain::profiles::ListedMod {
                id: "a".into(),
                folder: "a".into(),
                enabled: true,
                priority: None,
            });
        let history = dir(&data);
        std::fs::create_dir_all(&history).unwrap();
        // A directory where the index file belongs makes the history write fail after the list save.
        std::fs::create_dir(history.join(INDEX_FILE)).unwrap();

        assert!(save_profiles(&data, &after).is_err());
        let saved = Profiles::read_or_default(&data.profiles_file()).unwrap();
        assert_eq!(saved.active().mods.len(), 1);
    }
}
