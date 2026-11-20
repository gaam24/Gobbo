//! Mod packs: exporting, checking, inspecting and importing them, and downloading their mods.

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use tauri::ipc::Channel;
use tauri::{Emitter, Manager, State};

use super::download::PackDownload;
use super::link::{PackLinkDownload, PackLinkProgress};
use super::model::{PackExportView, PackImportView, PackIssue, PackOptions, PackPreview};
use crate::commands::{channel_sink_with_progress, run_task};
use crate::context::Data;
use crate::error::AppError;
use crate::import::ImportProgress;
use crate::infra::tasks::{TaskFlags, TaskKey, kind};

crate::commands::game_commands! {
    /// Writes the chosen profiles as a mod pack: a recipe of the mods, their files and choices, with the archives the
    /// options ask for.
    query export_pack(path: String, options: PackOptions) -> PackExportView =
        |root, game| super::export::export_pack(root, game, Path::new(&path), &options);

    /// What stands in the way of sharing the chosen profiles as a pack.
    query check_pack(options: PackOptions) -> Vec<PackIssue> =
        |root, game| super::check::check_pack(root, game, &options);

    /// Reads a pack and says what an import would do: the mods it has, the ones to get, and the profiles it updates.
    query inspect_pack(path: String) -> PackPreview =
        |root, game| super::inspect::inspect_pack(root, game, Path::new(&path));

    /// Imports a pack: the archives inside it, then its profiles, made new or updated from an earlier version.
    query import_pack(path: String) -> PackImportView =
        |root, game| super::import::import_pack(root, game, Path::new(&path));
}

/// Imports a pack with progress and a cancel flag. The command owns the flag; `cancel_pack_import` sets it.
#[tauri::command]
#[specta::specta]
pub async fn import_pack_with_progress(
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    path: String,
    progress: Channel<ImportProgress>,
) -> Result<PackImportView, AppError> {
    let key = TaskKey::pack_import(&game, &path);
    run_task(
        data,
        &tasks,
        key.as_str().to_string(),
        game,
        kind::PACK,
        move |root, game, flag, handle| {
            let mut send = |stage, percent| {
                let _ = progress.send(ImportProgress { stage, percent });
                handle.progress(percent);
            };
            super::import::import_pack_with_progress(
                root,
                game,
                Path::new(&path),
                &mut send,
                Some(flag),
            )
        },
    )
    .await
}

/// Marks a running pack import or download for cancellation. It stops between batches; a committed batch stays in the
/// library.
#[tauri::command]
#[specta::specta]
pub fn cancel_pack_import(tasks: State<'_, TaskFlags>, game: String, path: String) {
    tasks.cancel(TaskKey::pack_import(&game, &path).as_str());
}

/// Stops a running pack mod download at the next batch. The download and the import it runs share one flag, so the
/// whole operation stops; a separate pack import of the same file keeps running.
#[tauri::command]
#[specta::specta]
pub fn cancel_pack_download(tasks: State<'_, TaskFlags>, game: String, path: String) {
    tasks.cancel(TaskKey::pack_download(&game, &path).as_str());
}

/// Downloads the mods of a pack that the library lacks (premium accounts only), then imports the pack. Its own task
/// key lets `cancel_pack_download` stop the download and the batches that follow it as one operation.
#[tauri::command]
#[specta::specta]
pub async fn download_pack_mods(
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    path: String,
    progress: Channel<PackDownload>,
) -> Result<PackImportView, AppError> {
    let key = TaskKey::pack_download(&game, &path);
    run_task(
        data,
        &tasks,
        key.as_str().to_string(),
        game,
        kind::PACK,
        move |root, game, flag, handle| {
            super::download::download_pack_mods(
                root,
                game,
                Path::new(&path),
                &mut channel_sink_with_progress(progress, handle, |step: &PackDownload| {
                    step.percent
                }),
                Some(flag),
            )
        },
    )
    .await
}

/// The pack link that arrived before the window listened, so a second start does not lose it.
#[derive(Default)]
pub struct PackLinkPending(pub Mutex<Option<String>>);

/// The event that carries a `gobbo://pack` link to the window.
pub const PACK_LINK_EVENT: &str = "pack-link";

/// Keeps a pack link and tells the window. A provider link never reaches this function.
pub fn queue_pack_link(app: &tauri::AppHandle, link: &str) {
    if let Some(state) = app.try_state::<PackLinkPending>() {
        *state.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(link.to_string());
    }

    let _ = app.emit(PACK_LINK_EVENT, link.to_string());
}

/// Takes the pending pack link, if a deep link arrived before the window asked for it.
#[tauri::command]
#[specta::specta]
pub fn take_pack_link(pending: State<'_, PackLinkPending>) -> Option<String> {
    pending
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

/// Downloads a pack link after checking its hash, so the existing pack preview can inspect the file.
#[tauri::command]
#[specta::specta]
pub async fn download_pack_link(
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    link: String,
    progress: Channel<PackLinkProgress>,
) -> Result<PackLinkDownload, AppError> {
    let key = TaskKey::pack_link(&game, &link);
    run_task(
        data,
        &tasks,
        key.as_str().to_string(),
        game,
        kind::PACK_LINK,
        move |root, game, flag, handle| {
            super::link::download(
                root,
                game,
                &link,
                &mut channel_sink_with_progress(progress, handle, |step: &PackLinkProgress| {
                    step.percent
                }),
                Some(flag),
            )
        },
    )
    .await
}

/// Stops a pack link download at the next chunk.
#[tauri::command]
#[specta::specta]
pub fn cancel_pack_link(tasks: State<'_, TaskFlags>, game: String, link: String) {
    tasks.cancel(TaskKey::pack_link(&game, &link).as_str());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::tasks::cancelled;

    #[test]
    fn pack_import_and_download_have_distinct_task_keys() {
        let import = TaskKey::pack_import("game-a", "recipe.zip");
        let download = TaskKey::pack_download("game-a", "recipe.zip");

        assert_eq!(import.as_str(), "pack:game-a\nrecipe.zip");
        assert_eq!(download.as_str(), "pack-download:game-a\nrecipe.zip");
        assert_ne!(import.as_str(), download.as_str());
        assert_eq!(import.dedupe(), download.dedupe());
    }

    #[test]
    fn cancelling_a_download_key_leaves_an_import_of_the_same_file_running() {
        let tasks = TaskFlags::default();
        let import_key = TaskKey::pack_import("game-a", "recipe.zip");
        let download_key = TaskKey::pack_download("game-a", "recipe.zip");
        let import_flag = tasks.start(import_key, None, kind::PACK);
        let download_flag = tasks.start(
            TaskKey::pack_download("game-a", "recipe.zip"),
            None,
            kind::PACK,
        );

        tasks.cancel(download_key.as_str());

        assert!(!cancelled(Some(import_flag.as_ref())));
        assert!(cancelled(Some(download_flag.as_ref())));
    }
}
