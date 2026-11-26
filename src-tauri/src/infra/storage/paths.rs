//! The folders and files the storage decision looks at.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppError;

use super::{APP_ID, DECISION_FILE, PORTABLE_DATA_DIR, PORTABLE_MARKER};

/// The folder of the running executable.
pub fn exe_dir() -> Result<PathBuf, AppError> {
    let exe = std::env::current_exe().map_err(AppError::unexpected)?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| AppError::unexpected("the executable has no folder"))
}

/// The global data folder, the same one Tauri's `app_data_dir` uses when no override is set.
pub fn global_root() -> Result<PathBuf, AppError> {
    dirs::data_dir()
        .map(|dir| dir.join(APP_ID))
        .ok_or_else(|| AppError::user("settings.appdata_missing"))
}

/// The portable data folder next to the executable.
pub fn portable_root(exe_dir: &Path) -> PathBuf {
    exe_dir.join(PORTABLE_DATA_DIR)
}

/// The decision file next to the executable.
pub fn local_decision_file(exe_dir: &Path) -> PathBuf {
    exe_dir.join(DECISION_FILE)
}

/// The decision file in the global data folder.
pub fn global_decision_file(global_root: &Path) -> PathBuf {
    global_root.join(DECISION_FILE)
}

/// The marker shipped in the portable archive.
pub fn portable_marker(exe_dir: &Path) -> PathBuf {
    exe_dir.join(PORTABLE_MARKER)
}

/// `GOBBO_DATA_DIR`, made absolute so the GUI and the command line agree on the same folder.
pub fn env_override() -> Option<PathBuf> {
    std::env::var_os("GOBBO_DATA_DIR")
        .filter(|dir| !dir.is_empty())
        .map(|dir| {
            let path = PathBuf::from(dir);
            std::path::absolute(&path).unwrap_or(path)
        })
}

/// Whether the global folder holds an installation from a version before the choice existed. The per-game data lives
/// under `games/`; `settings.json` and the old `games.json` are the app-level files. The global folder itself is not a
/// decision: an empty folder is not data.
pub fn has_existing_data(root: &Path) -> bool {
    ["settings.json", "games.json", "games"]
        .iter()
        .any(|name| root.join(name).exists())
}

/// Whether a new file can be created in `dir`. It writes and removes a tiny probe file, so the answer does not depend
/// on platform-specific permission APIs. The probe is only asked when the first start needs it.
pub fn can_write(dir: &Path) -> bool {
    let probe = dir.join(format!(
        ".gobbo-write-{}.tmp",
        crate::infra::store::unique_suffix()
    ));
    let Ok(file) = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    else {
        return false;
    };
    drop(file);
    let _ = fs::remove_file(&probe);
    true
}
