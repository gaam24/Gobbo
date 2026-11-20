//! The requirement closure: transitive chains, alternatives, cycles and a requirement no key can install.

use std::sync::atomic::AtomicBool;

use super::*;
use crate::domain::source::{SourceRef, SourceRequirement};
use crate::library::Relations;
use crate::library::set_mod_relations;
use crate::sources::requirements::{RequirementAction, install_cancellable, plan};

fn folder_id(f: &Fixture, folder: &str) -> String {
    status(&f.data, SLOT_GAME)
        .unwrap()
        .mods
        .iter()
        .find(|item| item.folder == folder)
        .unwrap()
        .id
        .clone()
}

fn import_third(f: &Fixture) {
    let zip = f.zip("modC.zip", &[("Mods/modC/content/x.ws", "from C")]);
    import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
}

fn requires(f: &Fixture, id: &str, required: &[&String]) {
    set_mod_relations(
        &f.data,
        SLOT_GAME,
        id,
        &Relations {
            requires: required.iter().map(|id| (*id).clone()).collect(),
            ..Relations::default()
        },
    )
    .unwrap();
}

#[test]
fn a_three_level_chain_is_enabled_and_one_undo_takes_it_back() {
    let f = fixture("requirements-chain");
    f.import_two_mods();
    import_third(&f);
    let a = folder_id(&f, "modA");
    let b = folder_id(&f, "modB");
    let c = folder_id(&f, "modC");
    requires(&f, &b, &[&a]);
    requires(&f, &c, &[&b]);
    save_profile(
        &f.data,
        SLOT_GAME,
        &[row("modC", true), row("modB", false), row("modA", false)],
    )
    .unwrap();

    let preview = plan(&f.data, SLOT_GAME, "modC").unwrap();
    assert_eq!(preview.enable, 2);
    assert_eq!(preview.download, 0);
    assert_eq!(preview.blocked, 0);
    assert_eq!(
        preview
            .steps
            .iter()
            .map(|step| (step.action, step.name.as_str(), step.depth))
            .collect::<Vec<_>>(),
        [
            (RequirementAction::Enable, "modB", 1),
            (RequirementAction::Enable, "modA", 2),
        ]
    );

    let installed = install_cancellable(&f.data, SLOT_GAME, "modC", None, &mut |_| {}).unwrap();
    assert_eq!(installed.installed, 2);
    assert!(installed.blocked.is_empty());
    assert!(installed.failed.is_empty());
    assert!(
        installed
            .status
            .mods
            .iter()
            .filter(|item| ["modA", "modB"].contains(&item.folder.as_str()))
            .all(|item| item.enabled)
    );

    crate::context::profile::history::undo_list(&f.data, SLOT_GAME).unwrap();
    let after = status(&f.data, SLOT_GAME).unwrap();
    assert!(
        after
            .mods
            .iter()
            .filter(|item| ["modA", "modB"].contains(&item.folder.as_str()))
            .all(|item| !item.enabled)
    );
}

#[test]
fn an_alternative_group_needs_one_step_and_says_which_key_is_missing() {
    let f = fixture("requirements-alternatives");
    f.import_two_mods();
    import_third(&f);
    let c = folder_id(&f, "modC");
    let game = test_games::slot();
    let data = f.data.game(SLOT_GAME);
    let mut library = load_library(&data, &game).unwrap();
    let family = library.families.get_mut(&c).unwrap();

    for (mod_id, name) in [("owner-one", "One"), ("owner-two", "Two")] {
        family.requirements.push(SourceRequirement {
            source: SourceRef::new("thunderstore", mod_id),
            name: name.to_string(),
            from_source: true,
            group: Some(1),
            ignored: false,
        });
    }

    library.save(&data.index()).unwrap();

    let preview = plan(&f.data, SLOT_GAME, "modC").unwrap();
    assert_eq!(preview.steps.len(), 1);
    let step = &preview.steps[0];
    assert_eq!(step.action, RequirementAction::Blocked);
    assert!(
        step.group
            .as_deref()
            .is_some_and(|group| group.starts_with("one-of:"))
    );
    assert_eq!(
        step.message.as_ref().map(|message| message.code.as_str()),
        Some("source.no_game_key")
    );
    assert_eq!(preview.blocked, 1);
}

#[test]
fn a_cycle_is_reported_and_not_followed() {
    let f = fixture("requirements-cycle");
    f.import_two_mods();
    import_third(&f);
    let a = folder_id(&f, "modA");
    let b = folder_id(&f, "modB");
    let c = folder_id(&f, "modC");
    requires(&f, &b, &[&a]);
    requires(&f, &c, &[&b]);
    requires(&f, &a, &[&c]);
    save_profile(
        &f.data,
        SLOT_GAME,
        &[row("modC", true), row("modB", false), row("modA", false)],
    )
    .unwrap();

    let preview = plan(&f.data, SLOT_GAME, "modC").unwrap();
    assert_eq!(preview.steps.len(), 2);
    assert_eq!(preview.cycles.len(), 1);
    assert_eq!(preview.cycles[0].code, "requirements.cycle");
    assert!(
        preview.cycles[0].params["path"].contains("modC"),
        "{:?}",
        preview.cycles[0]
    );
}

#[test]
fn a_cancelled_requirement_install_enables_nothing() {
    let f = fixture("requirements-cancelled");
    f.import_two_mods();
    let flag = AtomicBool::new(true);
    let error =
        install_cancellable(&f.data, SLOT_GAME, "modA", Some(&flag), &mut |_| {}).unwrap_err();
    assert_eq!(error.code, crate::import::IMPORT_CANCELLED);
    let after = status(&f.data, SLOT_GAME).unwrap();
    assert!(after.mods.iter().all(|item| !item.enabled));
}
