//! Saving the active profile's mod groups. Groups are one list edit, so they share the list's single history entry.

use std::collections::BTreeSet;

use crate::context::Data;
use crate::context::Loaded;
use crate::context::load_game;
use crate::context::load_profiles;
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::domain::profiles::ModGroup;
use crate::error::AppError;

/// Saves the active profile's groups as one list change. Members no longer on the list are dropped, and a group left
/// with fewer than two members is removed. Duplicate ids or a family in two groups are refused as invalid input.
pub fn save_mod_groups(root: &Data, game_id: &str, groups: &[ModGroup]) -> Result<(), AppError> {
    let Loaded { data, .. } = load_game(root, game_id)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;

    let mut ids = BTreeSet::new();
    let mut members = BTreeSet::new();
    for group in groups {
        if !ids.insert(group.id.as_str()) {
            return Err(AppError::user("groups.invalid").with("id", &group.id));
        }
        for member in &group.members {
            if !members.insert(member.as_str()) {
                return Err(AppError::user("groups.invalid").with("id", member));
            }
        }
    }

    let listed: BTreeSet<&str> = profiles
        .active()
        .mods
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    let kept: Vec<ModGroup> = groups
        .iter()
        .map(|group| ModGroup {
            members: group
                .members
                .iter()
                .filter(|id| listed.contains(id.as_str()))
                .cloned()
                .collect(),
            ..group.clone()
        })
        .filter(|group| group.members.len() >= 2)
        .collect();

    profiles.active_mut().groups = kept;
    save_profiles(&data, &profiles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::profiles::{Entry, GroupKind, Profile, Profiles};
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
            members: members.iter().map(|member| (*member).to_string()).collect(),
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

    fn saved(data: &Data) -> Profile {
        crate::context::load_profiles(&data.game(test_games::SLOT_ID))
            .unwrap()
            .active()
            .clone()
    }

    #[test]
    fn duplicate_group_ids_are_refused() {
        let data = setup("groups-duplicate-id");
        let result = save_mod_groups(
            &data,
            test_games::SLOT_ID,
            &[
                group("g", &["mod-a", "mod-b"]),
                group("g", &["mod-b", "mod-c"]),
            ],
        );
        assert_eq!(result.unwrap_err().code, "groups.invalid");
    }

    #[test]
    fn a_family_in_two_groups_is_refused() {
        let data = setup("groups-duplicate-member");
        let result = save_mod_groups(
            &data,
            test_games::SLOT_ID,
            &[
                group("g1", &["mod-a", "mod-b"]),
                group("g2", &["mod-b", "mod-c"]),
            ],
        );
        assert_eq!(result.unwrap_err().code, "groups.invalid");
    }

    #[test]
    fn saving_groups_prunes_stale_members_and_drops_small_groups() {
        let data = setup("groups-prune");
        save_mod_groups(
            &data,
            test_games::SLOT_ID,
            &[
                group("g1", &["mod-a", "gone"]),
                group("g2", &["mod-b", "mod-c"]),
            ],
        )
        .unwrap();

        let groups = saved(&data).groups;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "g2");
        assert_eq!(groups[0].members, ["mod-b", "mod-c"]);
    }
}
