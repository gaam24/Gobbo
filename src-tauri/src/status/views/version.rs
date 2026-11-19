//! The installed-version view, for the version picker.

use serde::Serialize;

/// One installed version of a mod, for the version picker.
#[derive(Debug, Serialize, specta::Type)]
pub struct VersionView {
    pub folder: String,
    pub name: String,
    pub version: Option<String>,
    pub files: u32,
    pub size_kb: u32,
    pub archive: Option<String>,
    pub archive_file: Option<String>,
    pub installed_at: u32,
    /// Number of profiles that select this version.
    pub used_by: u32,
}
