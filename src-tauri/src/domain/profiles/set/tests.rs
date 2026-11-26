use super::super::group::{GroupKind, ModGroup};
use super::super::profile::Separator;

use super::*;

fn entry(id: &str, enabled: bool) -> ListedMod {
    ListedMod {
        id: id.to_string(),
        folder: id.to_string(),
        enabled,
        priority: None,
    }
}

fn profile(ids: &[&str]) -> Profile {
    Profile::new(
        "main".to_string(),
        "Main".to_string(),
        ids.iter().map(|id| entry(id, true)).collect(),
    )
}

fn pages(pairs: &[(&str, &str)]) -> PageKeys {
    pairs
        .iter()
        .map(|(id, key)| (id.to_string(), key.to_string()))
        .collect()
}

fn ids(list: &Profile) -> Vec<&str> {
    list.mods.iter().map(|item| item.id.as_str()).collect()
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

#[test]
fn a_page_run_moves_as_one_block_above_the_target() {
    let mut list = profile(&["alpha", "beta", "gamma", "delta"]);
    let keys = pages(&[
        ("alpha", "provider:one"),
        ("beta", "provider:one"),
        ("gamma", "provider:two"),
        ("delta", "provider:three"),
    ]);

    move_block(&mut list, &keys, "beta", "delta", MoveSide::Above).unwrap();

    assert_eq!(ids(&list), ["gamma", "alpha", "beta", "delta"]);
}

#[test]
fn a_stored_group_moves_as_one_block_below_the_target() {
    let mut list = profile(&["alpha", "beta", "gamma", "delta"]);
    list.groups = vec![group("first", &["alpha", "gamma"])];

    move_block(
        &mut list,
        &PageKeys::new(),
        "alpha",
        "delta",
        MoveSide::Below,
    )
    .unwrap();

    assert_eq!(ids(&list), ["beta", "delta", "alpha", "gamma"]);
    assert_eq!(list.groups[0].members, ["alpha", "gamma"]);
}

#[test]
fn a_single_mod_moves_to_either_side_of_its_target() {
    let mut list = profile(&["alpha", "beta", "gamma"]);

    move_block(
        &mut list,
        &PageKeys::new(),
        "alpha",
        "gamma",
        MoveSide::Below,
    )
    .unwrap();
    assert_eq!(ids(&list), ["beta", "gamma", "alpha"]);

    move_block(
        &mut list,
        &PageKeys::new(),
        "alpha",
        "beta",
        MoveSide::Above,
    )
    .unwrap();
    assert_eq!(ids(&list), ["alpha", "beta", "gamma"]);
}

#[test]
fn a_drop_on_its_own_block_changes_nothing() {
    let mut list = profile(&["alpha", "beta"]);
    let keys = pages(&[("alpha", "provider:one"), ("beta", "provider:one")]);

    move_block(&mut list, &keys, "alpha", "beta", MoveSide::Below).unwrap();

    assert_eq!(ids(&list), ["alpha", "beta"]);
}

#[test]
fn a_missing_id_is_refused() {
    let mut list = profile(&["alpha", "beta"]);
    let error = move_block(
        &mut list,
        &PageKeys::new(),
        "gone",
        "alpha",
        MoveSide::Above,
    )
    .unwrap_err();
    assert_eq!(error.code, "library.mod_missing");
    assert_eq!(error.params.get("folder").map(String::as_str), Some("gone"));
}

#[test]
fn an_enable_toggle_acts_on_the_whole_page_run() {
    let mut list = profile(&["alpha", "beta", "gamma"]);
    let keys = pages(&[("alpha", "provider:one"), ("beta", "provider:one")]);

    set_block_enabled(&mut list, &keys, "beta", false).unwrap();

    assert!(!list.mods[0].enabled);
    assert!(!list.mods[1].enabled);
    assert!(list.mods[2].enabled);
}

#[test]
fn an_enable_toggle_acts_on_the_whole_stored_group() {
    let mut list = profile(&["alpha", "beta", "gamma"]);
    list.groups = vec![group("pair", &["alpha", "gamma"])];

    set_block_enabled(&mut list, &PageKeys::new(), "alpha", false).unwrap();

    assert!(!list.mods[0].enabled);
    assert!(list.mods[1].enabled);
    assert!(!list.mods[2].enabled);
}

#[test]
fn a_group_needs_two_distinct_listed_members() {
    let mut list = profile(&["alpha", "beta"]);
    let one = vec!["alpha".to_string()];
    assert_eq!(
        add_group(&mut list, "grp1", "Pair", &one).unwrap_err().code,
        "groups.needs_pair"
    );
    let missing = vec!["alpha".to_string(), "gone".to_string()];
    assert_eq!(
        add_group(&mut list, "grp1", "Pair", &missing)
            .unwrap_err()
            .code,
        "library.mod_missing"
    );
    assert!(list.groups.is_empty());
}

#[test]
fn adding_a_group_moves_members_out_of_their_old_one() {
    let mut list = profile(&["alpha", "beta", "gamma", "delta"]);
    list.groups = vec![
        group("older1", &["alpha", "beta"]),
        group("older2", &["gamma", "delta"]),
    ];

    add_group(
        &mut list,
        "grp1",
        "Pair",
        &["beta".to_string(), "gamma".to_string()],
    )
    .unwrap();

    assert_eq!(list.groups.len(), 1);
    assert_eq!(list.groups[0].id, "grp1");
    assert_eq!(list.groups[0].members, ["beta", "gamma"]);
}

#[test]
fn group_ids_and_separator_ids_skip_the_ones_already_used() {
    let mut list = profile(&["alpha"]);
    list.groups = vec![group("grp1", &["alpha", "alpha"])];
    list.separators.push(Separator {
        id: "sep1".to_string(),
        name: "Head".to_string(),
        before: None,
    });

    assert_eq!(next_group_id(&list), "grp2");
    assert_eq!(next_separator_id(&list), "sep2");
}

#[test]
fn setting_a_group_moves_one_mod_and_drops_a_small_group() {
    let mut list = profile(&["alpha", "beta", "gamma", "delta"]);
    list.groups = vec![
        group("first", &["alpha", "beta"]),
        group("second", &["gamma", "delta"]),
    ];

    set_mod_group(&mut list, "beta", Some("second")).unwrap();

    assert_eq!(list.groups.len(), 1);
    assert_eq!(list.groups[0].id, "second");
    assert_eq!(list.groups[0].members, ["gamma", "delta", "beta"]);
}

#[test]
fn clearing_a_group_of_one_of_two_members_removes_the_group() {
    let mut list = profile(&["alpha", "beta"]);
    list.groups = vec![group("pair", &["alpha", "beta"])];

    set_mod_group(&mut list, "alpha", None).unwrap();

    assert!(list.groups.is_empty());
}

#[test]
fn an_unknown_group_is_refused() {
    let mut list = profile(&["alpha", "beta"]);
    assert_eq!(
        set_mod_group(&mut list, "alpha", Some("gone"))
            .unwrap_err()
            .code,
        "groups.invalid"
    );
    assert_eq!(
        remove_group(&mut list, "gone").unwrap_err().code,
        "groups.invalid"
    );
}

#[test]
fn removing_a_group_keeps_its_mods() {
    let mut list = profile(&["alpha", "beta"]);
    list.groups = vec![group("pair", &["alpha", "beta"])];

    remove_group(&mut list, "pair").unwrap();

    assert!(list.groups.is_empty());
    assert_eq!(ids(&list), ["alpha", "beta"]);
}

#[test]
fn a_separator_can_be_renamed() {
    let mut list = profile(&["alpha"]);
    list.separators.push(Separator {
        id: "sep1".to_string(),
        name: "Old".to_string(),
        before: Some("alpha".to_string()),
    });

    set_separator(&mut list, &PageKeys::new(), "sep1", "  New  ").unwrap();

    assert_eq!(list.separators[0].name, "New");
    assert_eq!(list.separators[0].before.as_deref(), Some("alpha"));
}

#[test]
fn a_new_separator_anchors_to_the_first_member_of_a_page_block() {
    let mut list = profile(&["alpha", "beta", "gamma"]);
    let keys = pages(&[("alpha", "provider:one"), ("beta", "provider:one")]);

    set_separator(&mut list, &keys, "beta", "Header").unwrap();

    assert_eq!(list.separators.len(), 1);
    assert_eq!(list.separators[0].id, "sep1");
    assert_eq!(list.separators[0].before.as_deref(), Some("alpha"));
}

#[test]
fn a_separator_needs_a_short_non_empty_name() {
    let mut list = profile(&["alpha"]);
    assert_eq!(
        set_separator(&mut list, &PageKeys::new(), "alpha", "  ")
            .unwrap_err()
            .code,
        "profile.separator_name"
    );
}

#[test]
fn normalizing_prunes_stale_and_duplicate_group_members() {
    let mut list = profile(&["alpha", "beta", "gamma"]);
    list.groups = vec![
        group("stale", &["alpha", "gone"]),
        group("kept", &["beta", "gamma"]),
        group("duplicate", &["gamma", "gamma", "alpha"]),
    ];

    normalize_groups(&mut list);

    assert_eq!(list.groups.len(), 1);
    assert_eq!(list.groups[0].id, "kept");
    assert_eq!(list.groups[0].members, ["beta", "gamma"]);
}
