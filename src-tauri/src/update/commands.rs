//! The update commands. The window asks which channel this copy uses, then asks the channel's own path for a newer
//! version. A portable update reports its download and then restarts the app; the installer path stays in the plugin.

use tauri::AppHandle;
use tauri::ipc::Channel;

use crate::error::AppError;
use crate::infra::storage;
use crate::update::portable;
use crate::update::{UpdateChannel, channel};

/// A newer portable version.
pub use crate::update::model::PortableUpdate;

/// The steps of a portable update, in order.
pub use crate::update::model::PortableUpdateStep;

/// The update path of this copy.
#[tauri::command]
#[specta::specta]
pub fn update_channel() -> Result<UpdateChannel, AppError> {
    let exe_dir = storage::exe_dir()?;
    let marker = storage::portable_marker(&exe_dir).exists();
    Ok(channel(
        marker,
        std::env::consts::OS,
        std::env::consts::ARCH,
    ))
}

/// Looks for a newer portable version. The manifest is read through the updater plugin, with its own key.
#[tauri::command]
#[specta::specta]
pub async fn portable_update_check(app: AppHandle) -> Result<Option<PortableUpdate>, AppError> {
    let update = portable::updater(&app)?
        .check()
        .await
        .map_err(portable::update_error)?;
    Ok(update.map(|update| PortableUpdate {
        version: update.version,
        notes: update.body.unwrap_or_default(),
    }))
}

/// Downloads the newer portable version, checks its signature, replaces the program files and starts the new copy.
/// The data folder is not touched. The command returns only when it fails: on success the app restarts.
#[tauri::command]
#[specta::specta]
pub async fn portable_update_install(
    app: AppHandle,
    progress: Channel<PortableUpdateStep>,
) -> Result<(), AppError> {
    // The elevated token would write the program files with administrator rights. The update is a normal-mode action:
    // it can move the copy the user is running, so it must not run beside the UAC boundary.
    if crate::platform::is_elevated() {
        return Err(AppError::user("update.restart_normally"));
    }

    portable::install(&app, progress).await
}
