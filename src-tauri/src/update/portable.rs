//! The portable update use case: download the signed archive, swap the program files and start the new copy. The data
//! folder is never touched.

use tauri::AppHandle;
use tauri::ipc::Channel;
use tauri_plugin_updater::{Updater, UpdaterExt};

use crate::error::AppError;
use crate::infra::storage;
use crate::update::archive;
use crate::update::commands::PortableUpdateStep;
use crate::update::{portable_target, relaunch, swap};

/// Downloads the newer portable version, checks its signature, replaces the program files and starts the new copy.
/// The data folder is not touched. The command returns only when it fails: on success the app restarts.
pub async fn install(
    app: &AppHandle,
    progress: Channel<PortableUpdateStep>,
) -> Result<(), AppError> {
    if crate::context::running::any_started_here_and_running() {
        return Err(crate::error::game_running());
    }
    let update = updater(app)?
        .check()
        .await
        .map_err(update_error)?
        .ok_or_else(|| AppError::user("update.not_available"))?;

    let mut downloaded: u64 = 0;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let percent = total.map(|total| {
                    let percent = crate::domain::progress::percent(downloaded, total).unwrap_or(0);
                    u8::try_from(percent).unwrap_or(100)
                });
                let _ = progress.send(PortableUpdateStep::Downloading { percent });
            },
            || {},
        )
        .await
        .map_err(update_error)?;

    let _ = progress.send(PortableUpdateStep::Installing);
    let exe = std::env::current_exe().map_err(AppError::unexpected)?;
    let exe_dir = storage::exe_dir()?;
    tauri::async_runtime::spawn_blocking(move || swap_in(&exe_dir, &bytes))
        .await
        .map_err(AppError::unexpected)??;

    relaunch::spawn_after_exit(&exe)?;
    app.exit(0);
    Ok(())
}

/// Unpacks the verified archive and swaps it in. The unpacking folder is removed in every case.
fn swap_in(exe_dir: &std::path::Path, bytes: &[u8]) -> Result<(), AppError> {
    let staging = exe_dir.join(swap::STAGING_DIR);
    let _ = std::fs::remove_dir_all(&staging);
    let result = archive::extract(bytes, &staging)
        .and_then(|()| swap::replace_program_files(exe_dir, &staging).map(|_| ()));
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// The updater for the portable archive: the manifest key is the portable one, not the installer's.
pub(super) fn updater(app: &AppHandle) -> Result<Updater, AppError> {
    let target = portable_target(std::env::consts::OS, std::env::consts::ARCH)
        .ok_or_else(|| AppError::user("update.unsupported"))?;
    app.updater_builder()
        .target(target)
        .build()
        .map_err(update_error)
}

/// A failed plugin call is a temporary update failure, not a bug in Gobbo.
pub(super) fn update_error(error: tauri_plugin_updater::Error) -> AppError {
    AppError::retryable("update.failed").with("detail", error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_swap_leaves_no_unpacking_folder_behind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bytes = b"not a zip".to_vec();
        assert!(swap_in(dir.path(), &bytes).is_err());
        assert!(!dir.path().join(swap::STAGING_DIR).exists());
    }
}
