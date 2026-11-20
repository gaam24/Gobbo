//! Sorting a list by the rules between mods. A rule says that one mod overrides another: it must stand above it, so its
//! copy of a shared file wins. Sorting keeps the current order wherever the rules allow, so a list with no broken rule
//! does not change, and a pack keeps its order as mods come and go.

use crate::context::Data;
use crate::context::Loaded;
use crate::context::load_game;
use crate::context::load_profiles;
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::domain::profiles::Entry as ProfileEntry;
use crate::error::AppError;
use crate::library::Library;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::collections::BinaryHeap;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SortMove {
    pub id: String,
    pub name: String,
    /// One-based place in the current list.
    pub from: u32,
    /// One-based place after sorting.
    pub to: u32,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SortPreview {
    pub total: u32,
    /// The mods whose place changes. Empty when the list already keeps every rule.
    pub moves: Vec<SortMove>,
}

/// What sorting the active profile by the rules would change. Nothing is written; the current order is the backup the
/// list history keeps when the sort runs.
pub fn preview_sort(root: &Data, game_id: &str) -> Result<SortPreview, AppError> {
    let Loaded {
        library, profiles, ..
    } = load_game(root, game_id)?;
    let current = &profiles.active().mods;
    let sorted = sorted_by_rules(current, &library)?;
    let position: BTreeMap<&str, usize> = current
        .iter()
        .enumerate()
        .map(|(at, entry)| (entry.id.as_str(), at))
        .collect();
    let mut moves = Vec::new();

    for (to, entry) in sorted.iter().enumerate() {
        let Some(&from) = position.get(entry.id.as_str()) else {
            continue;
        };

        if from == to {
            continue;
        }

        moves.push(SortMove {
            id: entry.id.clone(),
            name: library
                .families
                .get(&entry.id)
                .map_or_else(|| entry.id.clone(), |family| family.name.clone()),
            from: u32::try_from(from + 1).unwrap_or(u32::MAX),
            to: u32::try_from(to + 1).unwrap_or(u32::MAX),
        });
    }

    Ok(SortPreview {
        total: u32::try_from(current.len()).unwrap_or(u32::MAX),
        moves,
    })
}

/// Sorts the active profile's list so that every mod stands above the mods it overrides.
pub fn sort_by_rules(root: &Data, game_id: &str) -> Result<(), AppError> {
    let Loaded { data, library, .. } = load_game(root, game_id)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    let sorted = sorted_by_rules(&profiles.active().mods, &library)?;
    profiles.active_mut().mods = sorted;
    save_profiles(&data, &profiles)
}

/// Sorts the active profile alphabetically by mod name. A group stays together as one block, placed by its first
/// member; separators stay anchored to the mod they name. Each edit is one history entry, like a rule sort.
pub fn sort_by_name(root: &Data, game_id: &str) -> Result<(), AppError> {
    let Loaded { data, library, .. } = load_game(root, game_id)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    let names: BTreeMap<&str, String> = library
        .families
        .iter()
        .map(|(id, family)| (id.as_str(), family.name.to_lowercase()))
        .collect();
    let profile = profiles.active_mut();
    let groups = profile.groups.clone();
    profile.mods = sorted_by_name(std::mem::take(&mut profile.mods), &groups, &names);
    save_profiles(&data, &profiles)
}

/// The alphabetical order of a list. Each group moves as one block; a mod outside a group moves alone. `names` is the
/// lowercased display name of each family, so the pure rule needs no library or disk.
pub fn sorted_by_name(
    mods: Vec<ProfileEntry>,
    groups: &[crate::domain::profiles::ModGroup],
    names: &BTreeMap<&str, String>,
) -> Vec<ProfileEntry> {
    let mut owner: BTreeMap<&str, usize> = BTreeMap::new();

    for (at, group) in groups.iter().enumerate() {
        for member in &group.members {
            owner.insert(member.as_str(), at);
        }
    }

    let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut blocks: Vec<((String, String), Vec<ProfileEntry>)> = Vec::new();

    for entry in &mods {
        if used.contains(&entry.id) {
            continue;
        }
        // A mod outside a group is added by the second pass; do not mark it used here.
        let Some(&group) = owner.get(entry.id.as_str()) else {
            continue;
        };
        let members: Vec<ProfileEntry> = mods
            .iter()
            .filter(|candidate| owner.get(candidate.id.as_str()) == Some(&group))
            .cloned()
            .collect();
        for member in &members {
            used.insert(member.id.clone());
        }

        let block_key = members.first().map(|first| key(first, names));

        if let Some(block_key) = block_key {
            blocks.push((block_key, members));
        }
    }

    for entry in &mods {
        if used.contains(&entry.id) {
            continue;
        }

        blocks.push((key(entry, names), vec![entry.clone()]));
    }

    blocks.sort_by(|(left, _), (right, _)| left.cmp(right));
    blocks.into_iter().flat_map(|(_, block)| block).collect()
}

fn key(entry: &ProfileEntry, names: &BTreeMap<&str, String>) -> (String, String) {
    (
        names
            .get(entry.id.as_str())
            .cloned()
            .unwrap_or_else(|| entry.id.clone()),
        entry.id.clone(),
    )
}

/// The list in an order that keeps every rule: each mod above the ones it overrides. Of the orders that do, the one
/// closest to the current list: at each place the mod that stood highest and is free to go there.
pub fn sorted_by_rules(
    mods: &[ProfileEntry],
    library: &Library,
) -> Result<Vec<ProfileEntry>, AppError> {
    let position: BTreeMap<&str, usize> = mods
        .iter()
        .enumerate()
        .map(|(at, entry)| (entry.id.as_str(), at))
        .collect();
    // `below[a]` lists the mods that `a` overrides; `waiting[b]` counts the mods that must still go above `b`.
    let mut below: Vec<Vec<usize>> = vec![Vec::new(); mods.len()];
    let mut waiting = vec![0_usize; mods.len()];

    for (above, entry) in mods.iter().enumerate() {
        let overrides = library
            .families
            .get(&entry.id)
            .map(|family| family.overrides.as_slice())
            .unwrap_or_default();
        for target in overrides {
            if let Some(&under) = position.get(target.as_str())
                && under != above
            {
                below[above].push(under);
                waiting[under] += 1;
            }
        }
    }

    let mut free: BinaryHeap<Reverse<usize>> = (0..mods.len())
        .filter(|&at| waiting[at] == 0)
        .map(Reverse)
        .collect();
    let mut order = Vec::with_capacity(mods.len());

    while let Some(Reverse(at)) = free.pop() {
        order.push(at);

        for &under in &below[at] {
            waiting[under] -= 1;

            if waiting[under] == 0 {
                free.push(Reverse(under));
            }
        }
    }

    if order.len() < mods.len() {
        let names: Vec<String> = (0..mods.len())
            .filter(|&at| waiting[at] > 0)
            .map(|at| {
                library
                    .families
                    .get(&mods[at].id)
                    .map_or_else(|| mods[at].id.clone(), |f| f.name.clone())
            })
            .collect();
        return Err(AppError::user("order.cycle").with("mods", names.join(", ")));
    }

    Ok(order.into_iter().map(|at| mods[at].clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Family;

    fn entry(id: &str) -> ProfileEntry {
        ProfileEntry {
            id: id.into(),
            folder: id.into(),
            enabled: true,
            priority: None,
        }
    }

    fn library(rules: &[(&str, &[&str])]) -> Library {
        let mut library = Library::default();

        for (id, overrides) in rules {
            library.families.insert(
                (*id).into(),
                Family {
                    id: (*id).into(),
                    name: id.to_uppercase(),
                    overrides: overrides.iter().map(|other| (*other).to_string()).collect(),
                    ..Family::default()
                },
            );
        }

        library
    }

    fn ids(list: &[ProfileEntry]) -> Vec<&str> {
        list.iter().map(|entry| entry.id.as_str()).collect()
    }

    #[test]
    fn alphabetical_sort_moves_a_group_as_one_block() {
        let mods = ["zeta", "alpha", "middle"].map(entry);
        let names: BTreeMap<&str, String> =
            [("zeta", "Zebra"), ("alpha", "Apple"), ("middle", "Middle")]
                .into_iter()
                .map(|(id, name)| (id, name.to_lowercase()))
                .collect();
        let groups = [crate::domain::profiles::ModGroup {
            id: "grp1".into(),
            name: "Pair".into(),
            members: vec!["zeta".into(), "middle".into()],
            kind: crate::domain::profiles::GroupKind::Bundle,
            language: None,
        }];
        let sorted = sorted_by_name(mods.to_vec(), &groups, &names);
        // The group is placed by its first member ("Zebra"), after the lone "Apple".
        assert_eq!(ids(&sorted), ["alpha", "zeta", "middle"]);
    }

    #[test]
    fn a_list_that_keeps_the_rules_does_not_change() {
        let mods = ["a", "b", "c"].map(entry);
        let library = library(&[("a", &["c"])]);
        assert_eq!(
            ids(&sorted_by_rules(&mods, &library).unwrap()),
            ["a", "b", "c"]
        );
    }

    #[test]
    fn a_mod_moves_above_the_mods_it_overrides_and_the_rest_keeps_its_order() {
        let mods = ["a", "b", "c", "d"].map(entry);
        let library = library(&[("d", &["b"]), ("c", &["a"])]);
        assert_eq!(
            ids(&sorted_by_rules(&mods, &library).unwrap()),
            ["c", "a", "d", "b"]
        );
    }

    #[test]
    fn rules_about_mods_outside_the_list_are_ignored() {
        let mods = ["a", "b"].map(entry);
        let library = library(&[("a", &["gone"])]);
        assert_eq!(ids(&sorted_by_rules(&mods, &library).unwrap()), ["a", "b"]);
    }

    #[test]
    fn a_cycle_names_its_mods() {
        let mods = ["a", "b", "c"].map(entry);
        let library = library(&[("a", &["b"]), ("b", &["a"])]);
        let error = sorted_by_rules(&mods, &library).unwrap_err();
        assert_eq!(error.code, "order.cycle");
        assert_eq!(error.params.get("mods").map(String::as_str), Some("A, B"));
    }
}
