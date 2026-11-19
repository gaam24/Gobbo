//! Overlay diagnostics: run the in-session probe on the active profile.

use super::mounts::{self, MountDiagnostics};
use super::probe::{self, OverlayProbeReport};

crate::commands::game_commands! {
    /// Runs the x64 and x86 overlay probes in the active session and returns their checks.
    query run_overlay_probe() -> Vec<OverlayProbeReport> = probe::run_probe;

    /// The state folder of the active overlay session, for the "open folder" button.
    query overlay_state() -> Option<String> = |root, game| Ok(probe::state_path(root, game));

    /// The mounts of one game and the user-data layers no profile uses any more.
    query mount_diagnostics() -> MountDiagnostics = mounts::diagnose;

    /// Removes the orphaned layers the user confirmed, after checking they are still orphans.
    change clean_orphan_layers(paths: Vec<String>) = |root, game| mounts::clean(root, game, &paths);
}

/// Writes a support bundle with the log, version and, when a game is selected, its definition, setup check and last run.
#[tauri::command]
#[specta::specta]
pub async fn create_support_bundle(
    data: tauri::State<'_, crate::context::Data>,
    game: Option<String>,
) -> Result<String, crate::error::AppError> {
    crate::commands::with_data(data, move |root| {
        super::bundle::create(root, game.as_deref())
    })
    .await
}

/// Records a render error or an unhandled rejection from the webview in the same log as the backend.
#[tauri::command]
#[specta::specta]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the command keeps the typed IPC result so the webview can catch a failure"
)]
pub fn log_frontend_error(message: String, detail: String) -> Result<(), crate::error::AppError> {
    crate::infra::log::error(format!("frontend: {message}\n{detail}"));
    Ok(())
}
