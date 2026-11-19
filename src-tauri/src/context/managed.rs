//! The games the user added to Gobbo: the folders under `games/` and the data of the games removed from the list.

use crate::context::{Data, load_settings};
use crate::error::{AppError, io_at};
use crate::games;
use serde::Serialize;
use std::fs;

/// A game added to Gobbo: id, game folder from the settings, and whether that folder still exists.
#[derive(Debug, Serialize, specta::Type)]
pub struct ManagedGame {
    pub id: String,
    pub game_root: String,
    pub ready: bool,
}

/// The data folder of a game, active or removed, with its total size.
#[derive(Debug, Serialize, specta::Type)]
pub struct GameDataUsage {
    pub id: String,
    /// The data folder itself, so the interface can open it.
    pub path: String,
    /// `true` for `games/<id>.removed`, `false` for the active `games/<id>`.
    pub removed: bool,
    pub size_kb: u32,
}

/// Takes the state lock for file-moving work, creating the data root first: a new library has no folder for the lock
/// file yet.
pub fn lock_state(root: &Data) -> Result<crate::context::state_lock::StateLock, AppError> {
    fs::create_dir_all(&root.dir).map_err(io_at(&root.dir))?;
    crate::context::lock_state(&root.dir)
}

/// Runs the file-layout migrations with the current game definitions. Safe to call on every start: each step checks
/// its old layout and does nothing when it is absent.
pub fn migrate(root: &Data) -> Result<(), AppError> {
    let games = catalog()?;
    let _state = lock_state(root)?;
    migrate_with(root, &games);
    Ok(())
}

/// Games added to Gobbo, in the order of their definitions, with folders from the settings.
pub fn managed_games(root: &Data) -> Result<Vec<ManagedGame>, AppError> {
    Ok(managed_ids(root)?
        .into_iter()
        .map(|id| {
            let game_root = load_settings(&root.game(&id)).game_root;
            let ready = crate::context::is_dir(&game_root);
            ManagedGame {
                id,
                game_root,
                ready,
            }
        })
        .collect())
}

/// Game ids with an active data folder. The folder itself is the list; the old `games.json` is migrated before it is
/// read.
pub fn managed_ids(root: &Data) -> Result<Vec<String>, AppError> {
    let games = catalog()?;
    {
        let _state = lock_state(root)?;
        migrate_with(root, &games);
    }
    Ok(games
        .into_iter()
        .map(|game| game.id)
        .filter(|id| root.game(id).dir.is_dir())
        .collect())
}

/// Every game data folder with its size, active and removed. Used by the storage page.
pub fn data_usage(root: &Data) -> Result<Vec<GameDataUsage>, AppError> {
    let games = catalog()?;
    {
        let _state = lock_state(root)?;
        migrate_with(root, &games);
    }
    let mut usage = Vec::with_capacity(games.len());
    for game in games {
        let active = root.game(&game.id);
        if active.dir.is_dir() {
            usage.push(GameDataUsage {
                id: game.id.clone(),
                path: active.dir.display().to_string(),
                removed: false,
                size_kb: crate::context::size_kb(crate::infra::fs::folder_size(&active.dir)),
            });
        }
        let removed = crate::migrations::removed_path(&root.dir, &game.id);
        if removed.is_dir() {
            usage.push(GameDataUsage {
                id: game.id,
                path: removed.display().to_string(),
                removed: true,
                size_kb: crate::context::size_kb(crate::infra::fs::folder_size(&removed)),
            });
        }
    }
    Ok(usage)
}

/// Moves `games/<id>.removed` back to `games/<id>`. Does nothing when the active folder is already there or no removed
/// folder exists.
pub fn restore_removed(root: &Data, game_id: &str) -> Result<(), AppError> {
    let active = root.game(game_id);
    if active.dir.is_dir() {
        return Ok(());
    }
    let removed = crate::migrations::removed_path(&root.dir, game_id);
    if !removed.is_dir() {
        return Ok(());
    }
    fs::rename(&removed, &active.dir).map_err(io_at(&removed))
}

/// The definitions of every game, shipped and from the user's folder. A user definition can be added and removed like a
/// shipped one.
fn catalog() -> Result<Vec<games::GameDef>, AppError> {
    let (definitions, _) = games::definitions()?;
    Ok(definitions.into_iter().map(|(game, _)| game).collect())
}

/// Runs the migrations with definitions that were already read.
fn migrate_with(root: &Data, games: &[games::GameDef]) {
    let known: Vec<String> = games.iter().map(|game| game.id.clone()).collect();
    crate::migrations::run(&root.dir, &known);
}
