//! Relations and requirements of a mod: the warnings, the Nexus requirements that are missing, and the count of unmet ones.

use super::*;
use crate::domain::source::SourceRequirement;
use crate::library::RelationKind;
use crate::library::Relations;
use crate::library::relations::RequirementState;
use crate::library::{
    add_relation, add_requirement, remove_relation, remove_requirement, set_mod_relations,
    set_requirement_ignored,
};
use crate::profiles::actions::save_profile;
use crate::profiles::order::sort_by_rules;
use crate::status::sync_profiles as status;
use crate::test_support::row;

/// The id of the mod in the folder, as the library gives it.
fn folder_id(f: &Fixture, folder: &str) -> String {
    status(&f.data, SLOT_GAME)
        .unwrap()
        .mods
        .iter()
        .find(|m| m.folder == folder)
        .unwrap()
        .id
        .clone()
}

/// The warning codes the status of the game shows.
fn warned(f: &Fixture) -> Vec<String> {
    status(&f.data, SLOT_GAME)
        .unwrap()
        .warnings
        .iter()
        .map(|w| w.code.clone())
        .collect()
}

/// Two options of one group, so either meets the requirement, and one requirement of its own.
fn option_and_standalone() -> Vec<NexusRequirement> {
    vec![
        NexusRequirement {
            mod_id: 901,
            name: "Option one".into(),
            from_nexus: true,
            group: Some(0),
            ignored: false,
        },
        NexusRequirement {
            mod_id: 902,
            name: "Option two".into(),
            from_nexus: true,
            group: Some(0),
            ignored: false,
        },
        NexusRequirement {
            mod_id: 903,
            name: "Standalone".into(),
            from_nexus: true,
            group: None,
            ignored: false,
        },
    ]
}

/// The two Nexus mods that Nexus lists for a mod's newest file. Neither of them is a mod of the library.
fn listed_requirements() -> Vec<NexusRequirement> {
    vec![
        NexusRequirement {
            mod_id: 778,
            name: "Listed".into(),
            from_nexus: true,
            group: None,
            ignored: false,
        },
        NexusRequirement {
            mod_id: 779,
            name: "Also listed".into(),
            from_nexus: true,
            group: None,
            ignored: false,
        },
    ]
}

/// Writes the Nexus requirements into a family, leaving the requirements of other providers alone.
fn set_nexus_requirements(family: &mut Family, requirements: Vec<NexusRequirement>) {
    let nexus = crate::domain::source::ProviderId::new("nexus");
    family
        .requirements
        .retain(|requirement| requirement.source.provider != nexus);
    family.requirements.extend(
        requirements
            .into_iter()
            .map(crate::domain::source::SourceRequirement::from),
    );
}

/// Edits a mod's family in the library and saves the index, as the page of the mod does.
fn edit_family_on_disk(f: &Fixture, id: &str, edit: impl FnOnce(&mut Family)) {
    let game = test_games::slot();
    let data = f.data.game(SLOT_GAME);
    let mut library = load_library(&data, &game).unwrap();
    edit(library.families.get_mut(id).unwrap());
    library.save(&data.index()).unwrap();
}

#[test]
fn relations_warn_about_missing_disabled_and_conflicting_mods() {
    let f = fixture("relations");
    f.import_two_mods();
    let (a, b) = (folder_id(&f, "modA"), folder_id(&f, "modB"));
    let relations =
        |requires: &[&String], incompatible: &[&String], overrides: &[&String]| Relations {
            requires: requires.iter().map(|id| (*id).clone()).collect(),
            requirements: Vec::new(),
            incompatible: incompatible.iter().map(|id| (*id).clone()).collect(),
            overrides: overrides.iter().map(|id| (*id).clone()).collect(),
        };
    set_mod_relations(&f.data, SLOT_GAME, &b, &relations(&[&a], &[], &[])).unwrap();
    save_profile(&f.data, SLOT_GAME, &[row("modB", true), row("modA", false)]).unwrap();
    assert_eq!(warned(&f), ["relations.requires_disabled"]);

    save_profile(&f.data, SLOT_GAME, &[row("modA", true), row("modB", true)]).unwrap();
    assert!(warned(&f).is_empty());

    set_mod_relations(&f.data, SLOT_GAME, &a, &relations(&[], &[&b], &[])).unwrap();
    assert_eq!(warned(&f), ["relations.conflict"]);

    // A mod below a mod it should override is a warning, and sorting by the rules moves it up.
    set_mod_relations(&f.data, SLOT_GAME, &a, &relations(&[], &[], &[])).unwrap();
    set_mod_relations(&f.data, SLOT_GAME, &b, &relations(&[], &[], &[&a])).unwrap();
    assert_eq!(warned(&f), ["relations.order"]);
    sort_by_rules(&f.data, SLOT_GAME).unwrap();
    let order: Vec<String> = status(&f.data, SLOT_GAME)
        .unwrap()
        .mods
        .into_iter()
        .map(|m| m.folder)
        .collect();
    assert_eq!(order, ["modB", "modA"]);
    assert!(warned(&f).is_empty());

    let unknown = "nope".to_string();
    assert!(set_mod_relations(&f.data, SLOT_GAME, &a, &relations(&[&unknown], &[], &[])).is_err());
}

#[test]
fn a_nexus_requirement_is_missing_until_a_mod_with_that_id_is_on() {
    let f = fixture("nexus-requires");
    f.import_two_mods();
    let a = folder_id(&f, "modA");
    let shown = |folder: &str| {
        crate::status::details::mod_extras(&f.data, SLOT_GAME, &[folder.to_string()])
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .requirements
    };
    let requirement = |mod_id: u32, name: &str| NexusRequirement {
        mod_id,
        name: name.into(),
        from_nexus: false,
        group: None,
        ignored: false,
    };
    let relations = |nexus_requires: Vec<NexusRequirement>| Relations {
        requires: Vec::new(),
        requirements: nexus_requires
            .into_iter()
            .map(SourceRequirement::from)
            .collect(),
        incompatible: Vec::new(),
        overrides: Vec::new(),
    };

    // modA needs Nexus mod 777, which is not in the library: warned while modA is on.
    save_profile(&f.data, SLOT_GAME, &[row("modA", true), row("modB", false)]).unwrap();
    set_mod_relations(
        &f.data,
        SLOT_GAME,
        &a,
        &relations(vec![requirement(777, "RED4ext")]),
    )
    .unwrap();
    assert_eq!(warned(&f), ["relations.requires_missing"]);
    let missing = shown("modA");
    assert_eq!(missing[0].state, RequirementState::Missing);
    assert_eq!(missing[0].name, "RED4ext");

    // modB gets Nexus id 777 but stays off: the mod is in the library, so the requirement is only switched off.
    set_mod_source(
        &f.data,
        SLOT_GAME,
        &folder_id(&f, "modB"),
        "nexus",
        Some("777".into()),
    )
    .unwrap();
    assert_eq!(warned(&f), ["relations.requires_disabled"]);
    assert_eq!(shown("modA")[0].state, RequirementState::Disabled);

    save_profile(&f.data, SLOT_GAME, &[row("modA", true), row("modB", true)]).unwrap();
    assert!(warned(&f).is_empty());
    assert_eq!(shown("modA")[0].state, RequirementState::Enabled);

    // The id zero, the mod itself, and an id that a mod of the library has are refused: those go in the list of mods.
    let b = folder_id(&f, "modB");
    assert!(
        set_mod_relations(&f.data, SLOT_GAME, &a, &relations(vec![requirement(0, "")])).is_err()
    );
    assert!(
        set_mod_relations(
            &f.data,
            SLOT_GAME,
            &b,
            &relations(vec![requirement(777, "")])
        )
        .is_err()
    );
    assert!(
        set_mod_relations(
            &f.data,
            SLOT_GAME,
            &a,
            &relations(vec![requirement(777, "")])
        )
        .is_err()
    );

    // A name left empty reads as the Nexus id.
    set_mod_relations(
        &f.data,
        SLOT_GAME,
        &a,
        &relations(vec![requirement(888, "  ")]),
    )
    .unwrap();
    assert_eq!(shown("modA")[0].name, "Nexus #888");
    assert_eq!(shown("modA")[0].state, RequirementState::Missing);
}

#[test]
fn a_requirement_nexus_lists_survives_a_save_of_the_relations() {
    let f = fixture("nexus-listed");
    f.import_two_mods();
    let a = folder_id(&f, "modA");
    set_mod_source(
        &f.data,
        SLOT_GAME,
        &folder_id(&f, "modB"),
        "nexus",
        Some("778".into()),
    )
    .unwrap();

    // Nexus lists two requirements for modA's newest file: one of them is a mod in the library.
    let game = test_games::slot();
    let data = f.data.game(SLOT_GAME);
    let mut library = load_library(&data, &game).unwrap();
    let family = library.families.get_mut(&a).unwrap();
    set_nexus_requirements(family, listed_requirements());
    library.save(&data.index()).unwrap();

    // Saving the list as the inspector sends it: the mod in the library does not block the save, and the typed
    // requirement is added. The Nexus ones keep their flag.
    let typed = NexusRequirement {
        mod_id: 777,
        name: "RED4ext".into(),
        from_nexus: false,
        group: None,
        ignored: false,
    };
    let mut sent = listed_requirements();
    sent.push(typed);
    set_mod_relations(
        &f.data,
        SLOT_GAME,
        &a,
        &Relations {
            requires: Vec::new(),
            requirements: sent.into_iter().map(SourceRequirement::from).collect(),
            incompatible: Vec::new(),
            overrides: Vec::new(),
        },
    )
    .unwrap();

    let saved = load_library(&data, &game).unwrap().families[&a]
        .requirements
        .iter()
        .filter(|requirement| requirement.source.provider.as_str() == "nexus")
        .map(|requirement| {
            (
                requirement.source.mod_id.parse::<u32>().unwrap(),
                requirement.from_source,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(saved, [(778, true), (779, true), (777, false)]);
}

#[test]
fn alternatives_are_met_by_any_one_of_them_and_dlcs_are_only_a_note() {
    let f = fixture("nexus-groups");
    f.import_two_mods();
    let (a, b) = (folder_id(&f, "modA"), folder_id(&f, "modB"));
    set_mod_source(&f.data, SLOT_GAME, &b, "nexus", Some("902".into())).unwrap();

    // modA needs one of two Nexus mods (a group) and one Nexus mod of its own, and Nexus names a DLC for it.
    edit_family_on_disk(&f, &a, |family| {
        set_nexus_requirements(family, option_and_standalone());
        family.source_dlcs.insert(
            crate::domain::source::ProviderId::new("nexus"),
            vec![vec!["Phantom Liberty".into()]],
        );
    });
    let shown = |folder: &str| {
        crate::status::details::mod_extras(&f.data, SLOT_GAME, &[folder.to_string()])
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
    };

    // The mod with id 902 is in the library but off, so the list is unmet: one warning for the list, one for the other.
    save_profile(&f.data, SLOT_GAME, &[row("modA", true), row("modB", false)]).unwrap();
    assert_eq!(
        warned(&f),
        ["relations.requires_one_of", "relations.requires_missing"]
    );
    let view = shown("modA");
    let states: Vec<(u32, Option<u32>, RequirementState)> = view
        .requirements
        .iter()
        .map(|requirement| {
            (
                requirement.source.mod_id.parse().unwrap(),
                requirement.group,
                requirement.state,
            )
        })
        .collect();
    assert_eq!(
        states,
        [
            (901, Some(0), RequirementState::Missing),
            (902, Some(0), RequirementState::Disabled),
            (903, None, RequirementState::Missing),
        ]
    );
    assert!(
        view.requirements
            .iter()
            .any(|requirement| requirement.source.provider.as_str() == "nexus")
    );
    assert_eq!(
        shown("modA")
            .source_dlcs
            .iter()
            .find(|entry| entry.provider == "nexus")
            .map(|entry| entry.dlcs.clone()),
        Some(vec![vec!["Phantom Liberty".to_string()]])
    );

    // Switching the mod of the list on meets the list: only the requirement of its own is still missing.
    save_profile(&f.data, SLOT_GAME, &[row("modA", true), row("modB", true)]).unwrap();
    assert_eq!(warned(&f), ["relations.requires_missing"]);
}

#[test]
fn the_count_of_unmet_requirements_follows_the_profile_not_the_mod_state() {
    let f = fixture("unmet-count");
    f.import_two_mods();
    let (a, b) = (folder_id(&f, "modA"), folder_id(&f, "modB"));
    let unmet = |folder: &str| {
        status(&f.data, SLOT_GAME)
            .unwrap()
            .mods
            .into_iter()
            .find(|m| m.folder == folder)
            .unwrap()
            .unmet_requirements
    };

    // modA needs modB, one Nexus list none of whose mods is in the library, and a Nexus mod of its own.
    set_mod_relations(
        &f.data,
        SLOT_GAME,
        &a,
        &Relations {
            requires: vec![b],
            ..Relations::default()
        },
    )
    .unwrap();
    edit_family_on_disk(&f, &a, |family| {
        set_nexus_requirements(family, option_and_standalone());
    });

    // modB is off: the requirement on it, the list and the Nexus mod of its own are unmet. modB needs nothing.
    save_profile(&f.data, SLOT_GAME, &[row("modA", true), row("modB", false)]).unwrap();
    assert_eq!(unmet("modA"), 3);
    assert_eq!(unmet("modB"), 0);

    // The count does not depend on whether modA itself is on.
    save_profile(&f.data, SLOT_GAME, &[row("modA", false), row("modB", true)]).unwrap();
    assert_eq!(unmet("modA"), 2);

    save_profile(&f.data, SLOT_GAME, &[row("modA", true), row("modB", true)]).unwrap();
    assert_eq!(unmet("modA"), 2);
    assert_eq!(unmet("modB"), 0);
}

#[test]
fn an_ignored_requirement_is_not_counted_and_keeps_its_switch_across_saves() {
    let f = fixture("nexus-ignored");
    f.import_two_mods();
    let a = folder_id(&f, "modA");
    save_profile(&f.data, SLOT_GAME, &[row("modA", true), row("modB", false)]).unwrap();
    let requirement = |ignored: bool| SourceRequirement {
        source: crate::domain::source::SourceRef::new("nexus", "777"),
        name: "ReShade".into(),
        from_source: false,
        group: None,
        ignored,
    };
    let relations = |requirements: Vec<SourceRequirement>| Relations {
        requires: Vec::new(),
        requirements,
        incompatible: Vec::new(),
        overrides: Vec::new(),
    };
    let unmet = || {
        status(&f.data, SLOT_GAME)
            .unwrap()
            .mods
            .into_iter()
            .find(|m| m.folder == "modA")
            .unwrap()
            .unmet_requirements
    };
    let shown = || {
        crate::status::details::mod_extras(&f.data, SLOT_GAME, &["modA".to_string()])
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
    };

    set_mod_relations(&f.data, SLOT_GAME, &a, &relations(vec![requirement(false)])).unwrap();
    assert_eq!(warned(&f), ["relations.requires_missing"]);
    assert_eq!(unmet(), 1);

    // The ignore switch comes from the interface with the rest of the list, and the requirement stops counting.
    set_mod_relations(&f.data, SLOT_GAME, &a, &relations(vec![requirement(true)])).unwrap();
    assert!(warned(&f).is_empty());
    assert_eq!(unmet(), 0);
    assert!(shown().requirements[0].ignored);

    // The inspector sends the list back as it got it: the switch is still on.
    let sent = shown()
        .requirements
        .iter()
        .map(|view| SourceRequirement {
            source: view.source.clone(),
            name: view.name.clone(),
            from_source: view.from_source,
            group: view.group,
            ignored: view.ignored,
        })
        .collect();
    set_mod_relations(&f.data, SLOT_GAME, &a, &relations(sent)).unwrap();
    assert!(warned(&f).is_empty());
    assert_eq!(unmet(), 0);
    assert!(shown().requirements[0].ignored);
}

/// The external requirements of a mod as the status shows them: the provider's mod id and whether each is ignored.
fn requirement_list(f: &Fixture, folder: &str) -> Vec<(String, bool)> {
    crate::status::details::mod_extras(&f.data, SLOT_GAME, &[folder.to_string()])
        .unwrap()
        .into_iter()
        .next()
        .unwrap()
        .requirements
        .into_iter()
        .map(|view| (view.source.mod_id, view.ignored))
        .collect()
}

#[test]
fn two_quick_requirement_adds_keep_both_and_a_repeat_of_a_listed_mod_changes_nothing() {
    let f = fixture("requirement-adds");
    f.import_two_mods();
    let a = folder_id(&f, "modA");
    let typed = |mod_id: &str, name: &str| SourceRequirement {
        source: crate::domain::source::SourceRef::new("nexus", mod_id),
        name: name.into(),
        from_source: false,
        group: None,
        ignored: false,
    };

    add_requirement(&f.data, SLOT_GAME, &a, &typed("701", "Helper one")).unwrap();
    add_requirement(&f.data, SLOT_GAME, &a, &typed("702", "Helper two")).unwrap();
    add_requirement(&f.data, SLOT_GAME, &a, &typed("701", "Renamed")).unwrap();
    assert_eq!(
        requirement_list(&f, "modA"),
        vec![("701".to_string(), false), ("702".to_string(), false)]
    );

    // The switch of one requirement leaves the other one as it was, and removing one keeps the rest.
    set_requirement_ignored(&f.data, SLOT_GAME, &a, "nexus", "702", true).unwrap();
    assert_eq!(
        requirement_list(&f, "modA"),
        vec![("701".to_string(), false), ("702".to_string(), true)]
    );
    remove_requirement(&f.data, SLOT_GAME, &a, "nexus", "701").unwrap();
    assert_eq!(
        requirement_list(&f, "modA"),
        vec![("702".to_string(), true)]
    );

    // Neither a removal nor a switch of a mod that is not listed changes the list.
    remove_requirement(&f.data, SLOT_GAME, &a, "nexus", "999").unwrap();
    set_requirement_ignored(&f.data, SLOT_GAME, &a, "nexus", "999", false).unwrap();
    assert_eq!(
        requirement_list(&f, "modA"),
        vec![("702".to_string(), true)]
    );
}

/// The three relation lists of a family, as saved in the index.
fn relation_lists(f: &Fixture, id: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let game = test_games::slot();
    let data = f.data.game(SLOT_GAME);
    let library = load_library(&data, &game).unwrap();
    let family = &library.families[id];
    (
        family.requires.clone(),
        family.incompatible.clone(),
        family.overrides.clone(),
    )
}

#[test]
fn two_quick_relation_edits_keep_both_without_reloading() {
    let f = fixture("relation-adds");
    f.import_two_mods();
    let (a, b) = (folder_id(&f, "modA"), folder_id(&f, "modB"));

    // Both edits name the same target in two lists. The second must not overwrite the first, as a whole-list save
    // built from the stale status would.
    add_relation(&f.data, SLOT_GAME, &a, RelationKind::Requires, &b).unwrap();
    add_relation(&f.data, SLOT_GAME, &a, RelationKind::Incompatible, &b).unwrap();
    assert_eq!(
        relation_lists(&f, &a),
        (vec![b.clone()], vec![b.clone()], Vec::new())
    );

    // A repeat of a relation changes nothing, and a mod outside the library is refused.
    add_relation(&f.data, SLOT_GAME, &a, RelationKind::Requires, &b).unwrap();
    assert!(add_relation(&f.data, SLOT_GAME, &a, RelationKind::Overrides, "missing").is_err());
    assert_eq!(
        relation_lists(&f, &a),
        (vec![b.clone()], vec![b.clone()], Vec::new())
    );

    // Removing one list keeps the other; removing a mod that is not listed changes nothing.
    remove_relation(&f.data, SLOT_GAME, &a, RelationKind::Requires, &b).unwrap();
    remove_relation(&f.data, SLOT_GAME, &a, RelationKind::Requires, "missing").unwrap();
    assert_eq!(relation_lists(&f, &a), (Vec::new(), vec![b], Vec::new()));
}
