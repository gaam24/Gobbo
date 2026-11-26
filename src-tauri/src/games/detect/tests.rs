use std::fs;
use std::path::Path;

use super::*;
use crate::games::GameDef;
use crate::games::stores::Store;
use crate::games::stores::steam::tests::{install_slot, steam_home};
use crate::games::stores::steam::{find, libraries_in};
use crate::test_support::games as test_games;
use crate::test_support::{scratch, write};

#[test]
fn a_folder_without_the_game_files_is_not_a_match() {
    let home = scratch!("markers");
    let steam = steam_home(&home, &[]);
    fs::create_dir_all(steam.join("steamapps/common/Slot Game")).unwrap();

    assert_eq!(find(&test_games::slot(), &libraries_in(&[steam])), None);
}

#[test]
fn one_marker_is_enough_when_the_definition_allows_any() {
    let base = scratch!("markers-any");
    let root = base.join("Game");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Game.x86_64"), b"").unwrap();
    let mut game = test_games::slot();
    game.detect.markers = vec!["Game.exe".to_string(), "Game.x86_64".to_string()];

    assert!(!has_markers(&root, &game));
    game.detect.any_marker = true;
    assert!(has_markers(&root, &game));
    game.detect.markers.clear();
    game.detect.any_marker = false;
    assert!(has_markers(&root, &game));
}

/// A fixture game whose two builds name their program differently.
fn native_game() -> GameDef {
    let mut game = test_games::slot();
    game.detect.markers = vec!["Game.exe".to_string()];
    game.detect.linux_markers = vec!["Game.x86_64".to_string()];
    game.detect.steam_install_dirs = vec!["Native Game".to_string()];
    game.detect.steam_app_ids.clear();
    game
}

#[test]
fn the_marker_rule_uses_each_platform_set_without_leaking_it_to_the_other() {
    let base = scratch!("linux-markers-rule");
    let windows = base.join("Windows Game");
    let linux = base.join("Native Game");
    let both = base.join("Both Builds");
    let empty = base.join("Empty Game");
    for root in [&windows, &linux, &both, &empty] {
        fs::create_dir_all(root).unwrap();
    }
    write(&windows.join("Game.exe"), "");
    write(&linux.join("Game.x86_64"), "");
    write(&both.join("Game.exe"), "");
    write(&both.join("Game.x86_64"), "");

    let game = native_game();
    assert!(has_markers_for(false, &windows, &game));
    assert!(!has_markers_for(false, &linux, &game));
    assert!(has_markers_for(true, &linux, &game));
    assert!(has_markers_for(true, &windows, &game));
    assert!(has_markers_for(true, &both, &game));
    assert!(!has_markers_for(true, &empty, &game));
    assert!(!has_markers_for(false, &empty, &game));
}

#[test]
fn no_linux_markers_leaves_the_rule_as_it_was() {
    let base = scratch!("linux-markers-empty");
    let root = base.join("Game");
    fs::create_dir_all(&root).unwrap();
    let mut game = native_game();
    game.detect.linux_markers.clear();

    assert!(!has_markers_for(true, &root, &game));
    assert!(!has_markers_for(false, &root, &game));
    write(&root.join("Game.exe"), "");
    assert!(has_markers_for(true, &root, &game));
    assert!(has_markers_for(false, &root, &game));
    game.detect.markers.clear();
    assert!(has_markers_for(true, &root, &game));
    assert!(has_markers_for(false, &root, &game));
}

#[test]
fn any_marker_relaxes_each_platform_set_on_its_own() {
    let base = scratch!("linux-markers-any");
    let root = base.join("Game");
    fs::create_dir_all(&root).unwrap();
    write(&root.join("Game.x86_64"), "");
    let mut game = native_game();
    game.detect.linux_markers = vec!["Game.x86_64".into(), "Game2.x86_64".into()];

    assert!(!has_markers_for(true, &root, &game));
    game.detect.any_marker = true;
    assert!(has_markers_for(true, &root, &game));
    assert!(!has_markers_for(false, &root, &game));

    write(&root.join("Game.exe"), "");
    game.detect.linux_markers.clear();
    game.detect.markers = vec!["Game.exe".into(), "Game2.exe".into()];
    game.detect.any_marker = false;
    assert!(!has_markers_for(false, &root, &game));
    game.detect.any_marker = true;
    assert!(has_markers_for(false, &root, &game));
}

#[test]
fn the_scan_finds_a_native_install_through_its_linux_marker() {
    let home = scratch!("scan-native");
    let steam = steam_home(&home, &[]);
    let root = steam.join("steamapps/common/Native Game");
    fs::create_dir_all(&root).unwrap();
    write(&root.join("Game.x86_64"), "");

    let found = find(&native_game(), &libraries_in(&[steam]));
    assert_eq!(found.is_some(), cfg!(not(windows)));
    if let Some(found) = found {
        assert_eq!(found.game_root, root.display().to_string());
        assert_eq!(found.store, "steam");
    }
}

#[test]
fn the_order_file_folder_is_found_in_the_proton_prefix_once_it_exists() {
    let home = scratch!("proton");
    let steam = steam_home(&home, &[]);
    let root = install_slot(&steam, "Slot Game");
    let game = test_games::slot();
    let find = || {
        game.find_order_dir(
            &root,
            &home,
            super::super::stores::steam::proton_prefix(&game, &root),
            true,
        )
    };
    assert_eq!(find(), None);

    let prefix =
        steam.join("steamapps/compatdata/1001/pfx/drive_c/users/steamuser/Documents/Slot Game");
    fs::create_dir_all(&prefix).unwrap();
    assert_eq!(find(), Some(prefix));
}

#[test]
fn classify_install_names_the_store_that_claims_the_folder() {
    let base = scratch!("classify-install");
    let steam = base.join("library/steamapps/common/Slot");
    let gog = base.join("GOG Games/Slot Gog");
    let epic = base.join("Epic Games/Slot Epic");
    let heroic = base.join("Heroic/Slot Heroic");
    let heroic_gog = base.join("Heroic/Slot Heroic Gog");
    let legendary = base.join("Legendary/Slot");
    let manual = base.join("Games/Slot Manual");
    let folders = crate::platform::StoreFolders {
        epic_list: Some(base.join("UnrealEngineLauncher")),
        heroic: vec![base.join("config/heroic")],
        legendary: Some(base.join("config/legendary")),
        ..crate::platform::StoreFolders::default()
    };

    for root in [
        &steam,
        &gog,
        &epic,
        &heroic,
        &heroic_gog,
        &legendary,
        &manual,
    ] {
        fs::create_dir_all(root).unwrap();
    }
    write(&gog.join("goggame-456.info"), "{}");
    // Heroic's GOG installs keep the info file too; the launcher that manages the folder still wins.
    write(&heroic_gog.join("goggame-789.info"), "{}");
    write(
        &base.join("UnrealEngineLauncher/LauncherInstalled.dat"),
        &serde_json::json!({
            "InstallationList": [{
                "AppName": "EpicApp",
                "InstallLocation": epic.display().to_string(),
            }],
        })
        .to_string(),
    );
    write(
        &base.join("config/heroic/legendaryConfig/legendary/installed.json"),
        &serde_json::json!({
            "HeroicApp": { "install_path": heroic.display().to_string() },
        })
        .to_string(),
    );
    write(
        &base.join("config/heroic/gog_store/installed.json"),
        &serde_json::json!({ "installed": [
            { "appName": "789", "install_path": heroic_gog.display().to_string() },
        ] })
        .to_string(),
    );
    write(
        &base.join("config/legendary/installed.json"),
        &serde_json::json!({
            "LegendaryApp": { "install_path": legendary.display().to_string() },
        })
        .to_string(),
    );

    let classify = |root: &Path| classify_in(root, &folders).store;
    assert_eq!(classify(&steam), Store::Steam);
    assert_eq!(classify(&gog), Store::Gog);
    assert_eq!(classify(&epic), Store::Epic);
    assert_eq!(classify(&heroic), Store::Heroic);
    assert_eq!(classify(&heroic_gog), Store::Heroic);
    assert_eq!(classify(&legendary), Store::Legendary);
    assert_eq!(classify(&manual), Store::Manual);
    assert_eq!(
        classify_in(&epic, &crate::platform::StoreFolders::default()).store,
        Store::Manual
    );
}

#[test]
fn scan_answers_for_every_game_in_order() {
    let home = scratch!("scan");
    let steam = steam_home(&home, &[]);
    let root = install_slot(&steam, "Slot Game");
    let games = [test_games::slot(), test_games::mirror()];

    let scans = scan_in(
        &libraries_in(&[steam]),
        &games,
        &crate::platform::StoreFolders::default(),
    );
    assert_eq!(scans.len(), 2);
    assert_eq!(scans[0].id, test_games::SLOT_ID);
    assert_eq!(
        scans[0].found.as_ref().map(|f| f.game_root.clone()),
        Some(root.display().to_string())
    );
    assert_eq!(scans[1].id, test_games::MIRROR_ID);
    assert!(scans[1].found.is_none());
    assert!(scans[0].others.is_empty());
    assert!(scans[1].others.is_empty());
}

#[test]
fn a_scan_keeps_every_install_with_the_steam_one_first() {
    let home = scratch!("scan-many");
    let steam = steam_home(&home, &[]);
    let steam_root = install_slot(&steam, "Slot Game");
    let gog_root = home.join("GOG Games").join("Slot Gog");
    // The GOG folder has the game files and one `goggame-*.info` file naming the product id.
    write(&gog_root.join("bin/game.exe"), "");
    write(&gog_root.join("content/base/x.txt"), "");
    write(&gog_root.join("goggame-456.info"), "{}");

    let mut game = test_games::slot();
    game.detect.gog_ids = vec!["456".into()];
    let folders = crate::platform::StoreFolders {
        gog_libraries: vec![home.join("GOG Games")],
        ..crate::platform::StoreFolders::default()
    };

    let scans = scan_in(&libraries_in(&[steam]), &[game], &folders);
    let scan = &scans[0];
    let preferred = scan.found.as_ref().expect("the Steam install is found");
    assert_eq!(preferred.store, "steam");
    assert_eq!(preferred.game_root, steam_root.display().to_string());

    assert_eq!(scan.others.len(), 1);
    assert_eq!(scan.others[0].store, "gog");
    assert_eq!(scan.others[0].game_root, gog_root.display().to_string());
}

#[test]
fn two_claims_of_one_folder_appear_once_under_the_first_store() {
    let base = scratch!("scan-duplicates");
    let root = base.join("Games/Slot");
    write(&root.join("bin/game.exe"), "");
    write(&root.join("content/base/x.txt"), "");
    // Heroic's GOG list and the folder's own info file both name the install.
    write(&root.join("goggame-456.info"), "{}");
    write(
        &base.join("heroic/gog_store/installed.json"),
        &serde_json::json!({ "installed": [
            { "appName": "456", "install_path": root.display().to_string() },
        ] })
        .to_string(),
    );
    let folders = crate::platform::StoreFolders {
        gog_libraries: vec![base.join("Games")],
        heroic: vec![base.join("heroic")],
        ..crate::platform::StoreFolders::default()
    };

    let mut game = test_games::slot();
    game.detect.gog_ids = vec!["456".into()];
    let scans = scan_in(&[], &[game], &folders);
    assert_eq!(
        scans[0].found.as_ref().map(|found| found.store.as_str()),
        Some("heroic")
    );
    assert!(scans[0].others.is_empty());
}

#[cfg(windows)]
#[test]
fn the_windows_order_file_folder_is_in_documents() {
    let home = scratch!("windows-settings");
    let settings = home.join("Documents/Slot Game");
    fs::create_dir_all(&settings).unwrap();

    assert_eq!(
        test_games::slot().find_order_dir(&home.join("game"), &home, None, false),
        Some(settings)
    );
}
