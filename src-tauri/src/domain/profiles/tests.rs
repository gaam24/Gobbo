//! Tests of the profile list: the mods of a profile, the copies, the names and the file it is saved in.

use super::*;
use crate::test_support::scratch;
use std::fs;

fn entry(id: &str, folder: &str, enabled: bool) -> ListedMod {
    ListedMod {
        id: id.to_string(),
        folder: folder.to_string(),
        enabled,
        priority: None,
    }
}

fn two_profiles() -> Profiles {
    let mut profiles = Profiles::with_default(vec![entry("local:moda", "modA", true)]);
    profiles.create("Hardcore", None).unwrap();
    profiles
}

#[test]
fn a_mod_is_listed_by_every_profile_that_has_it() {
    let mut profiles = two_profiles();
    let hardcore = profiles.profiles[1].id.clone();
    profiles
        .profile_mut(&hardcore)
        .unwrap()
        .mods
        .push(entry("local:moda", "modA", true));
    let view = ProfilesView::new(&profiles);
    let names: Vec<&str> = view
        .with_mod("local:moda")
        .iter()
        .map(|profile| profile.name.as_str())
        .collect();
    assert_eq!(names, [DEFAULT_NAME, "Hardcore"]);
    assert!(view.with_mod("local:modb").is_empty());
}

#[test]
fn new_profile_starts_empty_and_the_active_one_stays() {
    let mut profiles = two_profiles();
    assert_eq!(profiles.active().name, DEFAULT_NAME);
    let id = profiles.create("Empty", None).unwrap();
    assert!(profiles.profile(&id).unwrap().mods.is_empty());
    assert_eq!(profiles.active().name, DEFAULT_NAME);
}

#[test]
fn copy_takes_the_list_and_flags_of_the_source() {
    let mut profiles = two_profiles();
    let source = profiles.profiles[0].id.clone();
    let copy = profiles.create("Copy", Some(&source)).unwrap();
    assert_eq!(
        profiles.profile(&copy).unwrap().mods,
        vec![entry("local:moda", "modA", true)]
    );
}

#[test]
fn names_are_trimmed_and_unique_ignoring_case() {
    let mut profiles = two_profiles();
    assert!(profiles.create("   ", None).is_err());
    assert!(profiles.create("hardcore", None).is_err());
    assert!(
        profiles
            .create(&"x".repeat(MAX_NAME_CHARS + 1), None)
            .is_err()
    );
    let id = profiles.create("  New  ", None).unwrap();
    assert_eq!(profiles.profile(&id).unwrap().name, "New");
}

#[test]
fn renaming_keeps_its_own_name_and_refuses_another_taken_one() {
    let mut profiles = two_profiles();
    let default = profiles.profiles[0].id.clone();
    profiles.rename(&default, DEFAULT_NAME).unwrap();
    assert!(profiles.rename(&default, "Hardcore").is_err());
    profiles.rename(&default, "Light").unwrap();
    assert_eq!(profiles.active().name, "Light");
}

#[test]
fn the_last_profile_cannot_go_and_deleting_the_active_one_switches_to_the_first() {
    let mut profiles = two_profiles();
    let first = profiles.profiles[0].id.clone();
    let second = profiles.profiles[1].id.clone();
    profiles.switch(&second).unwrap();

    profiles.delete(&second).unwrap();
    assert_eq!(profiles.active, first);
    assert!(profiles.delete(&first).is_err());
}

#[test]
fn switching_to_an_unknown_profile_is_refused() {
    let mut profiles = two_profiles();
    assert!(profiles.switch("nope").is_err());
}

#[test]
fn an_unknown_active_id_falls_back_to_the_first_profile_on_read() {
    let dir = scratch!("profiles-check");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(PROFILES_FILE);
    let mut profiles = two_profiles();
    profiles.active = "gone".to_string();
    profiles.save(&path).unwrap();

    let read = Profiles::read(&path).unwrap().unwrap();
    assert_eq!(read.active, "p1");
}

#[test]
fn profiles_round_trip_through_the_file() {
    let dir = scratch!("profiles");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(PROFILES_FILE);
    let profiles = two_profiles();
    profiles.save(&path).unwrap();
    assert_eq!(Profiles::read(&path).unwrap(), Some(profiles));
    assert_eq!(Profiles::read(&dir.join("missing.json")).unwrap(), None);
}

#[test]
fn a_separator_above_a_removed_mod_moves_to_the_next_one() {
    let mut profile = Profile::new(
        "p1".into(),
        "P".into(),
        vec![
            entry("a", "a", true),
            entry("b", "b", true),
            entry("c", "c", true),
        ],
    );
    profile.separators = vec![
        Separator {
            id: "s1".into(),
            name: "One".into(),
            before: Some("b".into()),
        },
        Separator {
            id: "s2".into(),
            name: "End".into(),
            before: Some("c".into()),
        },
    ];
    profile.remove_mods(|entry| entry.id == "b");
    assert_eq!(profile.separators[0].before.as_deref(), Some("c"));
    profile.remove_mods(|entry| entry.id == "c");
    assert_eq!(profile.separators[0].before, None);
    assert_eq!(profile.separators[1].before, None);
    assert_eq!(profile.mods, [entry("a", "a", true)]);
}

#[test]
fn a_copied_profile_takes_the_separators_too() {
    let mut profiles = two_profiles();
    profiles.profiles[0].separators = vec![Separator {
        id: "s1".into(),
        name: "Graphics".into(),
        before: Some("local:moda".into()),
    }];
    let copy = profiles.create("Copy", Some("p1")).unwrap();
    assert_eq!(profiles.profile(&copy).unwrap().separators.len(), 1);
}

#[test]
fn version_helpers_follow_every_profile() {
    let mut profiles = two_profiles();
    let first = profiles.profiles[0].id.clone();
    assert_eq!(ProfilesView::new(&profiles).using_version("modA").len(), 1);
    profiles.replace_version("modA", "modA-2");
    assert_eq!(profiles.profile(&first).unwrap().mods[0].folder, "modA-2");
}

#[test]
fn removing_a_mod_takes_it_from_every_list_and_keeps_the_others() {
    let mut profiles = two_profiles();
    let first = profiles.profiles[0].id.clone();
    let hardcore = profiles.profiles[1].id.clone();
    profiles
        .profile_mut(&first)
        .unwrap()
        .mods
        .push(entry("local:modb", "modB", true));
    profiles.profile_mut(&first).unwrap().deployed_mods = vec![
        entry("local:moda", "modA", true),
        entry("local:modb", "modB", true),
    ];
    profiles
        .profile_mut(&hardcore)
        .unwrap()
        .mods
        .push(entry("local:moda", "modA", true));

    profiles.remove_mod("local:moda");

    assert_eq!(
        profiles.profile(&first).unwrap().mods,
        vec![entry("local:modb", "modB", true)]
    );
    assert_eq!(
        profiles.profile(&first).unwrap().deployed_mods,
        vec![entry("local:modb", "modB", true)]
    );
    assert!(profiles.profile(&hardcore).unwrap().mods.is_empty());
}

#[test]
fn a_name_of_exactly_the_longest_length_is_kept() {
    let mut profiles = two_profiles();
    assert!(profiles.create(&"x".repeat(MAX_NAME_CHARS), None).is_ok());
}

#[test]
fn a_new_profile_skips_ids_that_are_taken() {
    let mut profiles = Profiles::with_default(Vec::new());
    // The count gives p2 first, so the next free id is p4 once p3 is taken too.
    profiles.profiles.push(Profile::new(
        "p3".to_string(),
        "Third".to_string(),
        Vec::new(),
    ));
    assert_eq!(profiles.create("New", None).unwrap(), "p4");
}

#[test]
fn a_damaged_profiles_file_is_kept_broken_and_starts_from_the_default() {
    let dir = scratch!("profiles-broken");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(PROFILES_FILE);
    fs::write(&path, "{ not json").unwrap();

    let profiles = Profiles::read_or_default(&path).unwrap();
    assert_eq!(profiles.profiles.len(), 1);
    assert_eq!(profiles.active, "p1");
    assert!(dir.join("profiles.json.broken").is_file());
}

#[test]
fn profiles_from_a_newer_gobbo_are_refused() {
    let dir = scratch!("profiles-newer");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(PROFILES_FILE);
    let mut value = serde_json::to_value(two_profiles()).unwrap();
    value["schema_version"] = serde_json::Value::from(999);
    fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();

    let error = Profiles::read(&path).unwrap_err();
    assert_eq!(error.code, "state.schema_newer");
}

fn group(id: &str, members: &[&str], kind: GroupKind) -> ModGroup {
    ModGroup {
        id: id.to_string(),
        name: id.to_string(),
        members: members.iter().map(|member| (*member).to_string()).collect(),
        kind,
        language: None,
    }
}

#[test]
fn a_group_kind_serializes_as_its_snake_case_variant() {
    assert_eq!(
        serde_json::to_string(&GroupKind::Bundle).unwrap(),
        "\"bundle\""
    );
    assert_eq!(
        serde_json::to_string(&GroupKind::Translation).unwrap(),
        "\"translation\""
    );
}

#[test]
fn a_profile_without_groups_loads_with_an_empty_list() {
    let dir = scratch!("profiles-no-groups");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(PROFILES_FILE);
    // An old file, written before groups existed.
    fs::write(
        &path,
        r#"{"active":"p1","profiles":[{"id":"p1","name":"Default","mods":[]}]}"#,
    )
    .unwrap();

    let profiles = Profiles::read(&path).unwrap().unwrap();
    assert!(profiles.active().groups.is_empty());
}

#[test]
fn removing_a_mod_prunes_it_from_groups_and_drops_small_groups() {
    let mut profile = Profile::new(
        "p1".into(),
        "P".into(),
        vec![
            entry("a", "a", true),
            entry("b", "b", true),
            entry("c", "c", true),
        ],
    );
    profile.groups = vec![
        group("g1", &["a", "b"], GroupKind::Bundle),
        group("g2", &["a", "c"], GroupKind::Translation),
        group("g3", &["b", "c"], GroupKind::Bundle),
    ];

    profile.remove_mods(|entry| entry.id == "a");

    assert_eq!(profile.groups.len(), 1);
    assert_eq!(profile.groups[0].id, "g3");
    assert_eq!(profile.groups[0].members, ["b", "c"]);
}

#[test]
fn a_list_state_round_trip_keeps_the_groups() {
    let mut profile = Profile::new(
        "p1".into(),
        "P".into(),
        vec![entry("a", "a", true), entry("b", "b", true)],
    );
    let groups = vec![group("g1", &["a", "b"], GroupKind::Translation)];
    profile.groups = groups.clone();

    let state = profile.list_state();
    assert_eq!(state.groups, groups);
    profile.groups.clear();
    profile.restore_list(state);
    assert_eq!(profile.groups, groups);
}

#[test]
fn a_unique_name_follows_the_same_case_rule_as_the_name_check() {
    let mut profiles = Profiles::with_default(Vec::new());
    profiles.create("Ćwiczenie", None).unwrap();
    // The taken name differs only in a non-ASCII capital: the free name must skip it, not hand `create` a name it
    // then refuses as taken.
    let id = profiles.create_unique("ćwiczenie").unwrap();
    assert_eq!(profiles.profile(&id).unwrap().name, "ćwiczenie (2)");
}
