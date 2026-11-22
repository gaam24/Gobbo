//! Thin use cases behind the mod-list edit commands. Each reads the library for page keys before it takes the profile
//! lock, applies one pure rule from `domain::profiles` under that lock, and saves the list as one history entry. The
//! index lock is never held while profiles are held: the library is read through the shared, read-only cache.

use crate::context::Data;
use crate::context::load_library_shared;
use crate::context::load_profiles;
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::domain::profiles::{
    MoveSide, PageKeys, add_group, move_block, next_group_id, remove_group, set_block_enabled,
    set_mod_group as set_mod_group_rule, set_separator as set_separator_rule,
};
use crate::error::AppError;
use crate::library::Library;

/// The game's data folder and the page key of every family in its library. The page keys are read before the profile
/// lock is taken, so a change never holds the index while it changes profiles.
fn game_pages(root: &Data, game: &str) -> Result<(Data, PageKeys), AppError> {
    let definition = root.definition(game)?;
    let data = root.game(game);
    let library = load_library_shared(&data, &definition)?;
    Ok((data, page_keys(&library)))
}

/// Builds `provider:mod_id` for each family, preferring the source with read page data as the status row does.
fn page_keys(library: &Library) -> PageKeys {
    library
        .families
        .iter()
        .filter_map(|(id, family)| {
            let (provider, source) = family
                .sources
                .iter()
                .find(|(_, source)| source.meta.is_some())
                .or_else(|| family.sources.iter().next())?;
            Some((id.clone(), format!("{provider}:{}", source.mod_id)))
        })
        .collect()
}

/// Moves the block containing `id` to just above or below the block containing `target`.
pub fn move_mod(
    root: &Data,
    game: &str,
    id: &str,
    target: &str,
    side: &str,
) -> Result<(), AppError> {
    let (data, pages) = game_pages(root, game)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    move_block(
        profiles.active_mut(),
        &pages,
        id,
        target,
        MoveSide::parse(side),
    )?;
    save_profiles(&data, &profiles)
}

/// Enables or disables every part of the page or group header `id`.
pub fn set_mod_enabled(root: &Data, game: &str, id: &str, enabled: bool) -> Result<(), AppError> {
    let (data, pages) = game_pages(root, game)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    set_block_enabled(profiles.active_mut(), &pages, id, enabled)?;
    save_profiles(&data, &profiles)
}

/// Selects the library version `folder` for its family and enables it in the active profile, and disables the entries
/// that use one of the `others` versions. Only those entries change, not their groups: the loader setup uses this to
/// keep one copy of a loader enabled, and a group with an unrelated mod must not follow it.
pub fn use_only(root: &Data, game: &str, folder: &str, others: &[String]) -> Result<(), AppError> {
    let definition = root.definition(game)?;
    let data = root.game(game);
    let library = load_library_shared(&data, &definition)?;
    let family = library
        .versions
        .get(folder)
        .map(|version| version.family.clone())
        .ok_or_else(|| AppError::user("library.version_unknown").with("folder", folder))?;
    drop(library);
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    profiles.active_entry_mut(&family)?;

    for entry in &mut profiles.active_mut().mods {
        if entry.id == family {
            entry.folder = folder.to_string();
            entry.enabled = true;
        } else if others.contains(&entry.folder) {
            entry.enabled = false;
        }
    }

    save_profiles(&data, &profiles)
}

/// Creates one group from `ids`. A selection of fewer than two distinct mods is refused by the rule.
pub fn group_mods(root: &Data, game: &str, ids: &[String]) -> Result<(), AppError> {
    let (data, _pages) = game_pages(root, game)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    let id = next_group_id(profiles.active());
    add_group(profiles.active_mut(), &id, &id, ids)?;
    save_profiles(&data, &profiles)
}

/// Removes a group, leaving its mods where they are on the list.
pub fn ungroup_mods(root: &Data, game: &str, group_id: &str) -> Result<(), AppError> {
    crate::games::load(game)?;
    let data = root.game(game);
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    remove_group(profiles.active_mut(), group_id)?;
    save_profiles(&data, &profiles)
}

/// Adds or renames a separator, anchoring a new one at a block boundary.
pub fn set_separator(root: &Data, game: &str, id: &str, name: &str) -> Result<(), AppError> {
    let (data, pages) = game_pages(root, game)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    set_separator_rule(profiles.active_mut(), &pages, id, name)?;
    save_profiles(&data, &profiles)
}

/// Puts one mod into a group or takes it out of every group.
pub fn set_mod_group(
    root: &Data,
    game: &str,
    id: &str,
    group_id: Option<&str>,
) -> Result<(), AppError> {
    let (data, _pages) = game_pages(root, game)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    set_mod_group_rule(profiles.active_mut(), id, group_id)?;
    save_profiles(&data, &profiles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profiles::{Entry, GroupKind, ModGroup, Profiles};
    use crate::test_support::{games as test_games, scratch, write};

    fn entry(id: &str) -> Entry {
        Entry {
            id: id.to_string(),
            folder: id.to_string(),
            enabled: true,
            priority: None,
        }
    }

    fn group(id: &str, members: &[&str]) -> ModGroup {
        ModGroup {
            id: id.to_string(),
            name: id.to_string(),
            members: members.iter().map(ToString::to_string).collect(),
            kind: GroupKind::Bundle,
            language: None,
        }
    }

    fn setup(name: &str) -> Data {
        test_games::install();
        let data = Data::new(scratch!(name));
        let profiles = Profiles::with_default(vec![entry("mod-a"), entry("mod-b"), entry("mod-c")]);
        let path = data.game(test_games::SLOT_ID).profiles_file();
        write(&path, &serde_json::to_string(&profiles).unwrap());
        data
    }

    fn saved(data: &Data) -> crate::domain::profiles::Profile {
        load_profiles(&data.game(test_games::SLOT_ID))
            .unwrap()
            .active()
            .clone()
    }

    #[test]
    fn a_move_is_saved_as_a_new_order() {
        let data = setup("edits-move");
        move_mod(&data, test_games::SLOT_ID, "mod-a", "mod-c", "below").unwrap();
        let list = saved(&data);
        let ids: Vec<&str> = list.mods.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids, ["mod-b", "mod-c", "mod-a"]);
    }

    #[test]
    fn grouping_one_mod_is_refused_without_writing() {
        let data = setup("edits-group-too-few");
        let error = group_mods(&data, test_games::SLOT_ID, &["mod-a".to_string()]).unwrap_err();
        assert_eq!(error.code, "groups.needs_pair");
        assert!(saved(&data).groups.is_empty());
    }

    #[test]
    fn grouping_two_mods_is_saved() {
        let data = setup("edits-group-two");
        group_mods(
            &data,
            test_games::SLOT_ID,
            &["mod-a".to_string(), "mod-b".to_string()],
        )
        .unwrap();
        let groups = saved(&data).groups;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "grp1");
        assert_eq!(groups[0].members, ["mod-a", "mod-b"]);
    }

    #[test]
    fn ungrouping_keeps_the_mods() {
        let data = setup("edits-ungroup");
        let profiles_path = data.game(test_games::SLOT_ID).profiles_file();
        let mut profiles = load_profiles(&data.game(test_games::SLOT_ID)).unwrap();
        profiles.active_mut().groups = vec![group("pair", &["mod-a", "mod-b"])];
        write(&profiles_path, &serde_json::to_string(&profiles).unwrap());

        ungroup_mods(&data, test_games::SLOT_ID, "pair").unwrap();

        assert!(saved(&data).groups.is_empty());
        assert_eq!(saved(&data).mods.len(), 3);
    }

    #[test]
    fn an_enable_toggle_is_saved() {
        let data = setup("edits-enable");
        set_mod_enabled(&data, test_games::SLOT_ID, "mod-b", false).unwrap();
        let list = saved(&data);
        assert!(list.mods[0].enabled);
        assert!(!list.mods[1].enabled);
    }

    #[test]
    fn a_separator_is_saved_above_the_named_mod() {
        let data = setup("edits-separator");
        set_separator(&data, test_games::SLOT_ID, "mod-b", "Header").unwrap();
        let separators = saved(&data).separators;
        assert_eq!(separators.len(), 1);
        assert_eq!(separators[0].before.as_deref(), Some("mod-b"));
    }
}
