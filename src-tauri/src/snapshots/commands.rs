//! Save snapshots as commands: list, create, preview, restore and delete.

use crate::context::Data;
use crate::error::AppError;
use crate::snapshots::{
    RestoreOutcome, RestorePreview, SnapshotView, create_manual, delete, list, preview_public,
    restore_public,
};

crate::commands::macros::game_commands! {
    /// Every snapshot of the game, newest first.
    query snapshots() -> Vec<SnapshotView> = |root, game| Ok(list(root, game));

    /// What restoring a snapshot would change. Nothing is written.
    query preview_snapshot_restore(id: String) -> RestorePreview =
        |root, game| preview_public(root, game, &id);
}

/// Saves the profile's stored data now. The game must not be running.
#[tauri::command]
#[specta::specta]
pub async fn create_snapshot(
    data: tauri::State<'_, Data>,
    game: String,
    profile: String,
) -> Result<SnapshotView, AppError> {
    crate::commands::helpers::for_game(data, game, move |root, game| {
        create_manual(root, game, &profile)
    })
    .await
}

/// Restores a snapshot after writing a fresh backup. The game must not be running.
#[tauri::command]
#[specta::specta]
pub async fn restore_snapshot(
    data: tauri::State<'_, Data>,
    game: String,
    id: String,
) -> Result<RestoreOutcome, AppError> {
    crate::commands::helpers::for_game(data, game, move |root, game| {
        restore_public(root, game, &id)
    })
    .await
}

/// Removes a snapshot folder.
#[tauri::command]
#[specta::specta]
pub async fn delete_snapshot(
    data: tauri::State<'_, Data>,
    game: String,
    id: String,
) -> Result<(), AppError> {
    crate::commands::helpers::for_game(data, game, move |root, game| delete(root, game, &id)).await
}
