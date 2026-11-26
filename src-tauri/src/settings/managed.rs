//! The commands that change the games the user added: check a game folder, add a game with its folder, remove it from
//! the list, and restore or delete the data of a removed game. The folder store is in `context::managed`.

use crate::context::managed::{ManagedGame, managed_games, restore_removed};
use crate::context::{Data, Settings, idle_data, save_settings};
use crate::error::{AppError, io_at};
use crate::games::{self, detect};
use crate::migrations::removed_path;
use std::fs;
use std::io;
use std::path::PathBuf;

/// The game folder the user gave, as a path. It must not be empty, and it must have the files the game definition marks.
/// Adding and checking a game give the same error for a folder that is not the game's.
fn game_folder(game: &games::GameDef, game_root: &str) -> Result<PathBuf, AppError> {
    game_folder_for(game, game_root, !crate::platform::is_windows())
}

/// `game_folder` with the marker rule's platform given, so a test can check both sides on one machine.
fn game_folder_for(
    game: &games::GameDef,
    game_root: &str,
    is_linux: bool,
) -> Result<PathBuf, AppError> {
    // A folder typed as `~/...` is in the home folder, as the saved game folder is.
    let game_root =
        crate::domain::pathkey::expand_home(game_root.trim(), &crate::games::paths::home());

    if game_root.is_empty() {
        return Err(crate::error::game_folder_missing());
    }

    let path = PathBuf::from(&game_root);

    if let Some(path) = detect::game_folder(&path, game, is_linux) {
        return Ok(path);
    }

    if !path.is_dir() {
        return Err(AppError::user("games.not_a_folder").with("path", &game_root));
    }

    // The message names the files the rule asks for, so the user knows what the folder must hold.
    let code = if game.detect.any_marker {
        "games.not_game_folder_any"
    } else {
        "games.not_game_folder"
    };
    Err(AppError::user(code)
        .with("path", &game_root)
        .with("game", &game.name)
        .with("files", detect::expected_markers(is_linux, game)))
}

/// Checks a game folder without adding the game. Returns the marker file that the folder matched under the scan's rule
/// (the first file of the satisfied set that it has), or `None` when the satisfied set names no file.
pub fn check_game_folder(game_id: &str, game_root: &str) -> Result<Option<String>, AppError> {
    let game = games::load(game_id)?;
    check_game_folder_in(&game, game_root, !crate::platform::is_windows())
}

/// `check_game_folder` with the definition and the marker rule's platform given, for a test on either system.
fn check_game_folder_in(
    game: &games::GameDef,
    game_root: &str,
    is_linux: bool,
) -> Result<Option<String>, AppError> {
    let path = game_folder_for(game, game_root, is_linux)?;
    Ok(detect::marker_file_for(is_linux, &path, game))
}

/// Adds a game to Gobbo. `game_root` must have the files from the game definition. When the game was removed before, its
/// old data folder comes back, so the library and the profiles are kept.
pub fn add_game(root: &Data, game_id: &str, game_root: &str) -> Result<Vec<ManagedGame>, AppError> {
    let _state = crate::context::managed::lock_state(root)?;
    let game = games::load(game_id)?;
    let path = game_folder(&game, game_root)?;
    let game_root = path.display().to_string();
    restore_removed(root, game_id)?;
    let current = crate::context::load_settings(&root.game(game_id));
    let home = crate::games::paths::home();
    let install = detect::classify_install(&home, &path);
    let mut settings = Settings {
        game_root,
        ..current
    };
    crate::context::settings::apply_install(&mut settings, &install, true);
    let prefix = crate::context::target::prefix(root, &game, &settings);
    let under_proton =
        crate::context::target::runtime(&game, &settings) != crate::games::runtime::Runtime::Native;

    if let Some(dir) = game.find_order_dir(&path, &home, prefix, under_proton) {
        settings.settings_dir = dir.display().to_string();
    }

    save_settings(root, game_id, settings)?;
    managed_games(root)
}

/// Removes the game from the list by moving its data folder to `games/<id>.removed`. The data stays on disk and comes
/// back when the game is added again. It does not remove the game while it runs or its mods are deployed.
pub fn remove_game(root: &Data, game_id: &str) -> Result<Vec<ManagedGame>, AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    games::load(game_id)?;
    idle_data(root, game_id)?;

    if crate::context::running::game_running(root, game_id)? {
        return Err(crate::error::game_running());
    }

    let active = root.game(game_id);

    if !active.dir.is_dir() {
        return managed_games(root);
    }

    let removed = removed_path(&root.dir, game_id);

    if removed.exists() {
        return Err(io_at(&removed)(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "the removed game data folder already exists",
        )));
    }

    fs::rename(&active.dir, &removed).map_err(io_at(&active.dir))?;
    managed_games(root)
}

/// Moves a removed game's data folder back to `games/<id>` and returns the games that are left in the list.
pub fn restore_game(root: &Data, game_id: &str) -> Result<Vec<ManagedGame>, AppError> {
    let _state = crate::context::managed::lock_state(root)?;
    games::load(game_id)?;
    let removed = removed_path(&root.dir, game_id);

    if !removed.is_dir() {
        return Err(io_at(&removed)(io::Error::new(
            io::ErrorKind::NotFound,
            "the removed game data folder does not exist",
        )));
    }

    restore_removed(root, game_id)?;
    managed_games(root)
}

/// Deletes the data of a removed game. Only `games/<id>.removed` is touched; an active folder of the same game is
/// never removed by this call.
pub fn delete_removed_game(root: &Data, game_id: &str) -> Result<(), AppError> {
    let _state = crate::context::managed::lock_state(root)?;
    games::load(game_id)?;
    let removed = removed_path(&root.dir, game_id);
    crate::infra::fs::remove_path(&removed).map_err(io_at(&removed))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::add_game;
    use crate::context::{Data, Settings, load_settings, save_settings};
    use crate::games::stores::Store;
    use crate::test_support::games as test_games;
    use crate::test_support::{scratch, write};

    /// A slot-shaped game folder with the markers its definition needs.
    fn install_slot(base: &Path, dir: &str) -> PathBuf {
        let root = base.join(dir);
        write(&root.join("bin/game.exe"), "");
        write(&root.join("content/base/x.txt"), "");
        root
    }

    /// A definition with a Windows build and a native Linux build.
    fn native_game() -> crate::games::GameDef {
        crate::games::GameDef::parse(
            r#"
schema = 1
id = "native-game"
name = "Native Game"
[detect]
markers = ["Game.exe"]
linux_markers = ["Game.x86_64"]
[[executables]]
name = "Game"
path = "Game.exe"
path_linux = "Game.x86_64"
[deploy]
[[deploy.slots]]
dir = "Mods"
"#,
        )
        .unwrap()
    }

    #[test]
    fn locate_accepts_a_native_folder_on_linux_and_a_windows_folder_on_both() {
        let base = scratch!("locate-native");
        let windows = base.join("Windows Game");
        let linux = base.join("Native Game");
        write(&windows.join("Game.exe"), "");
        write(&linux.join("Game.x86_64"), "");
        let game = native_game();

        assert_eq!(
            super::check_game_folder_in(&game, &windows.display().to_string(), false).unwrap(),
            Some("Game.exe".to_string())
        );
        assert_eq!(
            super::check_game_folder_in(&game, &windows.display().to_string(), true).unwrap(),
            Some("Game.exe".to_string())
        );
        assert!(super::check_game_folder_in(&game, &linux.display().to_string(), false).is_err());
        // The native folder is accepted by its Linux marker, so that is the marker the check shows.
        assert_eq!(
            super::check_game_folder_in(&game, &linux.display().to_string(), true).unwrap(),
            Some("Game.x86_64".to_string())
        );
    }

    #[test]
    fn a_folder_without_the_markers_names_the_files_it_should_have() {
        let base = scratch!("locate-message");
        let empty = base.join("Empty Folder");
        std::fs::create_dir_all(&empty).unwrap();
        let game = native_game();
        let text = empty.display().to_string();

        let windows = super::game_folder_for(&game, &text, false).unwrap_err();
        assert_eq!(windows.code, "games.not_game_folder");
        assert_eq!(windows.params["files"], "Game.exe");

        let linux = super::game_folder_for(&game, &text, true).unwrap_err();
        assert_eq!(linux.params["files"], "Game.exe / Game.x86_64");

        let mut any = native_game();
        any.detect.any_marker = true;
        let either = super::game_folder_for(&any, &text, true).unwrap_err();
        assert_eq!(either.code, "games.not_game_folder_any");

        let missing = base.join("No Such Folder").display().to_string();
        let not_a_folder = super::game_folder_for(&game, &missing, false).unwrap_err();
        assert_eq!(not_a_folder.code, "games.not_a_folder");
    }

    #[test]
    fn the_check_accepts_exactly_the_folders_the_scan_recognises() {
        let base = scratch!("locate-agrees");
        let game = native_game();
        let windows = base.join("Windows Game");
        write(&windows.join("Game.exe"), "");
        let native = base.join("Native Game");
        write(&native.join("Game.x86_64"), "");
        let partial = base.join("Partial Game");
        write(&partial.join("Other.txt"), "");

        for is_linux in [false, true] {
            for folder in [&windows, &native, &partial] {
                let scanned = crate::games::detect::has_markers_for(is_linux, folder, &game);
                let checked =
                    super::check_game_folder_in(&game, &folder.display().to_string(), is_linux)
                        .is_ok();
                assert_eq!(scanned, checked, "{} on linux={is_linux}", folder.display());
            }
        }
    }

    #[test]
    fn locate_uses_the_only_child_that_has_the_markers() {
        let base = scratch!("locate-wrapper");
        let outer = base.join("AstralGames");
        let inner = outer.join("Game");
        write(&inner.join("Game.exe"), "");
        let game = native_game();

        assert_eq!(
            super::check_game_folder_in(&game, &outer.display().to_string(), false).unwrap(),
            Some("Game.exe".to_string())
        );

        let second = outer.join("Game Copy");
        write(&second.join("Game.exe"), "");
        assert!(super::check_game_folder_in(&game, &outer.display().to_string(), false).is_err());
    }

    #[test]
    fn adding_a_game_remembers_the_store_that_claims_its_folder() {
        test_games::install();
        let base = scratch!("add-source");
        let steam = install_slot(&base.join("steamapps/common"), "Slot");
        let gog = install_slot(&base.join("GOG Games"), "Slot Gog");
        write(&gog.join("goggame-456.info"), "{}");

        let steam_root = Data::new(base.join("data-steam"));
        add_game(
            &steam_root,
            test_games::SLOT_ID,
            &steam.display().to_string(),
        )
        .unwrap();
        assert_eq!(
            load_settings(&steam_root.game(test_games::SLOT_ID)).source,
            "steam"
        );

        let gog_root = Data::new(base.join("data-gog"));
        add_game(&gog_root, test_games::SLOT_ID, &gog.display().to_string()).unwrap();
        assert_eq!(
            load_settings(&gog_root.game(test_games::SLOT_ID)).source,
            "gog"
        );
    }

    #[test]
    fn moving_the_game_folder_takes_the_store_prefix_unless_the_same_save_picks_one() {
        let install = crate::games::detect::StoreInstall {
            store: Store::Heroic,
            prefix: Some(PathBuf::from("/prefixes/Slot/pfx")),
        };
        assert_eq!(install.prefix_text(), "/prefixes/Slot/pfx");
        let none = crate::games::detect::StoreInstall {
            store: Store::Manual,
            prefix: None,
        };
        assert_eq!(none.prefix_text(), "");

        // A save that changes only the folder takes the store's prefix (none here); one that also picks a prefix keeps it.
        test_games::install();
        let base = scratch!("save-prefix");
        let first = install_slot(&base.join("Games"), "Slot One");
        let second = install_slot(&base.join("Games"), "Slot Two");
        let root = Data::new(base.join("data"));
        let chosen = base.join("Prefixes/Slot/pfx").display().to_string();
        save_settings(
            &root,
            test_games::SLOT_ID,
            Settings {
                game_root: first.display().to_string(),
                prefix_dir: chosen.clone(),
                ..Settings::default()
            },
        )
        .unwrap();
        assert_eq!(
            load_settings(&root.game(test_games::SLOT_ID)).prefix_dir,
            chosen
        );

        save_settings(
            &root,
            test_games::SLOT_ID,
            Settings {
                game_root: second.display().to_string(),
                ..load_settings(&root.game(test_games::SLOT_ID))
            },
        )
        .unwrap();
        assert_eq!(
            load_settings(&root.game(test_games::SLOT_ID)).prefix_dir,
            ""
        );
    }

    #[test]
    fn saving_a_different_game_folder_reclassifies_the_store() {
        test_games::install();
        let base = scratch!("save-source");
        let gog = install_slot(&base.join("GOG Games"), "Slot Gog");
        write(&gog.join("goggame-456.info"), "{}");
        let steam = install_slot(&base.join("library/steamapps/common"), "Slot Steam");
        let root = Data::new(base.join("data"));
        save_settings(
            &root,
            test_games::SLOT_ID,
            Settings {
                game_root: gog.display().to_string(),
                ..Settings::default()
            },
        )
        .unwrap();
        assert_eq!(load_settings(&root.game(test_games::SLOT_ID)).source, "gog");

        save_settings(
            &root,
            test_games::SLOT_ID,
            Settings {
                game_root: steam.display().to_string(),
                ..load_settings(&root.game(test_games::SLOT_ID))
            },
        )
        .unwrap();
        assert_eq!(
            load_settings(&root.game(test_games::SLOT_ID)).source,
            "steam"
        );
    }
}
