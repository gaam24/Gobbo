//! The values the storage decision returns to its callers.

use std::path::PathBuf;

use crate::domain::storage::StorageMode;

/// A resolved data folder, with the file that records the choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLocation {
    /// The mode this copy uses.
    pub mode: StorageMode,
    /// The data folder. Each game keeps its own folder below it.
    pub root: PathBuf,
    /// Empty for [`StorageMode::Custom`], which has no decision file.
    pub decision_file: PathBuf,
    /// Whether the portable archive's marker is next to the program.
    pub portable_marker: bool,
}

impl StorageLocation {
    /// The root to hand to Tauri's `appDirectoriesOverride`. The global mode keeps Tauri's own layout, so it needs no
    /// override; portable and custom put every app directory in one folder.
    pub fn tauri_root(&self) -> Option<PathBuf> {
        match self.mode {
            StorageMode::Portable | StorageMode::Custom => Some(self.root.clone()),
            StorageMode::Global => None,
        }
    }
}

/// What the start needs: a decision, or the facts for the first-start question.
#[derive(Debug, Clone)]
pub enum Resolved {
    /// A recorded choice or the environment override.
    Known(StorageLocation),
    /// No choice yet; the caller asks the first-start question.
    Choice(PromptContext),
}

/// The facts shown by the first-start question. Nothing is written until the user chooses.
#[derive(Debug, Clone)]
pub struct PromptContext {
    /// The mode the question recommends first.
    pub default_mode: StorageMode,
    /// The folder of the running program.
    pub exe_dir: PathBuf,
    /// `<exe_dir>/data`, the portable data folder.
    pub portable_root: PathBuf,
    /// The global app data folder, or `None` when the system does not provide one.
    pub global_root: Option<PathBuf>,
    /// Whether the global folder already holds an installation.
    pub global_data_exists: bool,
    /// Whether a new file can be written next to the program.
    pub portable_available: bool,
    /// Whether the portable archive's marker is next to the program.
    pub portable_marker: bool,
    /// The decision file next to the program.
    pub local_decision_file: PathBuf,
    /// The decision file in the global folder, when that folder exists.
    pub global_decision_file: Option<PathBuf>,
}
