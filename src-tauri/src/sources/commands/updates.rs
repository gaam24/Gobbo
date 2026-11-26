use tauri::State;
use tauri::ipc::Channel;

use crate::commands::helpers::{channel_sink_with_progress, run_task};
use crate::context::Data;
use crate::error::AppError;
use crate::infra::tasks::{TaskFlags, TaskKey, kind};
use crate::sources::update::{UpdateAllProgress, UpdateAllView, UpdateProgress};
use crate::status::views::Status;

/// Downloads the newest file of one source, installs it and selects it in the active profile. The channel carries the
/// download and import progress; the answer is the fresh status.
#[tauri::command]
#[specta::specta]
pub async fn update_mod(
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    id: String,
    provider: String,
    progress: Channel<UpdateProgress>,
) -> Result<Status, AppError> {
    let key = TaskKey::update_mod(&game, &id);
    run_task(
        data,
        &tasks,
        key,
        game,
        kind::UPDATE,
        move |root, game, flag, handle| {
            let mut send =
                channel_sink_with_progress(progress, handle, |step: &UpdateProgress| step.percent);
            super::update::update_mod_cancellable(
                root,
                game,
                &id,
                &provider,
                Some(flag),
                &mut send,
            )?;
            crate::status::status(root, game)
        },
    )
    .await
}

/// Updates every mod with a saved newer version, or only the mods `ids` names, one after another. It ends with the fresh
/// status and the failures.
#[tauri::command]
#[specta::specta]
pub async fn update_all(
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    ids: Option<Vec<String>>,
    progress: Channel<UpdateAllProgress>,
) -> Result<UpdateAllView, AppError> {
    let key = TaskKey::update_all(&game);
    run_task(
        data,
        &tasks,
        key,
        game,
        kind::UPDATE_ALL,
        move |root, game, flag, handle| {
            let mut send =
                channel_sink_with_progress(progress, handle, |step: &UpdateAllProgress| {
                    step.percent
                });
            super::update::update_all(root, game, ids.as_deref(), Some(flag), &mut send)
        },
    )
    .await
}

/// Stops the bulk update at the next mod, or in the middle of the download of the current one. The mods already
/// updated stay updated.
#[tauri::command]
#[specta::specta]
pub fn cancel_update_all(tasks: State<'_, TaskFlags>, game: String) {
    tasks.cancel(TaskKey::update_all(&game).as_str());
}
