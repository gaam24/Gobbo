use std::collections::{BTreeMap, BTreeSet};

use crate::error::{AppError, ErrorKind, MOD_MISSING};

use super::super::group::{GroupKind, ModGroup};
use super::super::profile::{Entry, Profile, Separator};
use super::next_separator_id;

/// A family id that is not on the list. The domain cannot reach the library helper, so it builds the same permanent
/// missing-mod failure with the shared code.
fn not_on_list(id: &str) -> AppError {
    AppError::new(MOD_MISSING)
        .with("folder", id)
        .kind(ErrorKind::Permanent)
}

/// The page key of every family, as the window derives it: `provider:mod_id`. A command builds this from the library
/// before it takes the profile lock, so the list-edit rules below stay pure and never read the index.
pub type PageKeys = BTreeMap<String, String>;

/// Which side of the target a mod or block lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveSide {
    /// Directly above the target block.
    Above,
    /// Directly below the target block.
    Below,
}

impl MoveSide {
    /// The side named by `text`. Anything other than `below` is `above`, so a stale window cannot pick an unknown side
    /// and still gets a predictable move.
    pub fn parse(text: &str) -> Self {
        if text == "below" {
            Self::Below
        } else {
            Self::Above
        }
    }
}

/// The family ids that move or toggle together with `id`, in list order. A stored group wins: every member of it that
/// is on the list moves as one. Otherwise a run of at least two adjacent, ungrouped mods that share a page key moves as
/// one, as the window's `movedPage` does. A mod on its own is its own block.
fn block_of(mods: &[Entry], groups: &[ModGroup], pages: &PageKeys, id: &str) -> Vec<String> {
    if let Some(group) = groups
        .iter()
        .find(|group| group.members.iter().any(|member| member == id))
    {
        let members: BTreeSet<&str> = group.members.iter().map(String::as_str).collect();
        let block: Vec<String> = mods
            .iter()
            .filter(|entry| members.contains(entry.id.as_str()))
            .map(|entry| entry.id.clone())
            .collect();
        if !block.is_empty() {
            return block;
        }
    }

    let Some(key) = pages.get(id) else {
        return vec![id.to_string()];
    };
    let grouped: BTreeSet<&str> = groups
        .iter()
        .flat_map(|group| group.members.iter().map(String::as_str))
        .collect();
    // A mod that belongs to a group is never part of a page run: the window shows a group header in its place.
    let same_page = |at: usize| {
        mods.get(at).is_some_and(|entry| {
            !grouped.contains(entry.id.as_str()) && pages.get(entry.id.as_str()) == Some(key)
        })
    };
    let start = match mods.iter().position(|entry| entry.id == id) {
        Some(at) if same_page(at) => at,
        _ => return vec![id.to_string()],
    };
    let mut first = start;

    while first > 0 && same_page(first - 1) {
        first -= 1;
    }

    let mut end = start + 1;

    while same_page(end) {
        end += 1;
    }

    if end - first < 2 {
        return vec![id.to_string()];
    }

    mods[first..end]
        .iter()
        .map(|entry| entry.id.clone())
        .collect()
}

/// Reorders the active list so the block containing `id` sits directly above or below the block containing `target`.
/// Separators keep their anchor and groups keep their members, since both name families rather than positions. Returns
/// a user error when either id is not on the list. Dropping a block on itself changes nothing.
pub fn move_block(
    profile: &mut Profile,
    pages: &PageKeys,
    id: &str,
    target: &str,
    side: MoveSide,
) -> Result<(), AppError> {
    if !profile.mods.iter().any(|entry| entry.id == id) {
        return Err(not_on_list(id));
    }

    if !profile.mods.iter().any(|entry| entry.id == target) {
        return Err(not_on_list(target));
    }

    let moving = block_of(&profile.mods, &profile.groups, pages, id);
    let anchor = block_of(&profile.mods, &profile.groups, pages, target);
    let moving_set: BTreeSet<&str> = moving.iter().map(String::as_str).collect();

    if anchor
        .iter()
        .any(|member| moving_set.contains(member.as_str()))
    {
        return Ok(());
    }

    let anchor_set: BTreeSet<&str> = anchor.iter().map(String::as_str).collect();
    let mut rest = Vec::with_capacity(profile.mods.len());
    let mut taken = Vec::with_capacity(moving.len());

    for entry in std::mem::take(&mut profile.mods) {
        if moving_set.contains(entry.id.as_str()) {
            taken.push(entry);
        } else {
            rest.push(entry);
        }
    }

    let places: Vec<usize> = rest
        .iter()
        .enumerate()
        .filter(|(_, entry)| anchor_set.contains(entry.id.as_str()))
        .map(|(at, _)| at)
        .collect();
    let at = match side {
        MoveSide::Above => places.first().copied(),
        MoveSide::Below => places.last().map(|last| last + 1),
    }
    .unwrap_or(rest.len());
    rest.splice(at..at, taken);
    profile.mods = rest;
    Ok(())
}

/// Sets `enabled` on every mod of the block containing `id`, so a page or group header toggles all of its parts.
pub fn set_block_enabled(
    profile: &mut Profile,
    pages: &PageKeys,
    id: &str,
    enabled: bool,
) -> Result<(), AppError> {
    if !profile.mods.iter().any(|entry| entry.id == id) {
        return Err(not_on_list(id));
    }

    let block = block_of(&profile.mods, &profile.groups, pages, id);
    let wanted: BTreeSet<&str> = block.iter().map(String::as_str).collect();

    for entry in &mut profile.mods {
        if wanted.contains(entry.id.as_str()) {
            entry.enabled = enabled;
        }
    }

    Ok(())
}

/// Adds a group holding `members`, with the id and name the caller chose. It refuses a group with fewer than two
/// distinct, listed members, and moves a mod that belonged to another group into the new one.
pub fn add_group(
    profile: &mut Profile,
    id: &str,
    name: &str,
    members: &[String],
) -> Result<(), AppError> {
    let listed: BTreeSet<&str> = profile.mods.iter().map(|entry| entry.id.as_str()).collect();
    let mut unique = Vec::with_capacity(members.len());
    let mut seen = BTreeSet::new();

    for member in members {
        if !listed.contains(member.as_str()) {
            return Err(not_on_list(member));
        }

        if seen.insert(member.as_str()) {
            unique.push(member.clone());
        }
    }

    if unique.len() < 2 {
        return Err(AppError::user("groups.needs_pair").with("id", id));
    }

    if profile.groups.iter().any(|group| group.id == id) {
        return Err(AppError::user("groups.invalid").with("id", id));
    }

    let wanted: BTreeSet<&str> = unique.iter().map(String::as_str).collect();

    for group in &mut profile.groups {
        group
            .members
            .retain(|member| !wanted.contains(member.as_str()));
    }

    profile.groups.retain(|group| group.members.len() >= 2);
    profile.groups.push(ModGroup {
        id: id.to_string(),
        name: name.to_string(),
        members: unique,
        kind: GroupKind::Bundle,
        language: None,
    });
    Ok(())
}

/// Removes the group `id`, leaving its mods on the list where they are.
pub fn remove_group(profile: &mut Profile, id: &str) -> Result<(), AppError> {
    let before = profile.groups.len();
    profile.groups.retain(|group| group.id != id);

    if profile.groups.len() == before {
        return Err(AppError::user("groups.invalid").with("id", id));
    }

    Ok(())
}

/// Puts `id` in `group_id`, or takes it out of every group when `group_id` is `None`. A group left with fewer than two
/// members is removed, so the list never carries a one-member group.
pub fn set_mod_group(
    profile: &mut Profile,
    id: &str,
    group_id: Option<&str>,
) -> Result<(), AppError> {
    if !profile.mods.iter().any(|entry| entry.id == id) {
        return Err(not_on_list(id));
    }

    if let Some(wanted) = group_id
        && !profile.groups.iter().any(|group| group.id == wanted)
    {
        return Err(AppError::user("groups.invalid").with("id", wanted));
    }

    for group in &mut profile.groups {
        group.members.retain(|member| member != id);
    }

    if let Some(wanted) = group_id
        && let Some(group) = profile.groups.iter_mut().find(|group| group.id == wanted)
        && !group.members.iter().any(|member| member == id)
    {
        group.members.push(id.to_string());
    }

    profile.groups.retain(|group| group.members.len() >= 2);
    Ok(())
}

/// Renames the separator `id`, or adds a separator named `name` above the block containing `id` when `id` is a mod.
/// A new separator anchors to the first member of that block, so it never splits a page run or a stored group.
pub fn set_separator(
    profile: &mut Profile,
    pages: &PageKeys,
    id: &str,
    name: &str,
) -> Result<(), AppError> {
    let name = name.trim();

    if name.is_empty() || name.chars().count() > 60 {
        return Err(AppError::user("profile.separator_name"));
    }

    if let Some(separator) = profile
        .separators
        .iter_mut()
        .find(|separator| separator.id == id)
    {
        separator.name = name.to_string();
        return Ok(());
    }

    if !profile.mods.iter().any(|entry| entry.id == id) {
        return Err(not_on_list(id));
    }

    let block = block_of(&profile.mods, &profile.groups, pages, id);
    let anchor = block.first().cloned().unwrap_or_else(|| id.to_string());
    let separator_id = next_separator_id(profile);
    profile.separators.push(Separator {
        id: separator_id,
        name: name.to_string(),
        before: Some(anchor),
    });
    Ok(())
}

/// Brings a saved list back into a consistent state: stale and duplicate group members go, a mod stays in one group
/// only, and a group left with fewer than two members is removed. This is the check a full list save needs, since the
/// window sends mods and separators but not the groups.
pub fn normalize_groups(profile: &mut Profile) {
    let listed: BTreeSet<&str> = profile.mods.iter().map(|entry| entry.id.as_str()).collect();
    let mut used: BTreeSet<String> = BTreeSet::new();
    let mut kept = Vec::with_capacity(profile.groups.len());

    for group in std::mem::take(&mut profile.groups) {
        let mut local = BTreeSet::new();
        let mut members = Vec::with_capacity(group.members.len());

        for member in group.members {
            if listed.contains(member.as_str())
                && !used.contains(&member)
                && local.insert(member.clone())
            {
                members.push(member);
            }
        }

        if members.len() >= 2 {
            used.extend(members.iter().cloned());
            kept.push(ModGroup { members, ..group });
        }
    }

    profile.groups = kept;
}
