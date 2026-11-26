use std::fs;

use super::*;
use crate::domain::naming;
use crate::snapshots::capture::create;
use crate::snapshots::manifest::{LEGACY_ID, SETS_DIR, SnapshotKind, Target, list};
use crate::snapshots::test_support::{fixture, manifest_with, snapshot_with_file, store};
use crate::snapshots::user_data;
use crate::snapshots::{SnapshotScope, set_dir_in};
use crate::test_support::write;

#[test]
fn a_snapshot_keeps_the_profile_store_and_a_restore_puts_it_back() {
    let (app, game, settings) = fixture("snapshot-roundtrip");
    let store = store(&app, &game.id);
    write(&store.join("slot.sav"), "one");
    let legacy = app
        .game(&game.id)
        .profile_state("p1")
        .join("data/legacy.sav");
    write(&legacy, "saved");

    let snapshot = create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
    assert_eq!(snapshot.files, 2);
    assert_eq!(snapshot.bytes, 8);
    // The snapshot is stored in the app's folder, not in the game's data folder.
    assert!(
        app.dir
            .join("snapshots")
            .join(&game.id)
            .join(&snapshot.id)
            .is_dir()
    );
    assert!(!app.game(&game.id).dir.join("snapshots").exists());
    // A non-legacy set is stored under its path hash; the legacy store keeps its own name.
    let scope = SnapshotScope::new(&app, &game.id);
    let sets = scope.dir(&snapshot.id).join(SETS_DIR);
    assert!(
        sets.join(naming::path_hash("saves"))
            .join("slot.sav")
            .is_file()
    );
    assert!(sets.join(LEGACY_ID).join("legacy.sav").is_file());

    write(&store.join("slot.sav"), "two");
    write(&legacy, "changed");
    let preview = preview(&app, &game, &settings, &snapshot.id).unwrap();
    assert_eq!(preview.replaced, 2);
    assert_eq!(preview.added, 0);
    assert_eq!(preview.removed, 0);
    assert!(!preview.running);

    let outcome = restore(&app, &game, &settings, &snapshot.id).unwrap();
    assert_eq!(outcome.files, 2);
    assert_eq!(fs::read_to_string(store.join("slot.sav")).unwrap(), "one");
    assert_eq!(fs::read_to_string(&legacy).unwrap(), "saved");
    assert_eq!(
        fs::read_to_string(
            app.game(&game.id)
                .profile_state("p1")
                .join("data.old/legacy.sav")
        )
        .unwrap(),
        "changed"
    );
    assert_eq!(
        outcome.backup.expect("a backup").kind,
        SnapshotKind::BeforeRestore
    );
    assert_eq!(list(&app, &game.id).len(), 2);
}

#[test]
fn a_restore_counts_an_added_and_a_removed_file() {
    let (app, game, settings) = fixture("snapshot-preview");
    let store = store(&app, &game.id);
    write(&store.join("slot.sav"), "one");

    let snapshot = create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
    fs::remove_file(store.join("slot.sav")).unwrap();
    write(&store.join("other.sav"), "x");

    let preview = preview(&app, &game, &settings, &snapshot.id).unwrap();
    assert_eq!(preview.added, 1);
    assert_eq!(preview.removed, 1);
    assert_eq!(preview.bytes, 3);
}

#[test]
fn a_tampered_snapshot_is_refused_and_leaves_the_store_alone() {
    let (app, game, settings, store, snapshot) = snapshot_with_file("snapshot-corrupt");
    let scope = SnapshotScope::new(&app, &game.id);
    let saved = set_dir_in(&scope.dir(&snapshot.id), "saves").join("slot.sav");
    write(&saved, "tampered");
    write(&store.join("slot.sav"), "current");

    let error = restore(&app, &game, &settings, &snapshot.id).unwrap_err();
    assert_eq!(error.code, "snapshot.corrupt");
    assert_eq!(
        fs::read_to_string(store.join("slot.sav")).unwrap(),
        "current"
    );
}

#[test]
fn a_restore_puts_back_the_profile_list_and_the_overwrite() {
    let (app, game, settings) = fixture("snapshot-profile");
    let data = app.game(&game.id);
    let mut profiles = crate::context::load_profiles(&data).unwrap();
    let active = profiles.active_mut();
    active.mods.push(crate::domain::profiles::ListedMod {
        id: "local:one".to_string(),
        folder: "one".to_string(),
        enabled: true,
        priority: Some(3),
    });
    active.separators.push(crate::domain::profiles::Separator {
        id: "sep1".to_string(),
        name: "Group".to_string(),
        before: Some("local:one".to_string()),
    });
    data.save_profiles(&profiles).unwrap();
    let overwrite = data.profile_state("p1").join("overwrite");
    write(&overwrite.join("bin/config.ini"), "saved");

    let snapshot = create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
    assert_eq!(snapshot.mods, 1);
    assert_eq!(snapshot.overwrite_files, 1);

    let mut profiles = crate::context::load_profiles(&data).unwrap();
    profiles.active_mut().mods.clear();
    profiles.active_mut().separators.clear();
    data.save_profiles(&profiles).unwrap();
    write(&overwrite.join("bin/config.ini"), "changed");

    let preview = preview(&app, &game, &settings, &snapshot.id).unwrap();
    assert_eq!(preview.mods, 1);
    assert!(!preview.recreates_profile);

    let outcome = restore(&app, &game, &settings, &snapshot.id).unwrap();
    assert!(!outcome.profile_created);
    assert_eq!(
        fs::read_to_string(overwrite.join("bin/config.ini")).unwrap(),
        "saved"
    );
    let profiles = crate::context::load_profiles(&data).unwrap();
    assert_eq!(profiles.active().mods.len(), 1);
    assert_eq!(profiles.active().mods[0].priority, Some(3));
    assert_eq!(profiles.active().separators[0].name, "Group");
}

#[test]
fn a_restore_recreates_a_deleted_profile() {
    let (app, game, settings) = fixture("snapshot-recreate");
    let data = app.game(&game.id);
    let mut profiles = crate::context::load_profiles(&data).unwrap();
    let second = profiles.create("Second", None).unwrap();
    let snapshot = create(&app, &game, &settings, "p1", SnapshotKind::Manual).unwrap();
    profiles.delete("p1").unwrap();
    profiles.switch(&second).unwrap();
    data.save_profiles(&profiles).unwrap();

    let outcome = restore(&app, &game, &settings, &snapshot.id).unwrap();
    assert!(outcome.profile_created);
    assert_eq!(outcome.profile, "p1");
    assert!(outcome.backup.is_none());
    let profiles = crate::context::load_profiles(&data).unwrap();
    assert!(profiles.profile("p1").is_ok());
}

#[test]
fn a_restore_writes_the_whiteouts_and_the_seed_marker_of_a_redirect() {
    let base = crate::test_support::scratch!("snapshot-whiteouts");
    let upper = base.join("upper");
    fs::create_dir_all(&upper).unwrap();
    let mut set = manifest_with("1-redirect", 0).sets.remove(0);
    set.target = Target::Upper;
    set.whiteouts = Some("gone.txt\n".to_string());
    set.whiteouts_journal = Some("later.txt\n".to_string());
    set.seeded = true;
    restore_redirect_state(&upper, &set).unwrap();
    assert_eq!(
        fs::read_to_string(user_data::whiteouts(&upper)).unwrap(),
        "gone.txt\n"
    );
    assert_eq!(
        fs::read_to_string(user_data::whiteouts_journal(&upper)).unwrap(),
        "later.txt\n"
    );
    assert!(user_data::seeded(&upper).is_file());

    set.whiteouts = None;
    set.whiteouts_journal = None;
    set.seeded = false;
    restore_redirect_state(&upper, &set).unwrap();
    assert!(!user_data::whiteouts(&upper).exists());
    assert!(!user_data::whiteouts_journal(&upper).exists());
    assert!(!user_data::seeded(&upper).exists());
}
