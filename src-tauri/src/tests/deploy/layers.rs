//! Overlay layers: one per profile, kept across switches, and refused while the game runs.

use super::*;
use crate::context::data::JOURNAL_FILE;
use crate::deploy::journal::{Entry, Journal};
#[cfg(target_os = "linux")]
use crate::deploy::ops::{
    apply_deployment, ensure_active_deployed, record_launch, undo_deployment,
};
use crate::deploy::overlay;
#[cfg(target_os = "linux")]
use crate::deploy::remove::delete_mod;
#[cfg(target_os = "linux")]
use crate::deploy::remove::reinstall_mod;
use crate::domain::deployment::DeploymentRecord;
#[cfg(target_os = "linux")]
use crate::launch::start::{game_running, overlay_session_state};
#[cfg(target_os = "linux")]
use crate::profiles::actions::switch_profile;

/// Writes a finished journal for a prepared state: the entries and the completion marker the undo reads.
fn prepared_state(state: &Path, entries: &[Entry]) -> PathBuf {
    fs::create_dir_all(state).unwrap();
    let journal = Journal::new(state.join(JOURNAL_FILE));
    for entry in entries {
        journal.append(entry).unwrap();
    }
    journal
        .complete(&DeploymentRecord {
            schema_version: crate::domain::STATE_SCHEMA,
            files: 0,
            plan_hash: String::new(),
        })
        .unwrap();
    drop(journal);
    state.to_path_buf()
}

#[test]
fn make_room_undoes_a_legacy_and_an_overlay_profile_once() {
    let f = fixture("make-room-load-order");
    let game = f.data.game(SLOT_GAME);
    let legacy = game.profile_state("p1");
    let layer = game.profile_state("p2");
    let placed = f.game_root.join("Mods/modA/content/x.ws");
    crate::test_support::write(&placed, "from A");
    crate::test_support::write(&layer.join("overwrite/game-write.txt"), "keep");

    // Two finished deployments: one wrote into the game folder, the other only prepared a layer.
    prepared_state(
        &legacy,
        &[Entry::FileDeployed {
            path: placed.clone(),
        }],
    );
    prepared_state(
        &layer,
        &[Entry::LayerPrepared {
            mods: layer.join("mods"),
            overwrite: layer.join("overwrite"),
        }],
    );
    let prepared = game.prepared_states();
    assert_eq!(prepared.len(), 2);

    // The fixture uses a load order, where an overlay apply makes room from every other profile too.
    let def = test_games::slot();
    assert!(def.has_load_order());
    let roots = crate::deploy::target::journal_roots(&def, &f.settings(), game.profile_state("p3"))
        .unwrap();
    overlay::make_room(&prepared, "p3", def.has_load_order(), false, &roots).unwrap();

    assert!(!placed.exists(), "the legacy deployment is undone");
    assert!(!legacy.join(JOURNAL_FILE).exists());
    assert!(!layer.join(JOURNAL_FILE).exists());
    assert!(
        layer.join("overwrite/game-write.txt").is_file(),
        "the game's writes stay"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn each_profile_keeps_its_own_staging_across_switches() {
    let f = fixture("profile-staging");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let first = f.active_id(SLOT_GAME);
    apply_deployment(&f.data, SLOT_GAME).unwrap();
    undo_deployment(&f.data, SLOT_GAME).unwrap();

    create_profile(&f.data, SLOT_GAME, "Druga", None, false).unwrap();
    let second = f.profile_id(SLOT_GAME, "Druga");
    switch_profile(&f.data, SLOT_GAME, &second).unwrap();
    let only_b = [row("modB", true)];
    save_profile(&f.data, SLOT_GAME, &only_b).unwrap();
    apply_deployment(&f.data, SLOT_GAME).unwrap();
    undo_deployment(&f.data, SLOT_GAME).unwrap();

    let game = f.data.game(SLOT_GAME);
    assert!(game.profile_state(&first).join("mods/Mods/modA").is_dir());
    assert!(game.profile_state(&second).join("mods/Mods/modB").is_dir());
    assert!(!game.profile_state(&second).join("mods/Mods/modA").exists());

    switch_profile(&f.data, SLOT_GAME, &first).unwrap();
    let applied = apply_deployment(&f.data, SLOT_GAME).unwrap();
    assert_eq!(applied.files, 1);
    undo_deployment(&f.data, SLOT_GAME).unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn deploying_and_undoing_refuse_while_the_game_runs() {
    use std::os::unix::fs::PermissionsExt;

    let f = fixture("deploy-running");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    apply_deployment(&f.data, SLOT_GAME).unwrap();
    // The launch records the profile, so undo knows this layer belongs to the running game.
    record_launch(&f.data, SLOT_GAME).unwrap();

    // A running game: a process named `proton` with a file of the game on the command line, as Steam starts it.
    let proton = f.game_root.join("bin/proton");
    fs::create_dir_all(proton.parent().unwrap()).unwrap();
    fs::write(&proton, "#!/bin/sh\nsleep 30\n").unwrap();
    fs::set_permissions(&proton, fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = std::process::Command::new(&proton)
        .arg("waitforexitandrun")
        .arg(f.game_root.join("bin/game.sh"))
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));

    assert!(game_running(&f.data, SLOT_GAME).unwrap());
    let apply_error = apply_deployment(&f.data, SLOT_GAME).unwrap_err();
    let undo_error = undo_deployment(&f.data, SLOT_GAME).unwrap_err();
    let reinstall_error = reinstall_mod(&f.data, SLOT_GAME, "modA", None).unwrap_err();
    child.kill().unwrap();
    child.wait().unwrap();

    assert_eq!(apply_error.code, "deploy.game_running", "{apply_error}");
    assert_eq!(undo_error.code, "deploy.game_running", "{undo_error}");
    assert_eq!(
        reinstall_error.code, "deploy.game_running",
        "{reinstall_error}"
    );
    // The deployment survives: nothing was removed under the running game.
    assert!(status(&f.data, SLOT_GAME).unwrap().deployed);
}

#[cfg(target_os = "linux")]
#[test]
fn deleting_a_mod_cleans_its_overlay_layer_but_keeps_game_writes() {
    let f = fixture("delete-overlay");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    apply_deployment(&f.data, SLOT_GAME).unwrap();
    let state = f.data.game(SLOT_GAME).profile_state("p1");
    assert!(state.join("mods/Mods/modA").is_dir());
    fs::write(state.join("overwrite/game-write.txt"), "keep me").unwrap();

    delete_mod(&f.data, SLOT_GAME, "local:moda").unwrap();
    assert!(!state.join("mods").exists());
    assert!(state.join("overwrite/game-write.txt").is_file());
    assert!(!state.join(JOURNAL_FILE).exists());
    assert!(!status(&f.data, SLOT_GAME).unwrap().deployed);
}

#[cfg(target_os = "linux")]
#[test]
fn two_overlay_profiles_keep_their_own_layers() {
    let f = mirror_fixture("multi-layer");
    for (name, folder) in [("Foo-1.zip", "Foo"), ("Bar-2.zip", "Bar")] {
        let entry = format!("r6/scripts/{folder}/a.reds");
        let zip = f.zip(name, &[(entry.as_str(), folder)]);
        import(&f.data, MIRROR_GAME, &zip, None, &mut |_, _| {}).unwrap();
    }
    f.enable_all(MIRROR_GAME);
    let first = f.active_id(MIRROR_GAME);
    apply_deployment(&f.data, MIRROR_GAME).unwrap();

    create_profile(&f.data, MIRROR_GAME, "Tylko Bar", None, false).unwrap();
    let second = f.profile_id(MIRROR_GAME, "Tylko Bar");
    switch_profile(&f.data, MIRROR_GAME, &second).unwrap();
    save_profile(&f.data, MIRROR_GAME, &[row("Bar-2", true)]).unwrap();
    apply_deployment(&f.data, MIRROR_GAME).unwrap();

    let game = f.data.game(MIRROR_GAME);
    assert!(
        game.profile_state(&first)
            .join("mods/r6/scripts/Foo/a.reds")
            .is_file()
    );
    assert!(
        game.profile_state(&second)
            .join("mods/r6/scripts/Bar/a.reds")
            .is_file()
    );
    assert!(
        !game
            .profile_state(&second)
            .join("mods/r6/scripts/Foo")
            .exists()
    );
    assert_eq!(game.prepared_states().len(), 2);

    // Launch uses the active profile's layer, not the first prepared one.
    ensure_active_deployed(&f.data, MIRROR_GAME, false).unwrap();
    let active_state = overlay_session_state(&game).unwrap().unwrap();
    assert_eq!(active_state, game.profile_state(&second));

    switch_profile(&f.data, MIRROR_GAME, &first).unwrap();
    ensure_active_deployed(&f.data, MIRROR_GAME, false).unwrap();
    let active_state = overlay_session_state(&game).unwrap().unwrap();
    assert_eq!(active_state, game.profile_state(&first));

    undo_deployment(&f.data, MIRROR_GAME).unwrap();
    switch_profile(&f.data, MIRROR_GAME, &second).unwrap();
    undo_deployment(&f.data, MIRROR_GAME).unwrap();
    assert!(game.prepared_states().is_empty());
}
