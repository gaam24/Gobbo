//! Save snapshots through the public entries the window calls: they take the app's data folder and keep the profiles
//! in the game's data folder.

use super::*;
use crate::context::profile::data as profile_data;
use crate::snapshots::{SnapshotKind, create_manual, list, preview_public, restore_public};
use crate::test_support::write;

#[test]
fn a_manual_snapshot_lives_in_the_app_folder_and_restores_the_profile_data() {
    let f = fixture("snapshots");
    let saves = f.base.join("saves");
    write(&saves.join("slot.sav"), "one");
    let settings = Settings {
        profile_data: vec![saves.display().to_string()],
        ..f.settings()
    };
    save_settings(&f.data, SLOT_GAME, settings.clone()).unwrap();

    let profile = f.active_id(SLOT_GAME);
    let game = test_games::slot();
    let game_data = f.data.game(SLOT_GAME);
    profile_data::switch(&game_data, &game, &settings, &profile).unwrap();
    assert!(
        game_data.profile_state(&profile).join("data").is_dir(),
        "the switch seeds the profile store"
    );

    let snapshot = create_manual(&f.data, SLOT_GAME, &profile).unwrap();
    assert_eq!(snapshot.profile, profile);
    assert_eq!(snapshot.kind, SnapshotKind::Manual);
    assert!(
        f.data
            .dir
            .join("snapshots")
            .join(SLOT_GAME)
            .join(&snapshot.id)
            .is_dir()
    );
    assert!(!f.data.game(SLOT_GAME).dir.join("snapshots").exists());

    let listed = list(&f.data, SLOT_GAME);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, snapshot.id);
    assert_eq!(listed[0].profile_name, "Default");

    write(&saves.join("slot.sav"), "two");
    let state_data = game_data.profile_state(&profile).join("data");
    let stored = crate::infra::fs::list_files(&state_data).unwrap();
    assert_eq!(stored.len(), 1);
    write(&state_data.join(&stored[0]), "two");
    let preview = preview_public(&f.data, SLOT_GAME, &snapshot.id).unwrap();
    assert_eq!(preview.replaced, 1);
    assert_eq!(preview.added, 0);
    assert_eq!(preview.removed, 0);

    let outcome = restore_public(&f.data, SLOT_GAME, &snapshot.id).unwrap();
    assert_eq!(fs::read_to_string(saves.join("slot.sav")).unwrap(), "one");
    assert_eq!(
        outcome.backup.expect("a backup").kind,
        SnapshotKind::BeforeRestore
    );
    assert_eq!(list(&f.data, SLOT_GAME).len(), 2);
}

/// A fixture with one copied save, a switched profile and one manual snapshot to restore.
fn snapshot_fixture(name: &str) -> (Fixture, PathBuf, String) {
    let f = fixture(name);
    let saves = f.base.join("saves");
    write(&saves.join("slot.sav"), "one");
    let settings = Settings {
        profile_data: vec![saves.display().to_string()],
        ..f.settings()
    };
    save_settings(&f.data, SLOT_GAME, settings.clone()).unwrap();
    let profile = f.active_id(SLOT_GAME);
    let game = test_games::slot();
    profile_data::switch(&f.data.game(SLOT_GAME), &game, &settings, &profile).unwrap();
    let snapshot = create_manual(&f.data, SLOT_GAME, &profile).unwrap();
    (f, saves, snapshot.id)
}

#[test]
fn a_restore_reenters_a_state_lock_its_caller_holds() {
    let (f, saves, id) = snapshot_fixture("snapshots-lock-reentrant");
    let held = crate::context::lock_state(&f.data.dir).unwrap();

    let outcome = restore_public(&f.data, SLOT_GAME, &id).unwrap();
    assert_eq!(
        outcome.backup.expect("a backup").kind,
        SnapshotKind::BeforeRestore
    );
    assert_eq!(fs::read_to_string(saves.join("slot.sav")).unwrap(), "one");

    // The nested lock of the restore must not release the outer one.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            sender
                .send(crate::context::lock_state(&f.data.dir).is_err())
                .unwrap();
        });
    });
    assert!(receiver.recv().unwrap(), "the outer lock is still held");
    drop(held);
}

#[test]
fn a_restore_in_another_thread_sees_the_held_state_lock() {
    let (f, _, id) = snapshot_fixture("snapshots-lock-busy");
    let _held = crate::context::lock_state(&f.data.dir).unwrap();

    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let error = restore_public(&f.data, SLOT_GAME, &id).unwrap_err();
            sender.send(error.code).unwrap();
        });
    });
    assert_eq!(receiver.recv().unwrap(), "state.busy");
}
