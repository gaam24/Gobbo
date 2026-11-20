//! The storage choice as the settings window shows it. The data folder itself lives in `Data`; this module only names
//! the mode and the file that records the choice.

use std::path::Path;

use serde::Serialize;

use crate::context::Data;
use crate::infra::storage;

pub use crate::domain::storage::StorageMode;

/// The data folder, its mode, and the file that records the choice.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct StorageInfo {
    /// The mode the running copy uses.
    pub mode: StorageMode,
    /// The data folder as the interface shows it.
    pub root: String,
    /// Empty for a custom folder, and before a choice was written.
    pub decision_file: String,
}

fn mode_for(data: &Path, exe_dir: Option<&Path>, custom: bool) -> StorageMode {
    if custom {
        return StorageMode::Custom;
    }

    match exe_dir {
        Some(exe_dir)
            if crate::domain::pathkey::same_path(data, &storage::portable_root(exe_dir)) =>
        {
            StorageMode::Portable
        }
        _ => StorageMode::Global,
    }
}

/// Reads the mode from the running data folder. The decision file is reported only so the settings page can point at
/// the file whose removal asks the question again.
pub fn info(data: &Data) -> StorageInfo {
    let exe_dir = storage::exe_dir().ok();
    let custom = storage::env_override().is_some();
    let mode = mode_for(&data.dir, exe_dir.as_deref(), custom);
    let decision_file = match mode {
        StorageMode::Custom => String::new(),
        StorageMode::Portable => exe_dir
            .as_deref()
            .map(|dir| storage::local_decision_file(dir).display().to_string())
            .unwrap_or_default(),
        StorageMode::Global => {
            let local = exe_dir
                .as_deref()
                .map(storage::local_decision_file)
                .filter(|path| path.is_file());
            let global = storage::global_root()
                .ok()
                .map(|root| storage::global_decision_file(&root))
                .filter(|path| path.is_file());
            local
                .or(global)
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        }
    };
    StorageInfo {
        mode,
        root: data.dir.display().to_string(),
        decision_file,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn the_portable_folder_is_reported_as_portable() {
        let root = scratch!("service-storage-portable");
        let exe = root.join("program");
        let data = storage::portable_root(&exe);
        std::fs::create_dir_all(&data).unwrap();
        assert_eq!(mode_for(&data, Some(&exe), false), StorageMode::Portable);
    }

    #[test]
    fn any_other_folder_is_global() {
        let root = scratch!("service-storage-global");
        let exe = root.join("program");
        let data = root.join("global");
        assert_eq!(mode_for(&data, Some(&exe), false), StorageMode::Global);
    }

    #[test]
    fn an_environment_folder_is_custom() {
        let root = scratch!("service-storage-custom");
        let exe = root.join("program");
        assert_eq!(mode_for(&root, Some(&exe), true), StorageMode::Custom);
    }
}
