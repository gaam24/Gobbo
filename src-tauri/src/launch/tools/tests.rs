use crate::games::GameDef;
use crate::import::tool::unpack_tool;
use crate::launch::tools::prerequisites::missing_prerequisites;
use crate::test_support::games as test_games;
use crate::test_support::{make_zip, scratch};
use std::fs;
#[cfg(not(windows))]
use std::path::PathBuf;

#[cfg(not(windows))]
const RUNTIME: &str = "Runtime 8.0";

fn game() -> GameDef {
    test_games::slot()
}

/// A game folder in a Steam library, with the fixture's Proton prefix.
#[cfg(not(windows))]
fn steam_game(name: &str) -> (PathBuf, PathBuf) {
    let library = scratch!(name).join("library");
    let game_root = library.join("steamapps/common/Slot Game");
    fs::create_dir_all(&game_root).unwrap();
    let wine = library.join("steamapps/compatdata/1001/pfx");
    fs::create_dir_all(&wine).unwrap();
    (game_root, wine)
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

    assert!(missing_prerequisites(&game, tool, "C:/Slot Game").is_empty());
}

#[cfg(not(windows))]
#[test]
fn a_prerequisite_is_missing_until_its_runtime_is_in_the_prefix() {
    let game = game();
    let tool = game.tool_for_source("nexus", "4242").unwrap();
    let (game_root, wine) = steam_game("runtime");
    let root = game_root.display().to_string();
    assert_eq!(
        missing_prerequisites(&game, tool, &root),
        vec![RUNTIME.to_string()]
    );

    fs::create_dir_all(wine.join("drive_c/Runtime/8.0.33")).unwrap();

    assert!(missing_prerequisites(&game, tool, &root).is_empty());
}

#[test]
fn a_game_without_a_prefix_has_no_missing_prerequisites() {
    let game = game();
    let tool = game.tool_for_source("nexus", "4242").unwrap();
    let dir = scratch!("no-prefix");

    assert!(missing_prerequisites(&game, tool, &dir.display().to_string()).is_empty());
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
