//! Update checks through the provider port: one mod or the whole library, the page data each provider returns, and the
//! requirements it lists for its newest file. The feature chooses the providers and stores the generic results; the
//! provider owns its requests, batching and parsing.

mod bulk;
mod check;
mod install;
pub mod manual;
mod select;
mod store;

pub use bulk::update_all;
pub use check::{
    check_mod_update, check_updates, fill_source_info, ignore_version, unignore_version,
};
pub use install::download_and_import;
pub use install::update_mod_cancellable;
pub use manual::{
    MANUAL_PAGE_EVENT, ManualPageRequest, ManualStep, ManualTarget, ManualUpdateFinish,
    ManualUpdateSessions, ManualUpdateView,
};
pub use select::select_installed_update;
pub use store::apply_meta;

use crate::context::Data;
use crate::context::library::not_in_library;
use crate::domain::library::{Family, Library};
use crate::error::AppError;
use crate::games::{self, GameDef};

/// Runs `work` with the game, the library and the family it names, under one read.
pub(super) fn with_family<T>(
    root: &Data,
    game_id: &str,
    id: &str,
    work: impl FnOnce(&GameDef, &Library, &Family) -> Result<T, AppError>,
) -> Result<T, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = crate::context::load_library(&data, &game)?;
    let family = library.families.get(id).ok_or_else(|| not_in_library(id))?;
    work(&game, &library, family)
}

use serde::Serialize;

use crate::error::Message;
use crate::status::views::Status;

/// The phase of an in-app update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateStage {
    Downloading,
    Importing,
    Selecting,
}

/// One progress step of an in-app update, sent to the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub struct UpdateProgress {
    pub stage: UpdateStage,
    pub percent: u32,
}

impl UpdateProgress {
    fn at(stage: UpdateStage, done: u64, total: u64) -> Self {
        let unknown = if stage == UpdateStage::Selecting {
            100
        } else {
            0
        };
        let percent = crate::domain::progress::percent(done, total).unwrap_or(unknown);
        Self { stage, percent }
    }
}

/// The phase of an "update all" run. `Switching` is a mod whose newest version is installed already and only needs to
/// be selected in the profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateAllStage {
    Downloading,
    Importing,
    Selecting,
    Switching,
}

impl From<UpdateStage> for UpdateAllStage {
    fn from(stage: UpdateStage) -> Self {
        match stage {
            UpdateStage::Downloading => Self::Downloading,
            UpdateStage::Importing => Self::Importing,
            UpdateStage::Selecting => Self::Selecting,
        }
    }
}

/// One step of a bulk update: the mod, its place in the run, and the overall percentage.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct UpdateAllProgress {
    pub name: String,
    /// The index of the mod, from 0.
    pub done: u32,
    pub total: u32,
    pub stage: UpdateAllStage,
    pub percent: u32,
}

/// One mod a bulk update selected a new version for, with the folder it selected before.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct UpdateChange {
    pub id: String,
    pub name: String,
    pub previous: String,
}

/// What a bulk update did: the fresh status, how many mods it changed, the failures it kept going through, and the
/// previous version of every changed mod, so the window can offer one undo.
#[derive(Debug, Serialize, specta::Type)]
pub struct UpdateAllView {
    pub status: Status,
    pub updated: u32,
    pub failed: Vec<Message>,
    pub changes: Vec<UpdateChange>,
    /// Whether the user stopped the run before every target was done.
    pub cancelled: bool,
}

/// One mod of an "update all" run, decided before the first download so a failure cannot change the list halfway.
pub(super) struct UpdateTarget {
    id: String,
    name: String,
    provider: String,
    /// The installed latest version's folder, when the library already has it: the run only selects it.
    installed_folder: Option<String>,
    /// The folder the profile selected before the update.
    previous: String,
}

#[cfg(test)]
mod tests {
    use super::{UpdateProgress, UpdateStage};

    #[test]
    fn an_update_reports_the_download_percentage() {
        assert_eq!(
            UpdateProgress::at(UpdateStage::Downloading, 25, 100),
            UpdateProgress {
                stage: UpdateStage::Downloading,
                percent: 25,
            }
        );
    }

    #[test]
    fn a_total_is_never_divided_by_zero() {
        assert_eq!(
            UpdateProgress::at(UpdateStage::Downloading, 3, 0).percent,
            0
        );
        assert_eq!(
            UpdateProgress::at(UpdateStage::Selecting, 1, 1).percent,
            100
        );
        assert_eq!(
            UpdateProgress::at(UpdateStage::Selecting, 0, 0).percent,
            100
        );
    }

    #[test]
    fn a_total_smaller_than_the_done_part_stays_at_one_hundred() {
        assert_eq!(
            UpdateProgress::at(UpdateStage::Importing, 200, 100).percent,
            100
        );
    }
}
