//! The IPC types of the update commands: the newer portable version and the steps of installing it. They live here so
//! the command and the portable use case both depend on one lower module instead of on each other.

use serde::Serialize;

/// A newer portable version.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PortableUpdate {
    /// The version the manifest names.
    pub version: String,
    /// The release notes of the version, when the manifest has them.
    pub notes: String,
}

/// The steps of a portable update, in order.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "step", rename_all = "lowercase")]
pub enum PortableUpdateStep {
    /// The archive is downloading. `percent` is unknown when the server sends no length.
    Downloading {
        /// The share of the archive received, from 0 to 100.
        percent: Option<u8>,
    },
    /// The archive is verified and its files are being swapped.
    Installing,
}
