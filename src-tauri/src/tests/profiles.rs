//! Profiles: their lists and flags, saving, copying, renaming and deleting them.

use super::*;
use crate::context::profile::data as profile_data;
use crate::deploy::ops::apply_deployment;
use crate::deploy::ops::undo_deployment;
use crate::profiles::actions::copy_profile_data;
use crate::profiles::actions::create_profile;
use crate::profiles::actions::delete_profile;
use crate::profiles::actions::move_profile_user_data;
use crate::profiles::actions::rename_profile;
use crate::profiles::actions::save_profile;
use crate::profiles::actions::set_load_order;
use crate::profiles::actions::set_profile_user_data;
use crate::profiles::actions::switch_profile;
#[cfg(windows)]
use crate::profiles::actions::user_data_size;
use crate::profiles::compare::compare_profiles;
use crate::status::details::mod_versions;
use crate::status::sync_profiles as status;
use crate::test_support::import;

/// A game with one user-data set outside its folder, at `<settings>/user`.
const USER_DATA_GAME: &str = test_games::USER_DATA_ID;
#[cfg(windows)]
const USER_DATA_COPY_GAME: &str = test_games::USER_DATA_COPY_ID;

/// Serializes the tests that change the process-wide running flag: it answers for the game id, not for one fixture.
static USER_DATA_MOVE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn two_versions_of_a_nexus_mod_are_both_in_the_profile() {
    let f = mirror_fixture("nexus-versions");
    let older = f.zip(
        "Example Cleanup Mod 7001 1.2.0 2026-10-23T17-21Z example-uploader.zip",
        &[("r6/scripts/ExampleCleanupMod/ExampleCleanupMod.reds", "x")],
    );
    import(&f.data, MIRROR_GAME, &older, None, &mut |_, _| {}).unwrap();
    let newer = f.zip(
        "Example Cleanup Mod 7001 1.2.1 2026-11-01T09-00Z Abc123xyz.zip",
        &[("r6/scripts/ExampleCleanupMod/ExampleCleanupMod.reds", "y")],
    );
    import(&f.data, MIRROR_GAME, &newer, None, &mut |_, _| {}).unwrap();

    let status = status(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(status.mods.len(), 1, "both versions belong to one mod");
    let listed = &status.mods[0];
    assert!(!listed.enabled);
    assert_eq!(listed.version_count, 2);
    let versions = mod_versions(&f.data, MIRROR_GAME, &listed.folder).unwrap();
    assert!(
        versions
            .iter()
            .any(|version| version.version.as_deref() == Some("1.2.0"))
    );
    assert!(
        versions
            .iter()
            .any(|version| version.version.as_deref() == Some("1.2.1"))
    );
}

#[test]
fn saved_order_and_enabled_flags_are_kept() {
    let f = fixture("order");
    f.import_two_mods();
    let list = rows(&[("modB", true), ("modA", false)]);
    save_profile(&f.data, SLOT_GAME, &list).unwrap();

    let status = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(status.mods[0].folder, "modB");
    assert_eq!(status.mods[0].priority, 1);
    assert!(!status.mods[1].enabled);
}

#[test]
fn a_new_separator_above_the_others_keeps_their_ids() {
    let f = fixture("separator-ids");
    f.import_two_mods();
    let separator = |id: &str, name: &str| separator_row(id, name);
    let mod_a = || row("modA", true);
    let mod_b = || row("modB", true);

    save_profile(
        &f.data,
        SLOT_GAME,
        &[
            separator("", "First"),
            mod_a(),
            separator("", "Second"),
            mod_b(),
        ],
    )
    .unwrap();
    let before = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(
        before
            .separators
            .iter()
            .map(|separator| separator.id.as_str())
            .collect::<Vec<_>>(),
        ["sep1", "sep2"]
    );

    // A separator inserted at the top is claimed before the two that already exist, so their ids stay.
    save_profile(
        &f.data,
        SLOT_GAME,
        &[
            separator("", "New"),
            separator("sep1", "First"),
            mod_a(),
            separator("sep2", "Second"),
            mod_b(),
        ],
    )
    .unwrap();
    let after = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(
        after
            .separators
            .iter()
            .map(|separator| separator.id.as_str())
            .collect::<Vec<_>>(),
        ["sep3", "sep1", "sep2"]
    );
}

#[test]
fn only_the_mods_in_the_load_order_reach_the_settings_file() {
    let f = fixture("load-order");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let family = |folder: &str| {
        status(&f.data, SLOT_GAME)
            .unwrap()
            .mods
            .iter()
            .find(|m| m.folder == folder)
            .unwrap()
            .id
            .clone()
    };
    let (family_a, family_b) = (family("modA"), family("modB"));
    let load = |folder: &str| {
        status(&f.data, SLOT_GAME)
            .unwrap()
            .mods
            .iter()
            .find(|m| m.folder == folder)
            .unwrap()
            .load_priority
    };

    // Only modB is in the order, so only modB gets a number. The game orders modA itself.
    set_load_order(&f.data, SLOT_GAME, std::slice::from_ref(&family_b)).unwrap();
    assert_eq!(load("modB"), Some(1));
    assert_eq!(load("modA"), None);
    // The list order is not the load order: modA still comes first in the list.
    assert_eq!(status(&f.data, SLOT_GAME).unwrap().mods[0].folder, "modA");

    // A longer order numbers its mods in the order the user gave them.
    set_load_order(&f.data, SLOT_GAME, &[family_b.clone(), family_a]).unwrap();
    assert_eq!(load("modB"), Some(1));
    assert_eq!(load("modA"), Some(2));

    // An order that names a mod the profile does not have changes nothing.
    let unknown = set_load_order(&f.data, SLOT_GAME, &["local:nope".to_string()]).unwrap_err();
    assert_eq!(unknown.code, "profile.load_order_unknown");
    assert_eq!(load("modA"), Some(2));

    set_load_order(&f.data, SLOT_GAME, std::slice::from_ref(&family_b)).unwrap();
    apply_deployment(&f.data, SLOT_GAME).unwrap();
    let settings = fs::read_to_string(f.settings_dir.join("mods.settings")).unwrap();
    assert!(
        settings.contains("[modB]\nEnabled=1\nPriority=1\n"),
        "{settings}"
    );
    assert!(!settings.contains("[modA]"), "{settings}");
    undo_deployment(&f.data, SLOT_GAME).unwrap();

    set_load_order(&f.data, SLOT_GAME, &[]).unwrap();
    assert_eq!(load("modB"), None);
}

#[test]
fn a_mod_the_game_reads_no_order_for_cannot_be_numbered() {
    let f = fixture("load-order-orderable");
    f.import_two_mods();
    // Every file of this mod keeps its game path, so the mod has no folder of its own for the game to read an order from.
    let zip = f.zip("mirrored.zip", &[("DLC/dlc-only/blob0.bundle", "mirrored")]);
    import(&f.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
    f.enable_all(SLOT_GAME);
    let listed = status(&f.data, SLOT_GAME).unwrap().mods;
    let id_of = |folder: &str| {
        listed
            .iter()
            .find(|m| m.folder == folder)
            .unwrap()
            .id
            .clone()
    };
    let (family_a, family_b) = (id_of("modA"), id_of("modB"));
    let mirrored = listed.iter().find(|m| !m.orderable).unwrap().id.clone();
    assert!(
        listed
            .iter()
            .find(|m| m.folder == "modA")
            .unwrap()
            .orderable
    );
    assert!(
        listed
            .iter()
            .find(|m| m.folder == "modB")
            .unwrap()
            .orderable
    );

    let error = set_load_order(&f.data, SLOT_GAME, std::slice::from_ref(&mirrored)).unwrap_err();
    assert_eq!(error.code, "profile.load_order_not_orderable");
    // A refused order changes nothing, even when it also names a mod the game can order.
    let error = set_load_order(&f.data, SLOT_GAME, &[family_b.clone(), mirrored]).unwrap_err();
    assert_eq!(error.code, "profile.load_order_not_orderable");
    assert!(
        status(&f.data, SLOT_GAME)
            .unwrap()
            .mods
            .iter()
            .all(|m| m.load_priority.is_none())
    );

    set_load_order(&f.data, SLOT_GAME, &[family_b, family_a]).unwrap();
    let after = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(
        after
            .mods
            .iter()
            .find(|m| m.folder == "modB")
            .unwrap()
            .load_priority,
        Some(1)
    );
}

#[test]
fn a_load_order_change_waits_for_a_deployment_and_then_applies() {
    let f = fixture("load-order-draft");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    apply_deployment(&f.data, SLOT_GAME).unwrap();
    let family_b = status(&f.data, SLOT_GAME)
        .unwrap()
        .mods
        .iter()
        .find(|m| m.folder == "modB")
        .unwrap()
        .id
        .clone();

    // The list is the same, so only the priority changed: the deployed settings file must still be written again.
    set_load_order(&f.data, SLOT_GAME, std::slice::from_ref(&family_b)).unwrap();
    assert_eq!(status(&f.data, SLOT_GAME).unwrap().draft_changes, 1);
    apply_deployment(&f.data, SLOT_GAME).unwrap();

    assert_eq!(status(&f.data, SLOT_GAME).unwrap().draft_changes, 0);
    let settings = fs::read_to_string(f.settings_dir.join("mods.settings")).unwrap();
    assert!(
        settings.contains("[modB]\nEnabled=1\nPriority=1\n"),
        "{settings}"
    );
    undo_deployment(&f.data, SLOT_GAME).unwrap();
}

/// The first file called `name` below `dir`, searching every folder.
fn find_named(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();

        if path.is_dir() {
            if let Some(found) = find_named(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|file| file == name) {
            return Some(path);
        }
    }

    None
}

#[test]
fn an_order_file_in_the_deployment_root_is_written_into_the_layer_not_the_game() {
    let game = test_games::UE4SS_ORDER_ID;
    let f = fixture("ue4ss-order");
    save_settings(&f.data, game, f.settings()).unwrap();
    for (name, folder, text) in [
        ("modA.zip", "modA", "from A"),
        ("modB.zip", "modB", "from B"),
    ] {
        let entry = format!("Mods/{folder}/content/x.ws");
        let zip = f.zip(name, &[(entry.as_str(), text)]);
        import(&f.data, game, &zip, None, &mut |_, _| {}).unwrap();
    }
    f.enable_all(game);
    let family_b = status(&f.data, game)
        .unwrap()
        .mods
        .iter()
        .find(|m| m.folder == "modB")
        .unwrap()
        .id
        .clone();

    set_load_order(&f.data, game, &[family_b]).unwrap();
    apply_deployment(&f.data, game).unwrap();

    // Only modB is in the order, so the list names only modB, and it is written where the overlay serves it.
    let list = find_named(&f.base.join("data"), "mods.txt").expect("the layer holds the list");
    assert_eq!(fs::read_to_string(&list).unwrap(), "modB : 1\r\n");
    assert!(list.ends_with(std::path::Path::new("Mods").join("mods.txt")));
    // The game folder keeps no list of its own, and the library copy of the mods is untouched.
    assert!(!f.game_root.join("Mods").join("mods.txt").exists());
}

#[test]
fn a_mod_saved_out_of_the_profile_comes_back_disabled() {
    let f = fixture("unlisted");
    f.import_two_mods();
    save_profile(&f.data, SLOT_GAME, &[row("modA", true)]).unwrap();

    // Every profile lists every mod of the library, so the removed one comes back at the end, disabled.
    let status = status(&f.data, SLOT_GAME).unwrap();
    let listed: Vec<(&str, bool)> = status
        .mods
        .iter()
        .map(|m| (m.folder.as_str(), m.enabled))
        .collect();
    assert_eq!(listed, [("modA", true), ("modB", false)]);
}

#[test]
fn every_profile_keeps_its_own_list_and_flags() {
    let f = fixture("profiles");
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let (default, light) = f.default_and_light();
    // A new profile does not change the active one.
    assert_eq!(f.active_id(SLOT_GAME), default);
    assert!(
        status(&f.data, SLOT_GAME)
            .unwrap()
            .mods
            .iter()
            .all(|m| m.enabled)
    );

    // A fresh profile lists every mod of the library, disabled.
    switch_profile(&f.data, SLOT_GAME, &light).unwrap();
    let fresh = status(&f.data, SLOT_GAME).unwrap();
    let light_mods: Vec<(&str, bool)> = fresh
        .mods
        .iter()
        .map(|m| (m.folder.as_str(), m.enabled))
        .collect();
    assert_eq!(light_mods, [("modA", false), ("modB", false)]);

    let rows = [row("modB", true)];
    save_profile(&f.data, SLOT_GAME, &rows).unwrap();
    switch_profile(&f.data, SLOT_GAME, &default).unwrap();
    let back = status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(back.mods.len(), 2);
    assert!(back.mods.iter().all(|m| m.enabled));

    let counts: Vec<(&str, u32, u32)> = back
        .profiles
        .iter()
        .map(|p| (p.name.as_str(), p.mod_count, p.enabled_count))
        .collect();
    assert_eq!(counts, [("Default", 2, 2), ("Light", 2, 1)]);
}

/// Two profiles after the light one keeps only `modB`.
fn light_with_only_b(f: &Fixture) -> (String, String) {
    f.import_two_mods();
    f.enable_all(SLOT_GAME);
    let (default, light) = f.default_and_light();
    switch_profile(&f.data, SLOT_GAME, &light).unwrap();
    save_profile(&f.data, SLOT_GAME, &[row("modB", true)]).unwrap();
    (default, light)
}

#[test]
fn comparing_profiles_lists_only_what_differs() {
    let f = fixture("compare-profiles");
    let (default, light) = light_with_only_b(&f);

    let compare = compare_profiles(&f.data, SLOT_GAME, &default, &light).unwrap();
    assert_eq!(compare.first_name, "Default");
    assert_eq!(compare.second_name, "Light");
    assert_eq!(compare.same, 1);
    let differing: Vec<(&str, bool, bool)> = compare
        .rows
        .iter()
        .map(|row| {
            (
                row.name.as_str(),
                row.first.as_ref().is_some_and(|side| side.enabled),
                row.second.as_ref().is_some_and(|side| side.enabled),
            )
        })
        .collect();
    assert_eq!(differing, [("modA", true, false)]);
}

#[test]
fn deleting_the_active_profile_falls_back_to_the_first_one() {
    let f = fixture("delete-profile");
    f.import_two_mods();
    let default = f.active_id(SLOT_GAME);
    create_profile(&f.data, SLOT_GAME, "Tymczasowy", None, false).unwrap();
    let temp = f.profile_id(SLOT_GAME, "Tymczasowy");
    switch_profile(&f.data, SLOT_GAME, &temp).unwrap();

    delete_profile(&f.data, SLOT_GAME, &temp).unwrap();
    assert_eq!(f.active_id(SLOT_GAME), default);
    assert!(delete_profile(&f.data, SLOT_GAME, &default).is_err());
}

#[test]
fn duplicating_with_data_copies_the_legacy_store_and_the_overwrite() {
    let f = fixture("copy-profile-data");
    let saves = f.base.join("saves");
    crate::test_support::write(&saves.join("slot.sav"), "one");
    let settings = Settings {
        profile_data: vec![saves.display().to_string()],
        ..f.settings()
    };
    f.save_settings(SLOT_GAME, settings.clone()).unwrap();
    let game = test_games::slot();
    let data = f.data.game(SLOT_GAME);
    let source = f.active_id(SLOT_GAME);
    profile_data::switch(&data, &game, &settings, &source).unwrap();
    crate::test_support::write(&saves.join("slot.sav"), "source");
    let overwrite = data
        .profile_state(&source)
        .join("overwrite/game/config.ini");
    crate::test_support::write(&overwrite, "saved");

    create_profile(&f.data, SLOT_GAME, "Kopia", Some(&source), true).unwrap();
    let target = f.profile_id(SLOT_GAME, "Kopia");
    assert_eq!(
        fs::read_to_string(
            data.profile_state(&target)
                .join("overwrite/game/config.ini")
        )
        .unwrap(),
        "saved"
    );

    profile_data::switch(&data, &game, &settings, &target).unwrap();
    assert_eq!(
        fs::read_to_string(saves.join("slot.sav")).unwrap(),
        "source"
    );
    assert_eq!(
        fs::read_to_string(saves.join("slot.sav")).unwrap(),
        "source",
        "the source profile is untouched"
    );
}

#[test]
fn copying_profile_data_keeps_a_before_copy_snapshot_of_the_target() {
    let f = fixture("copy-profile-existing");
    let saves = f.base.join("saves");
    crate::test_support::write(&saves.join("slot.sav"), "one");
    let settings = Settings {
        profile_data: vec![saves.display().to_string()],
        ..f.settings()
    };
    f.save_settings(SLOT_GAME, settings.clone()).unwrap();
    let game = test_games::slot();
    let data = f.data.game(SLOT_GAME);
    let source = f.active_id(SLOT_GAME);
    create_profile(&f.data, SLOT_GAME, "Target", None, false).unwrap();
    let target = f.profile_id(SLOT_GAME, "Target");
    profile_data::switch(&data, &game, &settings, &source).unwrap();
    crate::test_support::write(&saves.join("slot.sav"), "source");

    copy_profile_data(&f.data, SLOT_GAME, &source, &target).unwrap();
    assert_eq!(
        crate::snapshots::list(&f.data, SLOT_GAME)
            .iter()
            .filter(|snapshot| snapshot.kind == crate::snapshots::SnapshotKind::BeforeCopy)
            .count(),
        1
    );
    profile_data::switch(&data, &game, &settings, &target).unwrap();
    assert_eq!(
        fs::read_to_string(saves.join("slot.sav")).unwrap(),
        "source"
    );
    assert_eq!(
        copy_profile_data(&f.data, SLOT_GAME, &source, &source)
            .unwrap_err()
            .code,
        "profiles.copy_same"
    );
}

#[test]
fn a_copy_takes_the_list_and_rename_checks_the_name() {
    let f = fixture("copy-profile");
    f.import_two_mods();
    let default = f.active_id(SLOT_GAME);
    create_profile(&f.data, SLOT_GAME, "Kopia", Some(default.as_str()), false).unwrap();
    let copy = f.profile_id(SLOT_GAME, "Kopia");
    assert_eq!(status(&f.data, SLOT_GAME).unwrap().profiles[1].mod_count, 2);

    assert!(create_profile(&f.data, SLOT_GAME, "kopia", None, false).is_err());
    rename_profile(&f.data, SLOT_GAME, &copy, "Mocna").unwrap();
    assert!(
        status(&f.data, SLOT_GAME)
            .unwrap()
            .profiles
            .iter()
            .any(|p| p.name == "Mocna")
    );
}

#[test]
fn a_mod_only_the_second_profile_lists_is_a_row_without_a_first_side() {
    let f = fixture("compare-second-only");
    let (default, light) = light_with_only_b(&f);

    let compare = compare_profiles(&f.data, SLOT_GAME, &light, &default).unwrap();
    assert_eq!(compare.same, 1);
    assert_eq!(compare.rows.len(), 1);
    assert_eq!(compare.rows[0].id, "local:moda");
    assert!(compare.rows[0].first.is_none());
    assert!(compare.rows[0].second.is_some());
}

#[test]
fn save_profile_refuses_repeats_and_paths_outside_the_library() {
    let f = fixture("validate");
    f.import_two_mods();
    let twice = [row("modA", true), row("modA", false)];
    assert_eq!(
        save_profile(&f.data, SLOT_GAME, &twice).unwrap_err().code,
        "profile.duplicate_mod"
    );
    let escape = [row("Mods/../modA", true)];
    assert!(save_profile(&f.data, SLOT_GAME, &escape).is_err());
}

#[test]
fn a_status_read_does_not_write_profiles_but_the_sync_does() {
    let f = fixture("status-query-purity");
    f.import_two_mods();
    let data = f.data.game(SLOT_GAME);
    let path = data.profiles_file();
    let before = fs::read(&path).ok();

    let seen = crate::status::status(&f.data, SLOT_GAME).unwrap();
    assert_eq!(seen.mods.len(), 2);
    assert_eq!(
        fs::read(&path).ok(),
        before,
        "a read must not write profiles.json"
    );

    let synced = crate::status::sync_profiles(&f.data, SLOT_GAME).unwrap();
    assert_eq!(synced.mods.len(), 2);
    assert!(fs::read(&path).is_ok(), "the sync saves the profile list");
}

/// A fixture for the user-data game: its folder, its settings folder and a saved settings file.
fn user_data_fixture(name: &str) -> Fixture {
    test_games::install();
    let base = scratch!(name);
    let game_root = base.join("game");
    let settings_dir = base.join("settings");
    fs::create_dir_all(game_root.join("Mods")).unwrap();
    fs::create_dir_all(&settings_dir).unwrap();
    let fixture = Fixture {
        data: Data::new(base.join("data")),
        game_root,
        settings_dir,
        base,
    };
    save_settings(&fixture.data, USER_DATA_GAME, fixture.settings()).unwrap();
    fixture
}

/// The user-data fixture for a definition whose set is copied around a launch instead of redirected.
#[cfg(windows)]
fn user_data_copy_fixture(name: &str) -> Fixture {
    test_games::install();
    let base = scratch!(name);
    let game_root = base.join("game");
    let settings_dir = base.join("settings");
    fs::create_dir_all(game_root.join("Mods")).unwrap();
    fs::create_dir_all(&settings_dir).unwrap();
    let fixture = Fixture {
        data: Data::new(base.join("data")),
        game_root,
        settings_dir,
        base,
    };
    save_settings(&fixture.data, USER_DATA_COPY_GAME, fixture.settings()).unwrap();
    fixture
}

/// Isolates the fixture's user-data set for the active profile and creates its upper layer. Returns the profile id,
/// the real folder and the upper layer.
fn isolated_user_data(f: &Fixture) -> (String, PathBuf, PathBuf) {
    let profile = f.active_id(USER_DATA_GAME);
    set_profile_user_data(&f.data, USER_DATA_GAME, &profile, "documents", true).unwrap();
    let state = f.data.game(USER_DATA_GAME).profile_state(&profile);
    let real = f.settings_dir.join("user");
    let upper = state.join("userdata/documents/upper");
    fs::create_dir_all(&upper).unwrap();
    (profile, real, upper)
}

/// Whether the profile still keeps the set isolated.
fn is_isolated(f: &Fixture, profile: &str) -> bool {
    load_profiles(&f.data.game(USER_DATA_GAME))
        .unwrap()
        .get(profile)
        .unwrap()
        .user_data_isolation("documents")
        .is_isolated()
}

/// The single `upper.moved-<unix>` folder next to the upper layer.
#[cfg(windows)]
fn moved_layer(upper: &Path) -> PathBuf {
    let parent = upper.parent().unwrap();
    let moved: Vec<_> = fs::read_dir(parent)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().is_dir()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("upper.moved-")
        })
        .map(|entry| entry.path())
        .collect();
    assert_eq!(moved.len(), 1, "one recovery copy of the layer");
    moved[0].clone()
}

/// The single `removed/<unix>` folder of the set.
#[cfg(windows)]
fn removed_layer(upper: &Path) -> PathBuf {
    let removed = upper.with_file_name("removed");
    let attempts: Vec<_> = fs::read_dir(&removed)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    assert_eq!(attempts.len(), 1, "one attempt keeps its deleted paths");
    attempts[0].clone()
}

#[cfg(windows)]
#[test]
fn moving_data_back_overwrites_changed_files_and_keeps_untouched_ones() {
    let _guard = USER_DATA_MOVE.lock().unwrap();
    let f = user_data_fixture("user-data-move");
    let (profile, real, upper) = isolated_user_data(&f);
    fs::create_dir_all(real.join("saves")).unwrap();
    fs::write(real.join("saves/changed.sav"), "real").unwrap();
    fs::write(real.join("saves/kept.sav"), "kept").unwrap();
    fs::create_dir_all(upper.join("saves")).unwrap();
    fs::write(upper.join("saves/changed.sav"), "profile").unwrap();
    fs::write(upper.join("saves/new.sav"), "new").unwrap();
    fs::write(upper.parent().unwrap().join("seeded"), b"").unwrap();
    fs::write(upper.with_extension("whiteouts"), "old\\gone.txt\n").unwrap();

    move_profile_user_data(&f.data, USER_DATA_GAME, &profile, "documents").unwrap();

    assert_eq!(
        fs::read_to_string(real.join("saves/changed.sav")).unwrap(),
        "profile"
    );
    assert_eq!(
        fs::read_to_string(real.join("saves/kept.sav")).unwrap(),
        "kept"
    );
    assert_eq!(
        fs::read_to_string(real.join("saves/new.sav")).unwrap(),
        "new"
    );
    assert!(!upper.exists(), "the layer becomes the recovery copy");
    assert!(!upper.with_extension("whiteouts").exists());
    assert!(
        !upper.parent().unwrap().join("seeded").exists(),
        "the copy has no seed marker"
    );
    let moved = moved_layer(&upper);
    assert_eq!(
        fs::read_to_string(moved.join("saves/changed.sav")).unwrap(),
        "profile"
    );
    assert!(!is_isolated(&f, &profile), "the set is shared again");
}

#[cfg(windows)]
#[test]
fn a_whiteout_moves_the_deleted_real_path_aside_before_the_merge() {
    let _guard = USER_DATA_MOVE.lock().unwrap();
    let f = user_data_fixture("user-data-whiteout");
    let (profile, real, upper) = isolated_user_data(&f);
    fs::create_dir_all(real.join("gone-folder")).unwrap();
    fs::write(real.join("gone-folder/inside.txt"), "deleted").unwrap();
    fs::write(real.join("gone.txt"), "deleted").unwrap();
    fs::write(real.join("journal.txt"), "deleted too").unwrap();
    fs::write(real.join("kept.txt"), "kept").unwrap();
    fs::write(upper.join("seen.txt"), "profile").unwrap();
    fs::write(upper.with_extension("whiteouts"), "gone.txt\ngone-folder\n").unwrap();
    fs::write(upper.with_extension("whiteouts.journal"), "journal.txt\n").unwrap();

    move_profile_user_data(&f.data, USER_DATA_GAME, &profile, "documents").unwrap();

    assert!(!real.join("gone.txt").exists());
    assert!(!real.join("gone-folder").exists());
    assert!(!real.join("journal.txt").exists());
    assert_eq!(fs::read_to_string(real.join("kept.txt")).unwrap(), "kept");
    assert_eq!(
        fs::read_to_string(real.join("seen.txt")).unwrap(),
        "profile"
    );
    let removed = removed_layer(&upper);
    assert_eq!(
        fs::read_to_string(removed.join("gone.txt")).unwrap(),
        "deleted"
    );
    assert_eq!(
        fs::read_to_string(removed.join("gone-folder/inside.txt")).unwrap(),
        "deleted"
    );
    assert_eq!(
        fs::read_to_string(removed.join("journal.txt")).unwrap(),
        "deleted too"
    );
}

#[cfg(windows)]
#[test]
fn a_failed_move_leaves_the_isolation_and_the_layer_intact() {
    let _guard = USER_DATA_MOVE.lock().unwrap();
    let f = user_data_fixture("user-data-move-conflict");
    let (profile, real, upper) = isolated_user_data(&f);
    fs::create_dir_all(real.join("conflict")).unwrap();
    fs::write(real.join("conflict/inside.txt"), "real").unwrap();
    fs::write(upper.join("conflict"), "profile").unwrap();

    let error = move_profile_user_data(&f.data, USER_DATA_GAME, &profile, "documents").unwrap_err();

    assert_eq!(error.code, "user_data.merge_conflict");
    assert!(is_isolated(&f, &profile));
    assert_eq!(
        fs::read_to_string(upper.join("conflict")).unwrap(),
        "profile"
    );
    assert!(upper.join("conflict").is_file());
    assert!(real.join("conflict").is_dir());
    assert_eq!(
        fs::read_to_string(real.join("conflict/inside.txt")).unwrap(),
        "real"
    );
    let moved = fs::read_dir(upper.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("upper.moved-")
        });
    assert!(!moved, "a failed move keeps the layer in place");
}

#[cfg(windows)]
#[test]
fn a_move_back_is_refused_while_the_game_runs() {
    let _guard = USER_DATA_MOVE.lock().unwrap();
    let f = user_data_fixture("user-data-move-running");
    let (profile, _, upper) = isolated_user_data(&f);
    let running = crate::context::running::mark_started(USER_DATA_GAME);

    let error = move_profile_user_data(&f.data, USER_DATA_GAME, &profile, "documents").unwrap_err();
    running.store(false, std::sync::atomic::Ordering::Release);

    assert_eq!(error.code, "deploy.game_running");
    assert!(upper.is_dir());
    assert!(is_isolated(&f, &profile));
}

#[cfg(windows)]
#[test]
fn a_move_back_needs_an_isolated_redirect_set() {
    let _guard = USER_DATA_MOVE.lock().unwrap();
    let shared = user_data_fixture("user-data-move-shared");
    let profile = shared.active_id(USER_DATA_GAME);
    let error =
        move_profile_user_data(&shared.data, USER_DATA_GAME, &profile, "documents").unwrap_err();
    assert_eq!(error.code, "user_data.not_isolated");

    let copy = user_data_copy_fixture("user-data-move-copy");
    let profile = copy.active_id(USER_DATA_COPY_GAME);
    set_profile_user_data(&copy.data, USER_DATA_COPY_GAME, &profile, "documents", true).unwrap();
    let error =
        move_profile_user_data(&copy.data, USER_DATA_COPY_GAME, &profile, "documents").unwrap_err();
    assert_eq!(error.code, "user_data.not_redirect");
}

#[cfg(windows)]
#[test]
fn the_user_data_size_counts_only_the_profile_layer() {
    let _guard = USER_DATA_MOVE.lock().unwrap();
    let f = user_data_fixture("user-data-size");
    let (profile, real, upper) = isolated_user_data(&f);
    fs::create_dir_all(&real).unwrap();
    fs::write(real.join("big.bin"), vec![0_u8; 10_000]).unwrap();
    fs::write(upper.join("small.bin"), vec![0_u8; 500]).unwrap();

    assert_eq!(
        user_data_size(&f.data, USER_DATA_GAME, &profile, "documents").unwrap(),
        1,
        "10 kB in the real folder is not the profile's size"
    );

    set_profile_user_data(&f.data, USER_DATA_GAME, &profile, "documents", false).unwrap();
    assert_eq!(
        user_data_size(&f.data, USER_DATA_GAME, &profile, "documents").unwrap(),
        0,
        "a shared set has no profile layer"
    );
    assert_eq!(
        user_data_size(&f.data, USER_DATA_GAME, &profile, "missing")
            .unwrap_err()
            .code,
        "user_data.unknown_set"
    );
}

#[cfg(not(windows))]
#[test]
fn a_linux_redirect_is_refused_instead_of_copying_its_layer() {
    let _guard = USER_DATA_MOVE.lock().unwrap();
    let f = user_data_fixture("user-data-move-linux");
    let (profile, _, upper) = isolated_user_data(&f);
    fs::write(upper.join("seen.txt"), "profile").unwrap();

    let error = move_profile_user_data(&f.data, USER_DATA_GAME, &profile, "documents").unwrap_err();

    assert_eq!(error.code, "user_data.move_unsupported");
    assert!(upper.join("seen.txt").is_file());
    assert!(is_isolated(&f, &profile));
}
