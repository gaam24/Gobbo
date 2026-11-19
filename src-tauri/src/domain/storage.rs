//! Where Gobbo keeps its data. The value is shared by the storage resolver and the settings view, so both name the
//! same modes.

use serde::{Deserialize, Serialize};

/// Where the running app keeps its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum StorageMode {
    /// Next to the program, in `<program folder>/data`.
    Portable,
    /// In the system's app data folder.
    Global,
    /// A folder named by `GOBBO_DATA_DIR`, for tests and advanced use.
    Custom,
}
