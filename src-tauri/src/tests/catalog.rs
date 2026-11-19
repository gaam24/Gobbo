//! Games in the list: joining it, the blocking checks before deployment and the per-platform data folders.

use super::*;
use crate::context::data::{DEPLOYMENT_FILE, JOURNAL_FILE};
use crate::context::load_settings;
use crate::context::managed::managed_games;
use crate::deploy::ops::apply_deployment;
use crate::migrations::{LEGACY_GAMES_FILE, removed_path};
use crate::settings::managed::add_game;
use crate::settings::managed::remove_game;
use crate::status::sync_profiles as status;
use crate::test_support::scratch;

#[test]
fn a_game_joins_the_list_only_when_its_folder_has_the_game_files() {
    test_games::install();
    let base = scratch!("app-add");
    let data = Data::new(base.join("data"));
    let empty = base.join("pusty");
    fs::create_dir_all(&empty).unwrap();
    assert!(add_game(&data, SLOT_GAME, &empty.display().to_string()).is_err());
    assert!(add_game(&data, SLOT_GAME, "  ").is_err());
    assert!(managed_games(&data).unwrap().is_empty());

    let game = slot_game_folder(&base);
    let listed = add_game(&data, SLOT_GAME, &game.display().to_string()).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, SLOT_GAME);
    assert!(listed[0].ready);
    assert_eq!(
        load_settings(&data.game(SLOT_GAME)).game_root,
        game.display().to_string()
    );
    assert!(!data.dir.join(LEGACY_GAMES_FILE).exists());
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn a_required_file_blocks_deployment_until_it_is_installed() {
    let base = scratch!("required-file");
    let game_root = base.join("game");
    fs::create_dir_all(&game_root).unwrap();
    fs::write(game_root.join("game.exe"), b"").unwrap();
    let data = Data::new(base.join("data"));
    save_folders(&data, PLUGIN_GAME, &game_root, "");

    let status = status(&data, PLUGIN_GAME).unwrap();
    assert!(
        status
            .problems
            .iter()
            .any(|problem| problem.code == "status.requirement_missing")
    );

    let err = apply_deployment(&data, PLUGIN_GAME).unwrap_err();
    assert_eq!(err.code, "deploy.requirement_missing");
    let _ = fs::remove_dir_all(&base);
}

#[cfg(target_os = "linux")]
#[test]
fn a_mod_provides_a_required_file_and_the_overlay_keeps_the_game_folder_clean() {
    let base = scratch!("required-by-mod");
    let game_root = base.join("game");
    fs::create_dir_all(&game_root).unwrap();
    fs::write(game_root.join("game.exe"), b"").unwrap();
    let data = Data::new(base.join("data"));
    save_folders(&data, PLUGIN_GAME, &game_root, "");
    let zip = crate::test_support::make_zip(
        &base,
        "loader.zip",
        &[
            ("BepInEx/core/BepInEx.dll", "loader"),
            ("winhttp.dll", "proxy"),
        ],
    );
    import(&data, PLUGIN_GAME, &zip, None, &mut |_, _| {}).unwrap();
    let rows: Vec<Row> = status(&data, PLUGIN_GAME)
        .unwrap()
        .mods
        .iter()
        .map(|mod_view| row(&mod_view.folder, true))
        .collect();
    save_profile(&data, PLUGIN_GAME, &rows).unwrap();
    let problems = status(&data, PLUGIN_GAME).unwrap().problems;
    assert!(
        !problems
            .iter()
            .any(|problem| problem.code == "status.requirement_missing")
    );

    apply_deployment(&data, PLUGIN_GAME).unwrap();
    assert!(!game_root.join("BepInEx").exists());
    assert!(!game_root.join("winhttp.dll").exists());
    let _ = fs::remove_dir_all(&base);
}

#[cfg(target_os = "linux")]
#[test]
fn an_external_root_resolves_from_the_proton_prefix() {
    let base = scratch!("external-prefix");
    let library = base.join("Steam");
    let game_root = library.join("steamapps/common/External Game");
    fs::create_dir_all(&game_root).unwrap();
    fs::write(game_root.join("game.exe"), b"").unwrap();
    let prefix = library.join("steamapps/compatdata/3003/pfx");
    let appdata = prefix.join("drive_c/users/steamuser/AppData/Roaming/External Game");
    fs::create_dir_all(&appdata).unwrap();
    let data = Data::new(base.join("data"));
    save_folders(&data, EXTERNAL_GAME, &game_root, "");

    let status = status(&data, EXTERNAL_GAME).unwrap();
    assert!(status.needs_mods_root);
    assert!(status.mods_ready);
    assert_eq!(status.mods_root, appdata.display().to_string());
    let _ = fs::remove_dir_all(&base);
}

#[cfg(windows)]
#[test]
fn an_external_root_resolves_from_the_roaming_appdata_folder() {
    let base = scratch!("external-windows");
    let game_root = base.join("External Game");
    fs::create_dir_all(&game_root).unwrap();
    fs::write(game_root.join("game.exe"), b"").unwrap();
    let data = Data::new(base.join("data"));
    save_folders(&data, EXTERNAL_GAME, &game_root, "");

    let appdata =
        std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("External Game");
    let status = status(&data, EXTERNAL_GAME).unwrap();
    assert!(status.needs_mods_root);
    assert_eq!(status.mods_root, appdata.display().to_string());
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn adding_a_game_reenters_the_state_lock_on_one_thread() {
    let f = fixture("add-game-lock");
    remove_game(&f.data, SLOT_GAME).unwrap();
    let game = slot_game_folder(&f.base);
    let _held = crate::context::lock_state(&f.data.dir).unwrap();

    let listed = add_game(&f.data, SLOT_GAME, &game.display().to_string()).unwrap();
    assert_eq!(listed.len(), 1);
    assert!(f.data.game(SLOT_GAME).config().exists());

    // The nested lock of `add_game` must not release the outer one.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            sender
                .send(crate::context::lock_state(&f.data.dir).is_err())
                .unwrap();
        });
    });
    assert!(receiver.recv().unwrap(), "the outer lock is still held");
    let _ = fs::remove_dir_all(&f.base);
}

#[test]
fn removing_a_game_moves_its_data_aside_and_adding_it_back_restores_it() {
    let f = fixture("remove-game");
    let ids: Vec<String> = managed_games(&f.data)
        .unwrap()
        .into_iter()
        .map(|game| game.id)
        .collect();
    assert_eq!(ids, [SLOT_GAME]);

    // Mods deployed: the game cannot be deleted, because the deployment can only be undone from its view.
    let state = f.data.game(SLOT_GAME).profile_state("p1");
    fs::create_dir_all(&state).unwrap();
    fs::write(state.join(JOURNAL_FILE), b"").unwrap();
    fs::write(state.join(DEPLOYMENT_FILE), b"{}").unwrap();
    assert!(remove_game(&f.data, SLOT_GAME).is_err());

    fs::remove_file(state.join(JOURNAL_FILE)).unwrap();
    fs::remove_file(state.join(DEPLOYMENT_FILE)).unwrap();
    assert!(remove_game(&f.data, SLOT_GAME).unwrap().is_empty());
    assert!(managed_games(&f.data).unwrap().is_empty());
    assert!(!f.data.dir.join(LEGACY_GAMES_FILE).exists());
    // The game's data stays on disk, in the folder of a removed game.
    assert!(!f.data.game(SLOT_GAME).config().exists());
    assert!(
        removed_path(&f.data.dir, SLOT_GAME)
            .join("config.json")
            .exists()
    );

    // Adding the game again brings the same data folder back.
    let game = slot_game_folder(&f.base);
    add_game(&f.data, SLOT_GAME, &game.display().to_string()).unwrap();
    assert!(f.data.game(SLOT_GAME).config().exists());
    assert!(!removed_path(&f.data.dir, SLOT_GAME).exists());
    let _ = fs::remove_dir_all(&f.base);
}
