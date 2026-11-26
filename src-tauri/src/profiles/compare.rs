//! Comparing two profiles of one game: the mods that only one of them lists, and the mods both list with another
//! version or state. The order of the lists is not compared.

use crate::context::Data;
use crate::context::Loaded;
use crate::context::load_game;
use crate::domain::profiles::ListedMod;
use crate::error::AppError;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// One profile's side of a mod: the version folder it selects, its version as the mod says it, and whether it is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct CompareSide {
    pub folder: String,
    pub version: Option<String>,
    pub enabled: bool,
}

/// A mod that differs between the two profiles. A side is `None` when its profile does not list the mod.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CompareRow {
    pub id: String,
    pub name: String,
    pub first: Option<CompareSide>,
    pub second: Option<CompareSide>,
}

/// The comparison of two profiles: the rows that differ, and how many mods are the same in both.
#[derive(Debug, Serialize, specta::Type)]
pub struct ProfileCompare {
    pub first_name: String,
    pub second_name: String,
    pub rows: Vec<CompareRow>,
    pub same: u32,
}

/// Compares the profiles `first` and `second`. Rows come in the order of `first`, then the mods only `second` lists.
pub fn compare_profiles(
    root: &Data,
    game_id: &str,
    first: &str,
    second: &str,
) -> Result<ProfileCompare, AppError> {
    let Loaded {
        library, profiles, ..
    } = load_game(root, game_id)?;
    let left = profiles.profile(first)?;
    let right = profiles.profile(second)?;

    let side = |entry: &ListedMod| CompareSide {
        folder: entry.folder.clone(),
        version: crate::library::version_of(&library, entry),
        enabled: entry.enabled,
    };
    // Build the right list's index once; the old lookup walked it for every entry of the left list.
    let mut right_by_id: BTreeMap<&str, &ListedMod> = BTreeMap::new();

    for entry in &right.mods {
        right_by_id.entry(entry.id.as_str()).or_insert(entry);
    }

    let left_ids: BTreeSet<&str> = left.mods.iter().map(|entry| entry.id.as_str()).collect();

    let mut rows = Vec::new();
    let mut same: u32 = 0;

    for entry in &left.mods {
        let first_side = side(entry);
        let second_side = right_by_id.get(entry.id.as_str()).map(|entry| side(entry));

        if second_side.as_ref() == Some(&first_side) {
            same += 1;
            continue;
        }

        rows.push(CompareRow {
            id: entry.id.clone(),
            name: crate::library::listed_mod_name(&library, entry),
            first: Some(first_side),
            second: second_side,
        });
    }

    for entry in &right.mods {
        if left_ids.contains(entry.id.as_str()) {
            continue;
        }

        rows.push(CompareRow {
            id: entry.id.clone(),
            name: crate::library::listed_mod_name(&library, entry),
            first: None,
            second: Some(side(entry)),
        });
    }

    Ok(ProfileCompare {
        first_name: left.name.clone(),
        second_name: right.name.clone(),
        rows,
        same,
    })
}
