//! The games the user added to Gobbo: checking a game folder, adding a game with its folder, removing it from the list
//! and restoring or deleting the data of a removed game. The folders and their reading are in `context::managed`.

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
    let game_root = game_root.trim();

    if game_root.is_empty() {
        return Err(crate::error::game_folder_missing());
    }

    let path = PathBuf::from(game_root);

    if !detect::has_markers(&path, game) {
        return Err(AppError::user("games.not_game_folder")
            .with("path", game_root)
            .with("game", &game.name));
    }

    Ok(path)
}

/// Checks a game folder without adding the game. Returns the first of the game's marker files that the folder has, or
/// `None` when the game defines no markers.
pub fn check_game_folder(game_id: &str, game_root: &str) -> Result<Option<String>, AppError> {
    let game = games::load(game_id)?;
    let path = game_folder(&game, game_root)?;
    Ok(game
        .detect
        .markers
        .iter()
        .find(|marker| path.join(marker).exists())
        .cloned())
}

/// Adds a game to Gobbo. `game_root` must have the files from the game definition. When the game was removed before, its
/// old data folder comes back, so the library and the profiles are kept.
pub fn add_game(root: &Data, game_id: &str, game_root: &str) -> Result<Vec<ManagedGame>, AppError> {
    let _state = crate::context::managed::lock_state(root)?;
    let game = games::load(game_id)?;
    let path = game_folder(&game, game_root)?;
    let game_root = game_root.trim();
    restore_removed(root, game_id)?;
    let current = crate::context::load_settings(&root.game(game_id));
    let settings_dir = game
        .find_order_dir(&path, &crate::games::paths::home())
        .map(|dir| dir.display().to_string())
        .unwrap_or(current.settings_dir);
    save_settings(
        root,
        game_id,
        Settings {
            game_root: game_root.to_string(),
            settings_dir,
            ..current
        },
    )?;
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
