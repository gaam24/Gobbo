//! The commands the window uses to keep the native menu in step with the interface (its translated labels and the game
//! and busy state only the window knows), and to learn whether the system shows a tray icon at all.

use tauri::{AppHandle, State};

use crate::error::AppError;

use super::labels::TrayLabels;
use super::state::TrayState;

/// Replaces the tray's words after the interface language changes.
#[tauri::command]
#[specta::specta]
pub async fn set_tray_labels(
    app: AppHandle,
    tray: State<'_, TrayState>,
    labels: TrayLabels,
) -> Result<(), AppError> {
    tray.set_labels(labels);
    super::refresh(&app)
}

/// Tells the tray which game the window shows and what it is busy with. The tray keeps the last value when the window
/// closes, so its status remains meaningful while work continues in the background.
#[tauri::command]
#[specta::specta]
pub async fn set_tray_context(
    app: AppHandle,
    tray: State<'_, TrayState>,
    game: Option<String>,
    busy: Option<String>,
) -> Result<(), AppError> {
    tray.set_context(game, busy);
    super::refresh(&app)
}

/// Whether the system shows Gobbo's tray icon. Without one the interface does not offer hiding the window in the tray.
#[tauri::command]
#[specta::specta]
pub async fn tray_available(app: AppHandle) -> bool {
    super::close::available(&app)
}
