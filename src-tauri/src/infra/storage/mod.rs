//! Where Gobbo keeps its data: next to the program or in the global app data folder. The first start asks; the answer
//! is a small JSON file. A portable copy only writes next to its own executable, so moving the folder moves the data.

mod decision;
mod model;
mod paths;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub use decision::default_root;
pub use decision::{commit, resolve, root_for_cli};
pub use model::{PromptContext, Resolved, StorageLocation};
#[cfg(test)]
pub use paths::has_existing_data;
pub use paths::{
    can_write, env_override, exe_dir, global_decision_file, global_root, local_decision_file,
    portable_marker, portable_root,
};

/// The app identifier from `tauri.conf.json`. The global data folder is `dirs::data_dir()` with this name.
pub const APP_ID: &str = "dev.gobbo.modsorganizer";
/// The file that records the choice, next to the program or in the global data folder.
pub const DECISION_FILE: &str = "gobbo.storage.json";
/// The marker shipped only in the portable archive. It makes the first question recommend the portable folder.
pub const PORTABLE_MARKER: &str = "gobbo.portable";
/// The data folder inside the portable folder. It is not the program folder itself: the bundled `games/` definitions
/// would collide with the per-game data folder `games/<id>`.
pub const PORTABLE_DATA_DIR: &str = "data";

/// Whether a folder name is the portable data folder. An update never reads or writes it: it holds the user's library
/// and settings, not the program files the update replaces.
pub fn is_portable_data_dir(name: &str) -> bool {
    name == PORTABLE_DATA_DIR
}

/// The version of the decision file this build understands.
pub const DECISION_VERSION: u32 = 1;
