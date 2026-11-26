//! The Tauri commands the window uses to read, stop and forget tasks.

use tauri::State;

use crate::error::AppError;

use super::model::TaskView;
use super::registry::TaskFlags;

/// Lists the tasks the window can see in the global Working panel. It answers with the same `CommandResult` shape as
/// the other read commands, so a reload can hydrate the panel through the shared IPC helper.
#[tauri::command]
#[specta::specta]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the IPC shape must be a CommandResult so the frontend can hydrate through its shared unwrap helper"
)]
pub fn list_tasks(tasks: State<'_, TaskFlags>) -> Result<Vec<TaskView>, AppError> {
    Ok(tasks.list())
}

/// Stops a task by the id it was registered with. The id is the key of the stored cancellation flag, so the specific
/// cancel commands and this generic one share the same mechanism.
#[tauri::command]
#[specta::specta]
pub fn cancel_task(tasks: State<'_, TaskFlags>, id: String) {
    tasks.cancel(&id);
}

/// Forgets a failed or waiting task the user dismissed in the Working panel.
#[tauri::command]
#[specta::specta]
pub fn dismiss_task(tasks: State<'_, TaskFlags>, id: String) {
    tasks.dismiss(&id);
}
