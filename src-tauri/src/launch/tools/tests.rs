use crate::context::Data;
use crate::context::Settings;
use crate::games::GameDef;
use crate::import::tool::unpack_tool;
use crate::launch::tools::prerequisites::missing_prerequisites;
use crate::test_support::games as test_games;
use crate::test_support::{make_zip, scratch};
use std::fs;
use std::path::PathBuf;

#[cfg(not(windows))]
const RUNTIME: &str = "Runtime 8.0";

fn game() -> GameDef {
    test_games::slot()
}

/// A game folder in a Steam library, with the fixture's Proton prefix, its data and its settings.
#[cfg(not(windows))]
fn steam_game(name: &str) -> (PathBuf, PathBuf, Data, Settings) {
    let base = scratch!(name);
    let library = base.join("library");
    let game_root = library.join("steamapps/common/Slot Game");
    fs::create_dir_all(&game_root).unwrap();
    let wine = library.join("steamapps/compatdata/1001/pfx");
    fs::create_dir_all(&wine).unwrap();
    let settings = Settings {
        game_root: game_root.display().to_string(),
        ..Settings::default()
    };
    (game_root, wine, Data::new(base.join("data")), settings)
}

#[test]
fn the_nexus_mod_names_its_tool() {
    let game = game();
    let tool = game.tool_for_source("nexus", "4242").unwrap();
    assert_eq!(tool.id, "merger");
    assert!(game.tool_for_source("nexus", "1").is_none());
}

#[test]
fn an_archive_holding_the_program_folder_unpacks_into_the_game_folder() {
    let game = game();
    let tool = game.tool_for_source("nexus", "4242").unwrap();
    let dir = scratch!("wrapped");
    let game_root = dir.join("Slot Game");
    fs::create_dir_all(&game_root).unwrap();
    let archive = make_zip(
        &dir,
        "tool.zip",
        &[
            ("tools/tool.exe", "exe"),
            ("tools/Config/settings.xml", "<xml/>"),
        ],
    );

    unpack_tool(&game_root, tool, &archive).unwrap();

    assert!(game_root.join("tools/tool.exe").is_file());
    assert!(game_root.join("tools/Config/settings.xml").is_file());
    assert!(!game_root.join("tools/tools").exists());
}

#[test]
fn loose_files_of_an_archive_go_into_the_program_folder() {
    let game = game();
    let tool = game.tool_for_source("nexus", "4242").unwrap();
    let dir = scratch!("loose");
    let game_root = dir.join("Slot Game");
    fs::create_dir_all(&game_root).unwrap();
    let archive = make_zip(&dir, "tool.zip", &[("tool.exe", "exe")]);

    unpack_tool(&game_root, tool, &archive).unwrap();

    assert!(game_root.join("tools/tool.exe").is_file());
}

#[test]
fn an_archive_without_the_program_is_an_error() {
    let game = game();
    let tool = game.tool_for_source("nexus", "4242").unwrap();
    let dir = scratch!("no-program");
    let game_root = dir.join("Slot Game");
    fs::create_dir_all(&game_root).unwrap();
    let archive = make_zip(&dir, "tool.zip", &[("readme.txt", "text")]);

    let error = unpack_tool(&game_root, tool, &archive).unwrap_err();

    assert_eq!(error.code, "tools.archive_missing_program");
}

#[test]
fn an_archive_path_outside_the_game_folder_is_refused() {
    let game = game();
    let tool = game.tool_for_source("nexus", "4242").unwrap();
    let dir = scratch!("escape");
    let game_root = dir.join("Slot Game");
    fs::create_dir_all(&game_root).unwrap();
    let archive = make_zip(&dir, "tool.zip", &[("../outside.txt", "text")]);

    let error = unpack_tool(&game_root, tool, &archive).unwrap_err();

    assert_eq!(error.code, "import.bad_archive");
    assert!(!dir.join("outside.txt").exists());
}

#[cfg(windows)]
#[test]
fn windows_runs_a_tool_with_the_runtime_it_has() {
    let game = game();
    let tool = game.tool_for_source("nexus", "4242").unwrap();

    assert!(
        missing_prerequisites(
            &game,
            &Data::new(PathBuf::from("data")),
            tool,
            &Settings {
                game_root: "C:/Slot Game".to_string(),
                ..Settings::default()
            }
        )
        .is_empty()
    );
}

#[cfg(not(windows))]
#[test]
fn a_prerequisite_is_missing_until_its_runtime_is_in_the_prefix() {
    let game = game();
    let tool = game.tool_for_source("nexus", "4242").unwrap();
    let (_, wine, data, settings) = steam_game("runtime");
    assert_eq!(
        missing_prerequisites(&game, &data, tool, &settings),
        vec![RUNTIME.to_string()]
    );

    fs::create_dir_all(wine.join("drive_c/Runtime/8.0.33")).unwrap();

    assert!(missing_prerequisites(&game, &data, tool, &settings).is_empty());
}

#[test]
fn a_game_without_a_prefix_has_no_missing_prerequisites() {
    let game = game();
    let tool = game.tool_for_source("nexus", "4242").unwrap();
    let dir = scratch!("no-prefix");
    let settings = Settings {
        game_root: dir.display().to_string(),
        ..Settings::default()
    };

    assert!(missing_prerequisites(&game, &Data::new(dir.join("data")), tool, &settings).is_empty());
}

#[cfg(not(windows))]
#[test]
fn a_non_steam_game_checks_and_installs_prerequisites_in_the_owned_prefix() {
    use crate::games::runtime::Runtime;
    use crate::launch::proton::run_in_prefix;
    use std::os::unix::fs::PermissionsExt;

    let game = game();
    let tool = game.tool_for_source("nexus", "4242").unwrap();
    let base = scratch!("owned-prerequisites");
    let game_root = base.join("Games/Owned Game");
    fs::create_dir_all(&game_root).unwrap();
    let data = Data::new(base.join("data"));
    let wine = data.game(&game.id).dir.join("compatdata/pfx");
    let settings = Settings {
        game_root: game_root.display().to_string(),
        source: "gog".into(),
        ..Settings::default()
    };

    // Nothing is checked until the game has run and created the prefix.
    assert!(missing_prerequisites(&game, &data, tool, &settings).is_empty());
    fs::create_dir_all(wine.join("drive_c/Runtime/8.0.33")).unwrap();
    assert!(missing_prerequisites(&game, &data, tool, &settings).is_empty());
    fs::remove_dir_all(wine.join("drive_c/Runtime/8.0.33")).unwrap();
    assert_eq!(
        missing_prerequisites(&game, &data, tool, &settings),
        vec![RUNTIME.to_string()]
    );

    // The installer runs in the same prefix, with no Steam app id.
    let proton_dir = base.join("Proton");
    fs::create_dir_all(&proton_dir).unwrap();
    let out = base.join("out.txt");
    let proton = proton_dir.join("proton");
    fs::write(
        &proton,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$STEAM_COMPAT_DATA_PATH\" \"$SteamAppId\" \"$@\" > '{}'\n",
            out.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&proton, fs::Permissions::from_mode(0o755)).unwrap();
    let settings = Settings {
        proton_dir: proton_dir.display().to_string(),
        ..settings
    };
    let code = run_in_prefix(
        &game,
        &data,
        &settings,
        &game_root,
        std::path::Path::new("installer.exe"),
        &["-q".into()],
        &base.join("home"),
        Runtime::OwnedProton,
    )
    .unwrap();
    assert_eq!(code, 0);
    let written = fs::read_to_string(&out).unwrap();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(
        lines[0],
        data.game(&game.id)
            .dir
            .join("compatdata")
            .display()
            .to_string()
    );
    assert_eq!(lines[1], "");
    assert_eq!(lines[2], "waitforexitandrun");
}

#[cfg(not(windows))]
#[test]
fn sha512_is_the_digest_of_the_file() {
    let dir = scratch!("digest");
    let file = dir.join("abc.bin");
    fs::write(&file, b"abc").unwrap();

    assert_eq!(
        crate::infra::io::digest_file::<sha2::Sha512>(&file, &mut |_| {}).unwrap(),
        "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
    );
}
