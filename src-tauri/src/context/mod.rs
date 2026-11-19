//! The shared application context: the data folder, the loads every feature starts with, and their caches. This is the
//! neutral layer between the features and the disk.

pub mod data;
pub mod library;
pub mod managed;
pub mod profile;
pub mod running;
pub mod settings;
pub mod state_lock;
pub mod target;

pub use data::{
    Data, Loaded, edit_family, edit_library, edit_library_clean, edit_profiles, idle_data,
    load_game, load_library, load_library_shared, load_profiles, load_stopped, open,
};
pub use settings::{Settings, load_settings, save_settings};
pub use state_lock::lock_state;

/// Whether the trimmed text names an existing folder.
pub fn is_dir(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty() && std::path::Path::new(path).is_dir()
}

/// A byte count as whole kilobytes, rounded up and capped for the interface.
pub fn size_kb(bytes: u64) -> u32 {
    u32::try_from(bytes.div_ceil(1024)).unwrap_or(u32::MAX)
}
