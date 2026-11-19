//! The one comparison of two profile lists. The status, the history, the deployment preview and the pack update all
//! ask the same questions, so they share the answer instead of each keeping its own scan.

use rustc_hash::{FxHashMap, FxHashSet};

use super::Entry;

/// What differs between an old list and a new one. The references borrow the two input slices.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ListDiff<'a> {
    /// Entries only the new list has, in its order.
    pub added: Vec<&'a Entry>,
    /// Entries only the old list has, in its order.
    pub removed: Vec<&'a Entry>,
    /// Entries both lists have with another version folder. The pairs are `(old, new)`.
    pub updated: Vec<(&'a Entry, &'a Entry)>,
    /// Entries that must change place so the others keep their relative order. A move counts once, and an
    /// insertion or removal does not move the entries around it. A version change is counted in `updated`, not here.
    pub moved: u32,
    /// Entries both lists have that are switched on only in the new list; the new entry is the side the view shows.
    pub enabled_on: Vec<&'a Entry>,
    /// Entries both lists have that are switched off only in the new list.
    pub enabled_off: Vec<&'a Entry>,
    /// Entries both lists have with another load priority.
    pub priority_changed: u32,
    /// Entries whose place, version or presence changed. This is the count the status reports as draft changes.
    pub changed: u32,
}

/// The entries both lists share, in the new order, with the old place of each. The map and set building is shared by
/// `changed_count` and `diff`.
struct Prepared<'a> {
    new_ids: FxHashSet<&'a str>,
    common: Vec<(usize, &'a Entry, &'a Entry)>,
    added: Vec<&'a Entry>,
}

fn prepare<'a>(old: &'a [Entry], new: impl IntoIterator<Item = &'a Entry>) -> Prepared<'a> {
    let mut old_at: FxHashMap<&'a str, (usize, &'a Entry)> = FxHashMap::default();
    old_at.reserve(old.len());
    for (at, entry) in old.iter().enumerate() {
        old_at.insert(entry.id.as_str(), (at, entry));
    }
    let mut new_ids: FxHashSet<&'a str> = FxHashSet::default();
    let mut common: Vec<(usize, &'a Entry, &'a Entry)> = Vec::with_capacity(old.len());
    let mut added = Vec::new();
    for entry in new {
        new_ids.insert(entry.id.as_str());
        match old_at.get(entry.id.as_str()) {
            Some((at, before)) => common.push((*at, *before, entry)),
            None => added.push(entry),
        }
    }
    Prepared {
        new_ids,
        common,
        added,
    }
}

/// The number of entries whose place, version or presence changed. The same rule as `diff(...).changed`, without
/// building the lists of added, removed and updated entries that a caller interested only in the count never reads.
#[cfg(test)]
pub fn changed_count(old: &[Entry], new: &[Entry]) -> u32 {
    changed_count_filtered(old, new, |_| true)
}

/// The same count after `keep` picks the new entries. Borrowing the new list lets a caller count only the enabled
/// entries without cloning them into an owned `Vec<Entry>`.
pub fn changed_count_filtered<'a>(
    old: &'a [Entry],
    new: impl IntoIterator<Item = &'a Entry>,
    mut keep: impl FnMut(&Entry) -> bool,
) -> u32 {
    let prepared = prepare(old, new.into_iter().filter(move |entry| keep(entry)));
    let updated = prepared
        .common
        .iter()
        .filter(|(_, before, after)| before.folder != after.folder)
        .count();
    let removed = old
        .iter()
        .filter(|entry| !prepared.new_ids.contains(entry.id.as_str()))
        .count();
    let changed = prepared.added.len()
        + removed
        + updated
        + usize::try_from(moved(&prepared.common)).unwrap_or(usize::MAX);
    u32::try_from(changed).unwrap_or(u32::MAX)
}

/// Compares two lists by family id. A family that appears twice keeps the last entry, as the map-based drafts did.
pub fn diff<'a>(old: &'a [Entry], new: &'a [Entry]) -> ListDiff<'a> {
    let prepared = prepare(old, new);
    let mut result = ListDiff::default();
    for (_, before, entry) in &prepared.common {
        if before.folder != entry.folder {
            result.updated.push((*before, entry));
        }
        if before.enabled != entry.enabled {
            if entry.enabled {
                result.enabled_on.push(entry);
            } else {
                result.enabled_off.push(entry);
            }
        }
        if before.priority != entry.priority {
            result.priority_changed += 1;
        }
    }
    result.added = prepared.added;
    for entry in old {
        if !prepared.new_ids.contains(entry.id.as_str()) {
            result.removed.push(entry);
        }
    }
    result.moved = moved(&prepared.common);
    result.changed = u32::try_from(
        result.added.len()
            + result.removed.len()
            + result.updated.len()
            + usize::try_from(result.moved).unwrap_or(usize::MAX),
    )
    .unwrap_or(u32::MAX);
    result
}

/// The common entries that must change place so the rest keep their relative order. A version change is already
/// counted in `updated`, so its entry is not counted here too.
fn moved(common: &[(usize, &Entry, &Entry)]) -> u32 {
    if common.len() < 2 {
        return 0;
    }
    let positions: Vec<usize> = common.iter().map(|(at, _, _)| *at).collect();
    let in_order = longest_increasing(&positions);
    let count = common
        .iter()
        .enumerate()
        .filter(|(index, (_, before, after))| !in_order[*index] && before.folder == after.folder)
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Marks one longest strictly increasing subsequence of `positions`. `tails[k]` is the smallest last value of an
/// increasing subsequence of length `k + 1`; `parent` lets the caller walk the chosen subsequence back.
fn longest_increasing(positions: &[usize]) -> Vec<bool> {
    let mut tails: Vec<usize> = Vec::new();
    let mut tail_at: Vec<usize> = Vec::new();
    let mut parent: Vec<Option<usize>> = vec![None; positions.len()];
    for (index, &position) in positions.iter().enumerate() {
        let at = tails.partition_point(|&tail| tail < position);
        if at > 0 {
            parent[index] = Some(tail_at[at - 1]);
        }
        if at == tails.len() {
            tails.push(position);
            tail_at.push(index);
        } else {
            tails[at] = position;
            tail_at[at] = index;
        }
    }
    let mut in_order = vec![false; positions.len()];
    let mut current = tail_at.last().copied();
    while let Some(index) = current {
        in_order[index] = true;
        current = parent[index];
    }
    in_order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, folder: &str, enabled: bool) -> Entry {
        Entry {
            id: id.into(),
            folder: folder.into(),
            enabled,
            priority: None,
        }
    }

    #[test]
    fn a_filtered_count_only_counts_kept_entries() {
        let old = [entry("a", "a1", true)];
        let new = [entry("a", "a1", true), entry("d", "d1", false)];

        // The disabled addition counts when nothing is filtered out.
        assert_eq!(changed_count_filtered(&old, &new, |_| true), 1);
        // It stops counting once the filter keeps only enabled new entries.
        assert_eq!(changed_count_filtered(&old, &new, |entry| entry.enabled), 0);
    }

    #[test]
    fn added_removed_updated_and_moved_are_one_change_each() {
        let old = [
            entry("a", "a1", true),
            entry("b", "b1", true),
            entry("c", "c1", true),
        ];
        let new = [
            entry("c", "c1", true), // moved
            entry("a", "a2", true), // updated (also moved; one change)
            entry("d", "d1", true), // added
        ];
        let diff = diff(&old, &new);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.updated.len(), 1);
        assert_eq!(diff.moved, 1);
        assert_eq!(diff.changed, 4);
    }

    #[test]
    fn a_switched_flag_is_separate_from_a_place_change() {
        let old = [entry("a", "a1", true), entry("b", "b1", false)];
        let new = [entry("a", "a1", false), entry("b", "b2", false)];
        let diff = diff(&old, &new);
        assert_eq!(diff.enabled_off.len(), 1);
        assert!(diff.enabled_on.is_empty());
        assert_eq!(diff.updated.len(), 1);
        assert_eq!(diff.changed, 1);
    }

    #[test]
    fn an_insertion_does_not_move_the_entries_around_it() {
        let old = [entry("a", "a1", true), entry("b", "b1", true)];
        let new = [
            entry("x", "x1", true),
            entry("a", "a1", true),
            entry("b", "b1", true),
        ];
        let diff = diff(&old, &new);
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.moved, 0);
        assert_eq!(diff.changed, 1);
    }

    #[test]
    fn a_removal_does_not_move_the_entries_around_it() {
        let old = [
            entry("a", "a1", true),
            entry("b", "b1", true),
            entry("c", "c1", true),
        ];
        let new = [entry("a", "a1", true), entry("c", "c1", true)];
        let diff = diff(&old, &new);
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.moved, 0);
        assert_eq!(diff.changed, 1);
    }

    #[test]
    fn a_swap_is_one_move() {
        let old = [entry("a", "a1", true), entry("b", "b1", true)];
        let new = [entry("b", "b1", true), entry("a", "a1", true)];
        let diff = diff(&old, &new);
        assert_eq!(diff.moved, 1);
        assert_eq!(diff.changed, 1);
    }

    #[test]
    fn a_version_change_alone_is_not_a_move() {
        let old = [entry("a", "a1", true), entry("b", "b1", true)];
        let new = [entry("a", "a2", true), entry("b", "b1", true)];
        let diff = diff(&old, &new);
        assert_eq!(diff.updated.len(), 1);
        assert_eq!(diff.moved, 0);
        assert_eq!(diff.changed, 1);
    }

    #[test]
    fn a_priority_change_is_counted_without_changing_the_position() {
        let old = [Entry {
            priority: Some(1),
            ..entry("a", "a1", true)
        }];
        let new = [Entry {
            priority: Some(2),
            ..entry("a", "a1", true)
        }];
        let diff = diff(&old, &new);
        assert_eq!(diff.priority_changed, 1);
        assert_eq!(diff.moved, 0);
        assert_eq!(diff.changed, 0);
    }
}
