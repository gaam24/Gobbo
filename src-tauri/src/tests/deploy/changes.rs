//! Changing mods while a deployment is active: an import, and a delete that cleans up the deployments using it.

use super::*;
use crate::deploy::ops::apply_deployment;
use crate::deploy::ops::record_launch;
use crate::deploy::remove::delete_mod;
use crate::deploy::remove::delete_unused_versions;
use crate::deploy::remove::delete_version;
use crate::deploy::remove::reinstall_mod;
use crate::library::set_file_winner;
use crate::status::details::mod_versions;
use crate::status::sync_profiles as status;
use crate::test_support::import;

#[test]
fn import_works_while_a_deployment_is_active() {
    let f = fixture("import-deployed");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    apply_deployment(&f.data, SLOT_GAME).unwrap();

    let zip = f.zip("more.zip", &[("Mods/modC/content/x.ws", "from C")]);
    import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();

    let status = status(&f.data, SLOT_GAME).unwrap();
    assert!(status.deployed);
    let adopted = status.mods.iter().find(|m| m.folder == "more").unwrap();
    assert!(!adopted.enabled);
    assert_eq!(status.draft_changes, 0);
}

#[test]
fn deleting_a_mod_cleans_the_deployments_that_use_it() {
    let f = fixture("delete-deployed");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    apply_deployment(&f.data, SLOT_GAME).unwrap();

    delete_mod(&f.data, SLOT_GAME, "local:moda").unwrap();
    assert!(!f.data.game(SLOT_GAME).library().join("modA").exists());
    assert!(!f.game_root.join("Mods/modA").exists());
    assert!(f.data.game(SLOT_GAME).prepared_states().is_empty());
    let view = status(&f.data, SLOT_GAME).unwrap();
    assert!(!view.deployed);
    assert_eq!(
        view.mods
            .iter()
            .map(|mod_view| mod_view.folder.as_str())
            .collect::<Vec<_>>(),
        ["modB"]
    );
}

#[test]
fn deleting_a_deployed_family_forgets_its_winner_and_launch_memory() {
    let f = fixture("delete-ledger");
    f.import_two_mods();
    save_profile(&f.data, SLOT_GAME, &[row("modA", false), row("modB", true)]).unwrap();
    let mod_b = status(&f.data, SLOT_GAME)
        .unwrap()
        .mods
        .iter()
        .find(|mod_view| mod_view.folder == "modB")
        .unwrap()
        .id
        .clone();
    set_file_winner(&f.data, SLOT_GAME, "content/x.ws", Some(&mod_b)).unwrap();
    apply_deployment(&f.data, SLOT_GAME).unwrap();
    record_launch(&f.data, SLOT_GAME).unwrap();

    let before = load_profiles(&f.data.game(SLOT_GAME)).unwrap();
    assert_eq!(before.active().deployed_mods.len(), 1);
    assert_eq!(before.active().deployed_winners.len(), 1);
    assert_eq!(
        before.last_launched.as_deref(),
        Some(before.active.as_str())
    );

    delete_mod(&f.data, SLOT_GAME, &mod_b).unwrap();

    let after = load_profiles(&f.data.game(SLOT_GAME)).unwrap();
    assert!(after.active().deployed_mods.is_empty());
    assert!(after.active().deployed_winners.is_empty());
    assert!(after.active().deployed_at.is_none());
    assert!(after.active().winners.is_empty());
    assert!(after.last_launched.is_none());

    // The same archive comes back as the same family. It must not win the old file again.
    let zip = f.zip("modB.zip", &[("Mods/modB/content/x.ws", "from B again")]);
    import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    save_profile(&f.data, SLOT_GAME, &[row("modA", true), row("modB", true)]).unwrap();
    apply_deployment(&f.data, SLOT_GAME).unwrap();
    let layer = f
        .data
        .game(SLOT_GAME)
        .profile_state(&f.active_id(SLOT_GAME))
        .join("mods");
    assert_eq!(
        fs::read_to_string(layer.join("Mods/modA/content/x.ws")).unwrap(),
        "from A"
    );
}

#[test]
fn deleting_a_deployed_version_cleans_the_deployment_and_switches_the_profile() {
    let f = mirror_fixture("delete-version-deployed");
    let (old, new) = two_mod_versions(&f);
    import(&f.data, MIRROR_GAME, &old, None, &mut |_, _| {}).unwrap();
    f.enable_all(MIRROR_GAME);
    apply_deployment(&f.data, MIRROR_GAME).unwrap();
    import(&f.data, MIRROR_GAME, &new, None, &mut |_, _| {}).unwrap();

    let before = status(&f.data, MIRROR_GAME).unwrap();
    let versions = mod_versions(&f.data, MIRROR_GAME, &before.mods[0].folder).unwrap();
    let old_folder = versions
        .iter()
        .find(|version| version.version.as_deref() == Some("1.2.0"))
        .unwrap()
        .folder
        .clone();
    let new_folder = versions
        .iter()
        .find(|version| version.version.as_deref() == Some("1.2.1"))
        .unwrap()
        .folder
        .clone();

    delete_version(&f.data, MIRROR_GAME, &old_folder, Some(&new_folder)).unwrap();
    assert!(f.data.game(MIRROR_GAME).prepared_states().is_empty());
    assert!(
        !f.data
            .game(MIRROR_GAME)
            .library()
            .join(&old_folder)
            .exists()
    );
    let after = status(&f.data, MIRROR_GAME).unwrap();
    assert!(!after.deployed);
    assert_eq!(after.mods[0].folder, new_folder);
}

#[test]
fn deleting_unused_versions_keeps_a_prepared_deployment() {
    let f = mirror_fixture("unused-version-deployed");
    let (old, new) = two_mod_versions(&f);
    import(&f.data, MIRROR_GAME, &old, None, &mut |_, _| {}).unwrap();
    f.enable_all(MIRROR_GAME);
    apply_deployment(&f.data, MIRROR_GAME).unwrap();
    import(&f.data, MIRROR_GAME, &new, None, &mut |_, _| {}).unwrap();

    assert_eq!(delete_unused_versions(&f.data, MIRROR_GAME).unwrap(), 1);
    assert!(!f.data.game(MIRROR_GAME).prepared_states().is_empty());
    assert!(status(&f.data, MIRROR_GAME).unwrap().deployed);
}

#[test]
fn deleting_unused_versions_undoes_a_deployment_that_still_held_one() {
    let f = mirror_fixture("unused-version-deployed-draft");
    let (old, _) = two_mod_versions(&f);
    import(&f.data, MIRROR_GAME, &old, None, &mut |_, _| {}).unwrap();
    f.enable_all(MIRROR_GAME);
    apply_deployment(&f.data, MIRROR_GAME).unwrap();
    // The profile no longer lists the mod, but the prepared deployment still holds its files.
    save_profile(&f.data, MIRROR_GAME, &[]).unwrap();

    assert_eq!(delete_unused_versions(&f.data, MIRROR_GAME).unwrap(), 1);
    assert!(f.data.game(MIRROR_GAME).prepared_states().is_empty());
    assert!(!status(&f.data, MIRROR_GAME).unwrap().deployed);
}

#[test]
fn reinstalling_a_deployed_mod_takes_the_deployment_out_until_the_next_apply() {
    let f = fixture("reinstall-deployed");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    apply_deployment(&f.data, SLOT_GAME).unwrap();
    let layer = f
        .data
        .game(SLOT_GAME)
        .profile_state(&f.active_id(SLOT_GAME))
        .join("mods");
    assert!(layer.join("Mods/modA/content/x.ws").is_file());

    reinstall_mod(&f.data, SLOT_GAME, "modA", None).unwrap();

    // The deployment is undone as a removal undoes it: the layer goes, and the next apply builds it again.
    assert!(f.data.game(SLOT_GAME).prepared_states().is_empty());
    assert!(!layer.exists());
    assert!(!f.game_root.join("Mods/modA").exists());
    let view = status(&f.data, SLOT_GAME).unwrap();
    assert!(!view.deployed);
    assert!(
        f.data
            .game(SLOT_GAME)
            .library()
            .join("modA/content/x.ws")
            .is_file()
    );

    apply_deployment(&f.data, SLOT_GAME).unwrap();
    let layer = f
        .data
        .game(SLOT_GAME)
        .profile_state(&f.active_id(SLOT_GAME))
        .join("mods");
    assert!(layer.join("Mods/modA/content/x.ws").is_file());
}
