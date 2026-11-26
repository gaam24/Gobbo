//! The compact summary of one change to a profile's list. The history index keeps it, so opening the history does
//! not read every kept state. The restore preview is built separately from the state and the list as it is now.

use super::ListState;
use super::Separator;
use super::diff::diff;
use crate::domain::pack::WaitingMod;
use rustc_hash::FxHashMap;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;

/// What one change to a list did, in counts. A mod can be counted in more than one part when its version changed
/// and it was switched at the same time; the parts describe aspects, not distinct mods.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct HistorySummary {
    /// Mods the change brought onto the list.
    pub added: u32,
    /// Mods the change took off the list.
    pub removed: u32,
    /// Mods whose selected version folder changed.
    pub updated: u32,
    /// Mods that must change place so the others keep their relative order.
    pub moved: u32,
    /// Mods the change switched on.
    pub enabled_on: u32,
    /// Mods the change switched off.
    pub enabled_off: u32,
    /// Mods whose load priority changed.
    pub priority: u32,
    /// Separators added, removed, renamed or anchored above another mod.
    pub groups: u32,
    /// File winner choices changed.
    pub winners: u32,
    /// Pack mods waiting for the library changed.
    pub waiting: u32,
}

/// Describes one change of a profile's list.
pub fn summarize(old: &ListState, new: &ListState) -> HistorySummary {
    let diff = diff(&old.mods, &new.mods);
    HistorySummary {
        added: count(diff.added.len()),
        removed: count(diff.removed.len()),
        updated: count(diff.updated.len()),
        moved: diff.moved,
        enabled_on: count(diff.enabled_on.len()),
        enabled_off: count(diff.enabled_off.len()),
        priority: diff.priority_changed,
        groups: separator_changes(&old.separators, &new.separators),
        winners: winner_changes(&old.winners, &new.winners),
        waiting: waiting_changes(&old.waiting, &new.waiting),
    }
}

/// Separators added, removed or changed (a new name or another mod they sit above). The count is by separator id,
/// so a separator that only keeps its place is not reported.
pub fn separator_changes(old: &[Separator], new: &[Separator]) -> u32 {
    let old: FxHashMap<&str, &Separator> =
        old.iter().map(|item| (item.id.as_str(), item)).collect();
    let new: FxHashMap<&str, &Separator> =
        new.iter().map(|item| (item.id.as_str(), item)).collect();
    let changed = new
        .iter()
        .filter(|(id, item)| {
            old.get(*id)
                .copied()
                .is_none_or(|before| before.name != item.name || before.before != item.before)
        })
        .count()
        + old.keys().filter(|id| !new.contains_key(*id)).count();
    count(changed)
}

/// File winner choices added, removed or changed, by path.
pub fn winner_changes(old: &BTreeMap<String, String>, new: &BTreeMap<String, String>) -> u32 {
    let changed = new
        .iter()
        .filter(|(path, winner)| old.get(*path) != Some(*winner))
        .count()
        + old.keys().filter(|path| !new.contains_key(*path)).count();
    count(changed)
}

/// Pack mods waiting for the library added, removed or changed, by the family the pack names them by.
pub fn waiting_changes(old: &[WaitingMod], new: &[WaitingMod]) -> u32 {
    let old: FxHashMap<&str, &WaitingMod> = old.iter().map(|item| (item.key(), item)).collect();
    let new: FxHashMap<&str, &WaitingMod> = new.iter().map(|item| (item.key(), item)).collect();
    let changed = new
        .iter()
        .filter(|(key, item)| old.get(*key).copied() != Some(*item))
        .count()
        + old.keys().filter(|key| !new.contains_key(*key)).count();
    count(changed)
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profiles::ListedMod;
    use std::collections::BTreeMap;

    fn entry(id: &str, folder: &str, enabled: bool, priority: Option<u32>) -> ListedMod {
        ListedMod {
            id: id.into(),
            folder: folder.into(),
            enabled,
            priority,
        }
    }

    fn state(
        mods: Vec<ListedMod>,
        separators: Vec<Separator>,
        winners: BTreeMap<String, String>,
    ) -> ListState {
        ListState {
            mods,
            separators,
            groups: Vec::new(),
            winners,
            waiting: Vec::new(),
        }
    }

    #[test]
    fn a_summary_describes_the_mods_and_the_other_parts_of_the_list() {
        let old = state(
            vec![
                entry("a", "a1", true, Some(1)),
                entry("b", "b1", false, None),
                entry("c", "c1", true, None),
            ],
            vec![Separator {
                id: "s1".into(),
                name: "Old name".into(),
                before: Some("a".into()),
            }],
            BTreeMap::from([("path/one".to_string(), "a".to_string())]),
        );
        let new = state(
            vec![
                entry("a", "a1", false, Some(2)),
                entry("b", "b2", true, None),
                entry("d", "d1", true, None),
            ],
            vec![
                Separator {
                    id: "s1".into(),
                    name: "New name".into(),
                    before: Some("b".into()),
                },
                Separator {
                    id: "s2".into(),
                    name: "Extra".into(),
                    before: None,
                },
            ],
            BTreeMap::from([
                ("path/one".to_string(), "b".to_string()),
                ("path/two".to_string(), "a".to_string()),
            ]),
        );

        let summary = summarize(&old, &new);
        assert_eq!(summary.added, 1);
        assert_eq!(summary.removed, 1);
        assert_eq!(summary.updated, 1);
        assert_eq!(summary.moved, 0);
        assert_eq!(summary.enabled_on, 1);
        assert_eq!(summary.enabled_off, 1);
        assert_eq!(summary.priority, 1);
        assert_eq!(summary.groups, 2);
        assert_eq!(summary.winners, 2);
        assert_eq!(summary.waiting, 0);
        assert_ne!(summary, HistorySummary::default());
    }

    #[test]
    fn an_unchanged_list_has_an_empty_summary() {
        let state = state(
            vec![entry("a", "a1", true, None)],
            vec![Separator {
                id: "s1".into(),
                name: "Group".into(),
                before: Some("a".into()),
            }],
            BTreeMap::new(),
        );
        assert_eq!(summarize(&state, &state), HistorySummary::default());
    }

    #[test]
    fn a_renamed_separator_counts_once() {
        let old = vec![Separator {
            id: "s1".into(),
            name: "Old".into(),
            before: Some("a".into()),
        }];
        let new = vec![Separator {
            id: "s1".into(),
            name: "New".into(),
            before: Some("a".into()),
        }];
        assert_eq!(separator_changes(&old, &new), 1);
        assert_eq!(separator_changes(&old, &old), 0);
    }

    #[test]
    fn a_changed_file_winner_counts_once() {
        let old = BTreeMap::from([("a".to_string(), "x".to_string())]);
        let new = BTreeMap::from([("a".to_string(), "y".to_string())]);
        assert_eq!(winner_changes(&old, &new), 1);
        assert_eq!(winner_changes(&old, &old), 0);
    }
}
