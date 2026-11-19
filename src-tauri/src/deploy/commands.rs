//! Deployment: the plan preview, applying the active profile to the game and undoing it.

use tauri::State;
use tauri::ipc::Channel;

use super::ApplyView;
use super::DeployProgress;
use super::preview::DeployPreview;
use crate::commands::{channel_sink_with_progress, run_task};
use crate::context::Data;
use crate::error::AppError;
use crate::error::Message;
use crate::infra::tasks::{TaskFlags, TaskKey, kind};

/// Applies the active profile to the game. The progress channel receives the phase and the number of files written,
/// so the window can show the real percent.
#[tauri::command]
#[specta::specta]
pub async fn apply(
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    progress: Channel<DeployProgress>,
) -> Result<ApplyView, AppError> {
    let key = TaskKey::apply(&game);
    run_task(
        data,
        &tasks,
        key.as_str().to_string(),
        game,
        kind::APPLY,
        move |root, game, flag, handle| {
            let mut send =
                channel_sink_with_progress(progress, handle, |step: &DeployProgress| step.percent);
            super::ops::apply_deployment_cancellable(root, game, &mut send, Some(flag))
        },
    )
    .await
}

/// Stops a deployment at the next safe boundary before the file writes. A deployment already in progress is not cut
/// in half, so it either applies fully or stays untouched.
#[tauri::command]
#[specta::specta]
pub fn cancel_apply(tasks: State<'_, TaskFlags>, game: String) {
    tasks.cancel(TaskKey::apply(&game).as_str());
}

crate::commands::game_commands! {
    /// What a deployment of the active profile would do, before it does anything.
    query preview_deployment() -> DeployPreview = super::preview::preview_deployment;

    /// Every shared file of the active profile, its winner, its losers and the size of the winning copy.
    query conflict_explorer() -> super::explorer::ConflictExplorer = super::explorer::conflicts;

    /// One copy of a conflicting file, as a data URL when the window can show its format.
    query conflict_file_preview(path: String, family: String) -> super::explorer::ConflictPreview =
        |root, game| super::explorer::preview(root, game, &path, &family);

    /// Takes the deployment of the active profile out of the game. The answer lists what could not be undone.
    query undo() -> Vec<Message> = super::ops::undo_deployment;
}
