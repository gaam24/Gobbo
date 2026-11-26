use super::*;
use crate::context::Data;
use crate::context::Settings;
use crate::context::load_settings;
use crate::deploy::overlay::Session;
#[cfg(windows)]
use crate::launch::executable::pick_executable;
use crate::launch::start::launch::launch_command_with_settings;
#[cfg(windows)]
use crate::launch::start::launch::legacy_long;
use crate::test_support::games as test_games;
use crate::test_support::scratch;
use std::fs;
#[cfg(windows)]
use std::path::PathBuf;

#[test]
fn a_proxy_from_a_mod_under_a_deployment_root_inside_the_game_loads_native() {
    let base = scratch!("launch-native-inner-root");
    let game_root = base.join("game");
    let program = "inner/bin/game.exe";
    fs::create_dir_all(game_root.join("inner/bin")).unwrap();
    fs::write(game_root.join(program), "").unwrap();
    let session = Session::with_roots(
        &base.join("state"),
        &game_root.join("inner"),
        &game_root,
        Vec::new(),
    );
    let proxy = session.mods.join("inner/bin/version.dll");
    fs::create_dir_all(proxy.parent().unwrap()).unwrap();
    fs::write(&proxy, "proxy").unwrap();
    let game = test_games::mirror();

    assert_eq!(
        native_dlls_for_launch(&game, &game_root, program, Some(&session)),
        ["version.dll"],
        "the loader mod's proxy is in the layer, not in the game folder"
    );
    assert!(native_dlls_for_launch(&game, &game_root, program, None).is_empty());
}

#[test]
fn a_launch_that_carries_the_settings_does_not_read_config_again() {
    crate::test_support::games::install();
    let base = scratch!("launch-settings-snapshot");
    let root = Data::new(base.join("data"));
    let game_data = root.game(test_games::MIRROR_ID);
    let mut game = test_games::mirror();
    game.detect.steam_app_ids.clear();
    let program = game.executables[0].path.clone();
    game.executables[0].path_linux = Some(program);

    // Folder A holds the program, and the saved config points at it.
    let folder_a = base.join("a");
    fs::create_dir_all(folder_a.join("bin/x64")).unwrap();
    fs::write(folder_a.join("bin/x64/game.exe"), "").unwrap();
    crate::context::save_settings(
        &root,
        test_games::MIRROR_ID,
        Settings {
            game_root: folder_a.display().to_string(),
            ..Settings::default()
        },
    )
    .unwrap();
    let snapshot = load_settings(&game_data);

    let (launch, _, _) =
        launch_command_with_settings(&game, &root, test_games::MIRROR_ID, None, &base, &snapshot)
            .unwrap();

    // A new config points at folder B; the command built from the snapshot still carries folder A.
    let folder_b = base.join("b");
    fs::create_dir_all(folder_b.join("bin/x64")).unwrap();
    fs::write(folder_b.join("bin/x64/game.exe"), "").unwrap();
    crate::context::save_settings(
        &root,
        test_games::MIRROR_ID,
        Settings {
            game_root: folder_b.display().to_string(),
            ..Settings::default()
        },
    )
    .unwrap();

    let Launch::Plain(command) = launch else {
        panic!("with nothing deployed the native game starts as a plain command");
    };
    assert_eq!(
        command.get_program(),
        folder_a.join("bin/x64/game.exe").as_os_str()
    );

    let (launch, _, _) = launch_command(&game, &root, test_games::MIRROR_ID, None, &base).unwrap();
    let Launch::Plain(command) = launch else {
        panic!("with nothing deployed the native game starts as a plain command");
    };
    assert_eq!(
        command.get_program(),
        folder_b.join("bin/x64/game.exe").as_os_str()
    );
}

/// Windows only: a copy of the system shell in a folder past the legacy path limit starts through the real launch
/// path and exits with its code. Without the verbatim program path the start fails with "path not found".
#[cfg(windows)]
#[test]
fn a_program_in_a_folder_past_the_legacy_path_limit_starts_on_windows() {
    let base = scratch!("launch-legacy-long");
    let mut folder = base.clone();
    while !legacy_long(&folder) {
        folder = folder.join("long-folder-name-segment-0123456789");
    }
    fs::create_dir_all(&folder).unwrap();
    let shell = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32/cmd.exe");
    fs::copy(shell, folder.join("tool.exe")).unwrap();

    let mut game = test_games::slot();
    game.executables[0].path = "tool.exe".to_string();
    let root = Data::new(base.join("data"));
    let data = root.game(&game.id);
    let settings = Settings {
        game_root: folder.display().to_string(),
        launch_args: "/c exit 0".to_string(),
        ..Settings::default()
    };
    let exe = pick_executable(&game, None).unwrap();
    let (launch, with_mods, spawn_error) =
        program_launch(&game, &data, &settings, exe, &base).unwrap();
    assert!(!with_mods);

    let mut running = launch.start(spawn_error, None).unwrap();
    assert_eq!(running.wait(), Some(0));
}
