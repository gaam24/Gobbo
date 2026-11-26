use super::planning::{selected_keys, start_with_plan};
use super::removal::remove;
use crate::collections::model::{CollectionInstallRequest, CollectionPosition};
use crate::context::profile::history::save_profiles;
use crate::context::{Data, load_profiles};
use crate::domain::collection::{
    CollectionInfo, CollectionItem, CollectionOrigin, CollectionPlan, CollectionSource,
    CollectionUpdatePolicy,
};
use crate::domain::fomod::Choice;
use crate::domain::profiles::ListedMod;
use std::collections::BTreeMap;

fn item(key: &str, optional: bool, phase: u32) -> CollectionItem {
    CollectionItem {
        key: key.to_string(),
        name: key.to_string(),
        version: Some("1".into()),
        author: None,
        category: None,
        summary: String::new(),
        picture_url: None,
        optional,
        phase,
        source: CollectionSource::Manual,
        file_name: None,
        size_kb: 0,
        md5: None,
        update_policy: CollectionUpdatePolicy::Exact,
        choices: Vec::<Choice>::new(),
        patches: BTreeMap::new(),
        instructions: String::new(),
        adult: false,
    }
}

fn plan() -> CollectionPlan {
    CollectionPlan {
        format: crate::domain::collection::PLAN_FORMAT,
        info: CollectionInfo {
            revision_label: String::new(),
            provider: "nexus".into(),
            game_key: "examplegame".into(),
            slug: "example".into(),
            revision: 1,
            latest_revision: 1,
            name: "Example".into(),
            author: None,
            summary: String::new(),
            description: String::new(),
            instructions: String::new(),
            picture_url: None,
            adult: false,
            total_size_kb: 0,
            mod_count: 2,
            game_versions: Vec::new(),
            changelog: None,
            url: String::new(),
        },
        items: vec![item("a", false, 0), item("b", true, 666)],
        rules: Vec::new(),
        issues: Vec::new(),
        order: vec!["a".into(), "b".into()],
        has_bundles: false,
        has_patches: false,
        limited: false,
    }
}

fn origin(slug: &str, added: &[&str], families: &[&str]) -> CollectionOrigin {
    origin_with(slug, added, families, &[])
}

fn origin_with(
    slug: &str,
    added: &[&str],
    families: &[&str],
    disabled: &[&str],
) -> CollectionOrigin {
    CollectionOrigin {
        provider: "nexus".into(),
        game_key: "full-root".into(),
        slug: slug.into(),
        revision: 1,
        latest_revision: 1,
        revision_label: String::new(),
        name: slug.into(),
        author: None,
        url: String::new(),
        excluded: Vec::new(),
        manual: Vec::new(),
        added: added.iter().map(ToString::to_string).collect(),
        families: families.iter().map(ToString::to_string).collect(),
        disabled: disabled.iter().map(ToString::to_string).collect(),
        installed_at: 0,
    }
}

fn entry(id: &str) -> ListedMod {
    ListedMod {
        id: id.into(),
        folder: format!("{id}-1"),
        enabled: true,
        priority: None,
    }
}

fn waiting(family: &str) -> crate::domain::pack::WaitingMod {
    crate::domain::pack::WaitingMod {
        before: None,
        entry: item(family, false, 0).wanted(),
    }
}

#[test]
fn an_empty_selection_takes_the_required_members() {
    let plan = plan();
    assert_eq!(
        selected_keys(&plan, &[]).into_iter().collect::<Vec<_>>(),
        ["a"]
    );
}

#[test]
fn a_new_profile_takes_the_selected_members_and_skips_the_optional_one() {
    let _game = crate::test_support::games::full_root();
    let root = Data::new(crate::test_support::scratch!("collection-session-start"));
    let mut plan = plan();
    plan.info.game_key = "full-root".into();
    plan.info.provider = "nexus".into();
    plan.info.url = "https://www.nexusmods.com/games/full-root/collections/example".into();
    plan.items = vec![
        CollectionItem {
            key: "nexus:7".into(),
            name: "Required".into(),
            version: Some("1".into()),
            author: None,
            category: None,
            summary: String::new(),
            picture_url: None,
            optional: false,
            phase: 0,
            source: CollectionSource::Provider {
                provider: "nexus".into(),
                mod_id: "7".into(),
                file_id: "10".into(),
                part_id: None,
            },
            file_name: None,
            size_kb: 0,
            md5: None,
            update_policy: CollectionUpdatePolicy::Exact,
            choices: Vec::new(),
            patches: BTreeMap::new(),
            instructions: String::new(),
            adult: false,
        },
        item("optional", true, 666),
    ];
    plan.order = vec!["nexus:7".into(), "optional".into()];
    let data = root.game("full-root");
    crate::collections::cache::write_plan(&data, &plan).unwrap();
    let request = CollectionInstallRequest {
        link: plan.info.url.clone(),
        revision: Some(1),
        profile: None,
        profile_name: "Collection".into(),
        selected: vec!["nexus:7".into()],
        position: CollectionPosition::Bottom,
        activate: true,
    };
    let view = start_with_plan(&root, "full-root", &request, plan).unwrap();
    assert_eq!(view.origin.excluded, ["optional"]);
    let profiles = load_profiles(&data).unwrap();
    let profile = profiles.profile(&view.profile).unwrap();
    assert_eq!(profile.collections.len(), 1);
    assert_eq!(profile.waiting.len(), 1);
    assert_eq!(profile.waiting[0].entry.name, "Required");
}

#[test]
fn removing_a_collection_keeps_mods_it_did_not_add_and_mods_another_collection_lists() {
    let _game = crate::test_support::games::full_root();
    let root = Data::new(crate::test_support::scratch!("collection-session-remove"));
    let mut plan = plan();
    plan.info.game_key = "full-root".into();
    let data = root.game("full-root");
    crate::collections::cache::write_plan(&data, &plan).unwrap();
    {
        let mut profiles = load_profiles(&data).unwrap();
        let active = profiles.active.clone();
        let profile = profiles.profile_mut(&active).unwrap();
        profile.mods = vec![entry("own"), entry("added"), entry("shared")];
        profile.collections = vec![
            origin("example", &["added", "shared"], &["own", "added", "shared"]),
            origin("other", &[], &["shared"]),
        ];
        save_profiles(&data, &profiles).unwrap();
    }

    let active = load_profiles(&data).unwrap().active;
    remove(&root, "full-root", &active, "nexus:full-root:example").unwrap();
    let profiles = load_profiles(&data).unwrap();
    let profile = profiles.profile(&active).unwrap();
    let ids: Vec<&str> = profile.mods.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(ids, ["own", "shared"]);
    assert_eq!(profile.collections.len(), 1);
}

#[test]
fn removing_a_collection_keeps_a_waiting_mod_that_only_shares_a_name() {
    fn provider_item(key: &str, name: &str, mod_id: &str, file_id: &str) -> CollectionItem {
        CollectionItem {
            key: key.into(),
            name: name.into(),
            source: CollectionSource::Provider {
                provider: "nexus".into(),
                mod_id: mod_id.into(),
                file_id: file_id.into(),
                part_id: None,
            },
            ..item(key, false, 0)
        }
    }

    let _game = crate::test_support::games::full_root();
    let root = Data::new(crate::test_support::scratch!(
        "collection-session-remove-waiting"
    ));
    let data = root.game("full-root");
    let member = provider_item("nexus:7", "Shared Name", "7", "10");
    let namesake = provider_item("nexus:9", "Shared Name", "9", "12");
    let mut plan = plan();
    plan.info.game_key = "full-root".into();
    plan.items = vec![member.clone()];
    plan.order = vec![member.key.clone()];
    crate::collections::cache::write_plan(&data, &plan).unwrap();
    // Neither family was added to the profile, so the member filter decides both waiting entries.
    let profile_id = seed_profile(
        &data,
        vec![],
        vec![origin("example", &[], &[])],
        vec![
            crate::domain::pack::WaitingMod {
                before: None,
                entry: member.wanted(),
            },
            crate::domain::pack::WaitingMod {
                before: None,
                entry: namesake.wanted(),
            },
        ],
    );
    remove(&root, "full-root", &profile_id, "nexus:full-root:example").unwrap();
    let profiles = load_profiles(&data).unwrap();
    let profile = profiles.profile(&profile_id).unwrap();
    assert_eq!(profile.waiting.len(), 1);
    assert_eq!(profile.waiting[0].entry.family, "nexus:9");
    assert_eq!(profile.waiting[0].entry.name, "Shared Name");
}

#[test]
fn unknown_selected_keys_are_ignored() {
    let plan = plan();
    let selected = selected_keys(&plan, &["a".into(), "missing".into()]);
    assert!(selected.contains("a"));
    assert!(!selected.contains("missing"));
}

fn update_plan(members: &[&str]) -> CollectionPlan {
    let mut plan = plan();
    plan.info.game_key = "full-root".into();
    plan.info.url = "https://www.nexusmods.com/games/full-root/collections/example".into();
    plan.items = members.iter().map(|key| item(key, false, 0)).collect();
    plan.order = members.iter().map(ToString::to_string).collect();
    plan.info.mod_count = u32::try_from(members.len()).unwrap_or(u32::MAX);
    plan
}

fn update_request(
    plan: &CollectionPlan,
    profile: &str,
    selected: &[&str],
) -> CollectionInstallRequest {
    CollectionInstallRequest {
        link: plan.info.url.clone(),
        revision: Some(plan.info.revision),
        profile: Some(profile.to_string()),
        profile_name: String::new(),
        selected: selected.iter().map(ToString::to_string).collect(),
        position: CollectionPosition::Bottom,
        activate: false,
    }
}

fn seed_profile(
    data: &Data,
    mods: Vec<ListedMod>,
    collections: Vec<CollectionOrigin>,
    waiting: Vec<crate::domain::pack::WaitingMod>,
) -> String {
    let mut profiles = load_profiles(data).unwrap();
    let id = profiles.active.clone();
    let profile = profiles.profile_mut(&id).unwrap();
    profile.mods = mods;
    profile.collections = collections;
    profile.waiting = waiting;
    save_profiles(data, &profiles).unwrap();
    id
}

#[test]
fn an_update_switches_off_a_member_the_new_revision_removed() {
    let _game = crate::test_support::games::full_root();
    let root = Data::new(crate::test_support::scratch!(
        "collection-session-update-removed"
    ));
    let data = root.game("full-root");
    let plan = update_plan(&["kept", "fresh"]);
    crate::collections::cache::write_plan(&data, &plan).unwrap();
    let profile_id = seed_profile(
        &data,
        vec![entry("own"), entry("removed"), entry("kept")],
        vec![origin(
            "example",
            &["removed", "kept"],
            &["own", "removed", "kept"],
        )],
        vec![waiting("removed"), waiting("kept")],
    );
    let request = update_request(&plan, &profile_id, &["kept", "fresh"]);
    let view = start_with_plan(&root, "full-root", &request, plan).unwrap();
    assert_eq!(view.origin.disabled, ["removed"]);
    assert!(!view.origin.families.contains(&"removed".to_string()));
    let profiles = load_profiles(&data).unwrap();
    let profile = profiles.profile(&profile_id).unwrap();
    let enabled = |id: &str| {
        profile
            .mods
            .iter()
            .find(|entry| entry.id == id)
            .unwrap()
            .enabled
    };
    assert!(!enabled("removed"));
    assert!(enabled("kept"));
    assert!(enabled("own"));
    assert!(
        profile
            .waiting
            .iter()
            .all(|waiting| waiting.entry.family != "removed")
    );
}

#[test]
fn a_member_the_new_revision_brings_back_is_enabled_again() {
    let _game = crate::test_support::games::full_root();
    let root = Data::new(crate::test_support::scratch!(
        "collection-session-update-return"
    ));
    let data = root.game("full-root");
    let plan = update_plan(&["back"]);
    crate::collections::cache::write_plan(&data, &plan).unwrap();
    let mut back = entry("back");
    back.enabled = false;
    let profile_id = seed_profile(
        &data,
        vec![back],
        vec![origin_with("example", &["back"], &["back"], &["back"])],
        vec![],
    );
    let request = update_request(&plan, &profile_id, &["back"]);
    let view = start_with_plan(&root, "full-root", &request, plan).unwrap();
    assert!(view.origin.disabled.is_empty());
    let profiles = load_profiles(&data).unwrap();
    let profile = profiles.profile(&profile_id).unwrap();
    assert!(
        profile
            .mods
            .iter()
            .find(|entry| entry.id == "back")
            .unwrap()
            .enabled
    );
}

#[test]
fn a_mod_the_user_disabled_while_still_a_member_is_left_alone() {
    let _game = crate::test_support::games::full_root();
    let root = Data::new(crate::test_support::scratch!(
        "collection-session-update-kept-off"
    ));
    let data = root.game("full-root");
    let plan = update_plan(&["kept"]);
    crate::collections::cache::write_plan(&data, &plan).unwrap();
    let mut kept = entry("kept");
    kept.enabled = false;
    let profile_id = seed_profile(
        &data,
        vec![kept],
        vec![origin("example", &["kept"], &["kept"])],
        vec![],
    );
    let request = update_request(&plan, &profile_id, &["kept"]);
    let view = start_with_plan(&root, "full-root", &request, plan).unwrap();
    assert!(view.origin.disabled.is_empty());
    let profiles = load_profiles(&data).unwrap();
    let profile = profiles.profile(&profile_id).unwrap();
    assert!(
        !profile
            .mods
            .iter()
            .find(|entry| entry.id == "kept")
            .unwrap()
            .enabled
    );
}

#[test]
fn a_mod_shared_with_another_collection_is_not_switched_off() {
    let _game = crate::test_support::games::full_root();
    let root = Data::new(crate::test_support::scratch!(
        "collection-session-update-shared"
    ));
    let data = root.game("full-root");
    let plan = update_plan(&["fresh"]);
    crate::collections::cache::write_plan(&data, &plan).unwrap();
    let profile_id = seed_profile(
        &data,
        vec![entry("shared")],
        vec![
            origin("example", &["shared"], &["shared"]),
            origin("other", &[], &["shared"]),
        ],
        vec![],
    );
    let request = update_request(&plan, &profile_id, &["fresh"]);
    let view = start_with_plan(&root, "full-root", &request, plan).unwrap();
    assert!(view.origin.disabled.is_empty());
    let profiles = load_profiles(&data).unwrap();
    let profile = profiles.profile(&profile_id).unwrap();
    assert!(
        profile
            .mods
            .iter()
            .find(|entry| entry.id == "shared")
            .unwrap()
            .enabled
    );
}

#[test]
fn a_limited_plan_does_not_switch_anything_off() {
    let _game = crate::test_support::games::full_root();
    let root = Data::new(crate::test_support::scratch!(
        "collection-session-update-limited"
    ));
    let data = root.game("full-root");
    let mut plan = update_plan(&["fresh"]);
    plan.limited = true;
    crate::collections::cache::write_plan(&data, &plan).unwrap();
    let profile_id = seed_profile(
        &data,
        vec![entry("removed")],
        vec![origin("example", &["removed"], &["removed"])],
        vec![],
    );
    let request = update_request(&plan, &profile_id, &["fresh"]);
    let view = start_with_plan(&root, "full-root", &request, plan).unwrap();
    assert!(view.origin.disabled.is_empty());
    let profiles = load_profiles(&data).unwrap();
    let profile = profiles.profile(&profile_id).unwrap();
    assert!(
        profile
            .mods
            .iter()
            .find(|entry| entry.id == "removed")
            .unwrap()
            .enabled
    );
}

#[test]
fn a_new_file_of_the_same_mod_is_not_treated_as_removed() {
    let _game = crate::test_support::games::full_root();
    let root = Data::new(crate::test_support::scratch!(
        "collection-session-update-file"
    ));
    let data = root.game("full-root");
    let mut plan = update_plan(&[]);
    plan.items = vec![CollectionItem {
        key: "nexus:7".into(),
        name: "Pinned".into(),
        version: Some("2".into()),
        author: None,
        category: None,
        summary: String::new(),
        picture_url: None,
        optional: false,
        phase: 0,
        source: CollectionSource::Provider {
            provider: "nexus".into(),
            mod_id: "7".into(),
            file_id: "11".into(),
            part_id: None,
        },
        file_name: None,
        size_kb: 0,
        md5: None,
        update_policy: CollectionUpdatePolicy::Exact,
        choices: Vec::new(),
        patches: BTreeMap::new(),
        instructions: String::new(),
        adult: false,
    }];
    plan.order = vec!["nexus:7".into()];
    crate::collections::cache::write_plan(&data, &plan).unwrap();
    let profile_id = seed_profile(
        &data,
        vec![entry("nexus:7")],
        vec![origin("example", &["nexus:7"], &["nexus:7"])],
        vec![],
    );
    let request = update_request(&plan, &profile_id, &["nexus:7"]);
    let view = start_with_plan(&root, "full-root", &request, plan).unwrap();
    assert!(view.origin.disabled.is_empty());
    let profiles = load_profiles(&data).unwrap();
    let profile = profiles.profile(&profile_id).unwrap();
    assert!(
        profile
            .mods
            .iter()
            .find(|entry| entry.id == "nexus:7")
            .unwrap()
            .enabled
    );
}
