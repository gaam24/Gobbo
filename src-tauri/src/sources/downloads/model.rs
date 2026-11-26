//! What the downloads show: the state and the view of each one in the queue.

use crate::domain::source::SourceRef;
use crate::error::AppError;
use serde::Serialize;

/// The name a download shows when the provider has not named its file yet: the game key and the mod id, or just the mod
/// id when the link has no game key.
pub fn download_name(game_key: Option<&str>, mod_id: &str) -> String {
    game_key.map_or_else(|| mod_id.to_string(), |key| format!("{key} {mod_id}"))
}

/// Where a download is on its way into the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    /// Coming in from the provider.
    Downloading,
    /// In the downloads folder, waiting for its import.
    Ready,
    /// Going into the library.
    Installing,
    /// In the library. The downloaded file is gone.
    Installed,
    /// Stopped by the error the download carries.
    Failed,
}

/// One download of the queue, as the window shows it.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DownloadView {
    pub id: u32,
    /// The game it is for. Empty until the link is read, and when the link names a game that is not added.
    pub game: String,
    /// The file's name on the provider, or the mod of the link until the provider answers.
    pub name: String,
    /// The provider, mod and file the download comes from.
    pub source: Option<SourceRef>,
    pub state: DownloadState,
    pub received_kb: u32,
    /// The size, 0 while it is not known.
    pub total_kb: u32,
    /// The speed over the last moment, while the file comes in.
    pub speed_kb: u32,
    /// The downloaded archive, once it is whole.
    pub path: Option<String>,
    /// The archive has an installer and needs the window's FOMOD wizard. The backend keeps this wait with the queue, so
    /// the window still finds it after a restart; `Downloads::claim` clears it once the wizard is taken.
    pub auto_install: bool,
    /// The collection session that asked for this file, when one did. An owned download is not auto-installed by the
    /// generic queue; the owner imports it with the collection's FOMOD choices.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub error: Option<AppError>,
}

/// A link's download the backend imported on its own, for the window's notice and its fresh status.
#[derive(Debug, Serialize)]
pub struct DownloadImported {
    pub game: String,
    /// The download's file name.
    pub name: String,
    /// What the import added. `None` when the library already had the file.
    pub view: Option<crate::import::model::ImportView>,
    /// The library folder that already had this exact file, when the download was a duplicate.
    pub existing: Option<String>,
    /// The game's status after the profiles took the new mods.
    pub status: Box<crate::status::views::Status>,
}

impl DownloadView {
    pub(super) const fn new(name: String, state: DownloadState) -> Self {
        Self {
            id: 0,
            game: String::new(),
            name,
            source: None,
            state,
            received_kb: 0,
            total_kb: 0,
            speed_kb: 0,
            path: None,
            auto_install: false,
            owner: None,
            error: None,
        }
    }
}
