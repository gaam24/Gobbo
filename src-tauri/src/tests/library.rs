//! The library on disk: folders changed by hand, rescans, reinstalling, deleting and usage of mods.

use std::collections::BTreeSet;

use super::*;
use crate::context::managed::managed_games;
use crate::deploy::ops::apply_deployment;
use crate::deploy::ops::undo_deployment;
use crate::deploy::remove::delete_mod;
use crate::import::reinstall_mod;
use crate::library::delete_mod_version;
use crate::library::delete_unused_versions;
use crate::library::rescan;
use crate::library::set_mod_note;
use crate::library::set_mod_tags;
use crate::profiles::actions::create_profile;
use crate::profiles::actions::switch_profile;
use crate::settings::managed::check_game_folder;
use crate::status::details::mod_usage;
use crate::status::details::mod_versions;
use crate::status::sync_profiles as status;
use crate::test_support::scratch;

/// Two changes of the index at once, to two families, both keep their edit. Each change reads the index whole and writes
/// it back whole, so without the index lock one of them would write back the index it read before the other saved.
#[test]
fn changes_of_two_families_at_once_keep_both() {
    let f = fixture("changes-at-once");
    f.import_two_mods();
    let data = &f.data;
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for step in 0..20 {
                set_mod_tags(data, SLOT_GAME, "local:moda", vec![format!("tag{step}")]).unwrap();
            }
        });
        scope.spawn(|| {
            for step in 0..20 {
                set_mod_note(data, SLOT_GAME, "local:modb", format!("note{step}")).unwrap();
            }
        });
    });

    let game = test_games::slot();
    let library = load_library(&f.data.game(SLOT_GAME), &game).unwrap();
    assert_eq!(
        library.families["local:moda"].tags,
        BTreeSet::from(["tag19".to_string()])
    );
    assert_eq!(library.families["local:modb"].notes, "note19");
}

/// The status has no problem about the mod in the folder.
fn assert_no_problem_for(problems: &[Message], folder: &str) {
    assert!(
        problems
            .iter()
            .all(|p| p.params.get("folder").is_none_or(|found| found != folder)),
        "{problems:?}"
    );
}

#[test]
fn launch_reads_the_index_and_does_not_walk_the_folders() {
    let f = fixture("no-walk");
    f.import_two_mods();
    // A file added by hand is not visible in the status until the library is scanned.
    fs::write(
        f.data
            .game(SLOT_GAME)
            .library()
            .join("modA/content/extra.ws"),
        "new",
    )
    .unwrap();

    assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods[0].files, 1);
    rescan(&f.data, SLOT_GAME).unwrap();
    assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods[0].files, 2);
}

#[test]
fn a_folder_removed_by_hand_is_reported_not_hidden() {
    let f = fixture("removed");
    f.import_two_mods();
    fs::remove_dir_all(f.data.game(SLOT_GAME).library().join("modB")).unwrap();

    let view = status(&f.data, SLOT_GAME).unwrap();
    assert!(
        view.problems
            .iter()
            .any(|p| p.code == "status.index_folder_missing" && p.params["folder"] == "modB"),
        "{:?}",
        view.problems
    );
}

#[test]
fn a_folder_copied_in_by_hand_appears_after_rescan() {
    let f = fixture("adopt");
    f.import_two_mods();
    let by_hand = f.data.game(SLOT_GAME).library().join("modC/content");
    fs::create_dir_all(&by_hand).unwrap();
    fs::write(by_hand.join("y.ws"), "c").unwrap();

    let before = status(&f.data, SLOT_GAME).unwrap();
    assert!(!before.mods.iter().any(|m| m.folder == "modC"));
    assert!(
        before.problems.iter().any(|p| p
            .params
            .get("folder")
            .is_some_and(|folder| folder == "modC")),
        "{:?}",
        before.problems
    );
    rescan(&f.data, SLOT_GAME).unwrap();
    let after = status(&f.data, SLOT_GAME).unwrap();
    let adopted = after.mods.iter().find(|m| m.folder == "modC").unwrap();
    assert!(!adopted.enabled);
}

#[test]
fn deleting_a_mod_keeps_only_the_archives_of_the_remaining_mods() {
    let f = fixture("delete-mod");
    f.import_two_mods();
    let archives = f.data.game(SLOT_GAME).archives();
    let view = status(&f.data, SLOT_GAME).unwrap();
    let first = view.mods[0].id.clone();
    let second = view.mods[1].id.clone();

    delete_mod(&f.data, SLOT_GAME, &first).unwrap();
    let view = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(view.mods.len(), 1);
    assert_eq!(view.mods[0].id, second);
    assert!(!f.data.game(SLOT_GAME).library().join("modA").exists());
    assert_eq!(fs::read_dir(&archives).unwrap().count(), 1);

    delete_mod(&f.data, SLOT_GAME, &second).unwrap();
    let empty = status(&f.data, SLOT_GAME).unwrap();
    assert!(empty.mods.is_empty());
    assert!(empty.profiles.iter().all(|p| p.mod_count == 0));
    assert_eq!(fs::read_dir(&archives).unwrap().count(), 0);
}

#[test]
fn mod_usage_lists_every_profile_and_the_selected_version() {
    let f = fixture("mod-usage");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let first = f.active_id(SLOT_GAME);
    let id = status(&f.data, SLOT_GAME).unwrap().mods[0].id.clone();
    create_profile(&f.data, SLOT_GAME, "Other", None, false).unwrap();
    let other = f.profile_id(SLOT_GAME, "Other");
    switch_profile(&f.data, SLOT_GAME, &other).unwrap();
    status(&f.data, SLOT_GAME).unwrap();

    let usage = mod_usage(&f.data, SLOT_GAME, &id).unwrap();
    assert_eq!(usage.profiles.len(), 2);
    assert!(
        usage
            .profiles
            .iter()
            .all(|profile| !profile.folder.is_empty())
    );
    assert!(!usage.prepared);
    assert!(
        usage
            .profiles
            .iter()
            .any(|profile| profile.id == first && profile.enabled)
    );
    assert!(
        usage
            .profiles
            .iter()
            .any(|profile| profile.id == other && !profile.enabled)
    );

    switch_profile(&f.data, SLOT_GAME, &first).unwrap();
    apply_deployment(&f.data, SLOT_GAME).unwrap();
    let usage = mod_usage(&f.data, SLOT_GAME, &id).unwrap();
    assert!(usage.prepared);
    assert!(!usage.running);
    undo_deployment(&f.data, SLOT_GAME).unwrap();
}

#[test]
fn a_mod_already_removed_by_hand_is_dropped_from_the_index() {
    let f = fixture("delete-by-hand");
    f.import_two_mods();
    fs::remove_dir_all(f.data.game(SLOT_GAME).library().join("modB")).unwrap();
    assert!(
        status(&f.data, SLOT_GAME)
            .unwrap()
            .problems
            .iter()
            .any(|p| {
                p.params
                    .get("folder")
                    .is_some_and(|folder| folder == "modB")
            })
    );

    delete_mod(&f.data, SLOT_GAME, "local:modb").unwrap();
    let view = status(&f.data, SLOT_GAME).unwrap();
    assert_no_problem_for(&view.problems, "modB");
    assert_eq!(
        view.mods
            .iter()
            .map(|m| m.folder.as_str())
            .collect::<Vec<_>>(),
        ["modA"]
    );
}

#[test]
fn reinstalling_a_mod_unpacks_it_again_and_keeps_its_data() {
    let f = fixture("reinstall-mod");
    f.import_two_mods();
    let library = f.data.game(SLOT_GAME).library();
    set_mod_note(&f.data, SLOT_GAME, "local:moda", "keep me".to_string()).unwrap();
    let files = status(&f.data, SLOT_GAME)
        .unwrap()
        .mods
        .iter()
        .find(|m| m.folder == "modA")
        .unwrap()
        .files;
    fs::write(library.join("modA").join("stray.txt"), "by hand").unwrap();

    reinstall_mod(&f.data, SLOT_GAME, "modA").unwrap();

    assert!(!library.join("modA").join("stray.txt").exists());
    assert!(!library.join(".reinstall-modA").exists());
    let view = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(view.mods.len(), 2);
    let mod_a = view.mods.iter().find(|m| m.folder == "modA").unwrap();
    assert_eq!(mod_a.files, files);
    assert_eq!(mod_a.id, "local:moda");
    let extra = crate::status::details::mod_extras(&f.data, SLOT_GAME, &["modA".to_string()])
        .unwrap()
        .remove(0);
    assert_eq!(extra.notes, "keep me");
    assert!(mod_a.archive_file.is_some());
}

#[test]
fn reinstalling_brings_back_a_folder_removed_by_hand() {
    let f = fixture("reinstall-missing");
    f.import_two_mods();
    fs::remove_dir_all(f.data.game(SLOT_GAME).library().join("modB")).unwrap();

    reinstall_mod(&f.data, SLOT_GAME, "modB").unwrap();

    assert!(f.data.game(SLOT_GAME).library().join("modB").is_dir());
    let view = status(&f.data, SLOT_GAME).unwrap();
    assert_no_problem_for(&view.problems, "modB");
}

#[test]
fn reinstalling_one_mod_of_an_archive_leaves_the_others_alone() {
    let f = fixture("reinstall-pair");
    let zip = f.zip(
        "pair.zip",
        &[
            ("Mods/modA/content/a.ws", "from A"),
            ("Mods/modB/content/b.ws", "from B"),
        ],
    );
    import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    let library = f.data.game(SLOT_GAME).library();
    let folders: Vec<String> = status(&f.data, SLOT_GAME)
        .unwrap()
        .mods
        .iter()
        .map(|m| m.folder.clone())
        .collect();
    let folder_a = folders.iter().find(|f| f.contains("modA")).unwrap().clone();
    let folder_b = folders.iter().find(|f| f.contains("modB")).unwrap().clone();
    fs::write(library.join(&folder_a).join("stray.txt"), "by hand").unwrap();
    fs::write(library.join(&folder_b).join("mine.txt"), "kept").unwrap();

    reinstall_mod(&f.data, SLOT_GAME, &folder_a).unwrap();

    assert!(!library.join(&folder_a).join("stray.txt").exists());
    assert!(library.join(&folder_a).join("content/a.ws").exists());
    assert!(library.join(&folder_b).join("mine.txt").exists());
    assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods.len(), 2);
}

#[test]
fn a_mod_whose_archive_copy_is_gone_is_left_as_it_is() {
    let f = fixture("reinstall-no-archive");
    f.import_two_mods();
    let library = f.data.game(SLOT_GAME).library();
    for entry in fs::read_dir(f.data.game(SLOT_GAME).archives()).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::write(library.join("modA").join("stray.txt"), "by hand").unwrap();

    assert_eq!(
        reinstall_mod(&f.data, SLOT_GAME, "modA").unwrap_err().code,
        "library.archive_missing"
    );
    assert!(library.join("modA").join("stray.txt").exists());
}

#[test]
fn delete_refuses_paths_and_unknown_mods() {
    let f = fixture("delete-unknown");
    f.import_two_mods();
    assert!(delete_mod(&f.data, SLOT_GAME, "../modA").is_err());
    assert_eq!(
        delete_mod(&f.data, SLOT_GAME, "modZ").unwrap_err().code,
        "library.mod_missing"
    );
}

#[test]
fn rescan_clears_archive_copies_that_no_mod_uses() {
    let f = fixture("clean-rescan");
    f.import_two_mods();
    // A status writes the profile list, as it happens at every start of the app.
    status(&f.data, SLOT_GAME).unwrap();
    let archives = f.data.game(SLOT_GAME).archives();
    fs::write(archives.join(format!("{}.zip", "0".repeat(64))), "orphan").unwrap();
    fs::write(archives.join("half.zip.tmp"), "half").unwrap();

    rescan(&f.data, SLOT_GAME).unwrap();
    assert_eq!(fs::read_dir(&archives).unwrap().count(), 2);

    // Mods deleted by hand take their archive with them at the next scan.
    fs::remove_dir_all(f.data.game(SLOT_GAME).library().join("modA")).unwrap();
    fs::remove_dir_all(f.data.game(SLOT_GAME).library().join("modB")).unwrap();
    rescan(&f.data, SLOT_GAME).unwrap();
    assert_eq!(fs::read_dir(&archives).unwrap().count(), 0);
    let view = status(&f.data, SLOT_GAME).unwrap();
    assert!(view.mods.is_empty());
    assert!(view.problems.is_empty(), "{:?}", view.problems);
}

#[test]
fn the_library_folder_exists_from_the_first_status() {
    let f = fixture("library-folder");
    status(&f.data, SLOT_GAME).unwrap();
    assert!(f.data.game(SLOT_GAME).library().is_dir());
}

#[test]
fn checking_a_folder_names_its_marker_and_adds_nothing() {
    let base = scratch!("app-check");
    let data = Data::new(base.join("data"));
    let empty = base.join("pusty");
    fs::create_dir_all(&empty).unwrap();
    assert!(check_game_folder(SLOT_GAME, &empty.display().to_string()).is_err());
    assert!(check_game_folder(SLOT_GAME, "  ").is_err());

    let game = slot_game_folder(&base);
    assert_eq!(
        check_game_folder(SLOT_GAME, &game.display().to_string()).unwrap(),
        Some("bin/game.exe".to_string())
    );
    // Checking only looks: the game is not on the list.
    assert!(managed_games(&data).unwrap().is_empty());
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn deleting_a_used_version_switches_every_profile_to_the_replacement() {
    let f = mirror_fixture("delete-version");
    let (old, new) = two_mod_versions(&f);
    import(&f.data, MIRROR_GAME, &old, None, &mut |_, _| {}).unwrap();
    status(&f.data, MIRROR_GAME).unwrap();
    import(&f.data, MIRROR_GAME, &new, None, &mut |_, _| {}).unwrap();
    let view = status(&f.data, MIRROR_GAME).unwrap();
    let family = view.mods[0].id.clone();
    let versions = mod_versions(&f.data, MIRROR_GAME, &view.mods[0].folder).unwrap();
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

    assert_eq!(
        delete_mod_version(&f.data, MIRROR_GAME, &old_folder, None, |_, _| Ok(()))
            .unwrap_err()
            .code,
        "library.version_in_use"
    );
    delete_mod_version(
        &f.data,
        MIRROR_GAME,
        &old_folder,
        Some(&new_folder),
        |_, _| Ok(()),
    )
    .unwrap();
    let view = status(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(view.mods[0].folder, new_folder);
    assert_eq!(view.mods[0].version_count, 1);
    assert!(!f.data.game(MIRROR_GAME).library().join(old_folder).exists());
    let _ = family;
}

#[test]
fn opening_is_limited_to_gobbo_and_the_game_folders() {
    let f = fixture("may-open");
    fs::create_dir_all(f.data.game(SLOT_GAME).library()).unwrap();
    assert!(may_open(&f.data, Some(SLOT_GAME), &f.game_root));
    assert!(may_open(
        &f.data,
        Some(SLOT_GAME),
        &f.data.game(SLOT_GAME).library()
    ));
    assert!(may_open(&f.data, None, &f.base.join("data")));

    let outside = scratch!("outside");
    assert!(!may_open(&f.data, Some(SLOT_GAME), &outside));

    // A game id is not a path: a traversal id must not add a folder outside the data root. The path is not allowed
    // without a game, so only the bad id could have added it.
    let escape = f.base.join("escape");
    fs::create_dir_all(&escape).unwrap();
    assert!(!may_open(&f.data, None, &escape));
    assert!(!may_open(&f.data, Some("../../escape"), &escape));
}

#[test]
fn deleting_unused_versions_keeps_the_selected_one() {
    let f = mirror_fixture("unused-versions");
    for (name, text) in [
        (
            "Remove Mods Menu 35686 1.2.0 2026-10-23T17-21Z Q6xF6llIa.zip",
            "old",
        ),
        (
            "Remove Mods Menu 35686 1.2.1 2026-11-01T09-00Z Abc123xyz.zip",
            "new",
        ),
    ] {
        let zip = f.zip(name, &[("r6/scripts/RemoveModsMenu/a.reds", text)]);
        import(&f.data, MIRROR_GAME, &zip, None, &mut |_, _| {}).unwrap();
    }
    let before = status(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(before.mods[0].version_count, 2);
    assert_eq!(
        delete_unused_versions(&f.data, MIRROR_GAME, |_, _, _| Ok(())).unwrap(),
        1
    );
    let after = status(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(after.mods[0].version_count, 1);
    assert_eq!(after.mods[0].folder, before.mods[0].folder);
}
