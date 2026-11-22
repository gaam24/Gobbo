//! Deployment: the overlay layer. The layer is built from hardlinks to the library (a copy when the two places cannot
//! share a file); the game folder stays untouched. Undo still reads a journal written by an old link deployment, so a
//! game deployed by an older Gobbo can be cleaned up once.
//! The module owns the engine and the use cases around it: commands, preview, target building and overwrite.
//!
//! Every write goes to the journal (`journal.jsonl` in the state folder) first, and only then to disk.
//! `undo` reverts exactly what the journal recorded, so it also works after an interrupted deployment.
//! Files and folders that Gobbo did not create are left alone.

mod apply;
mod error;
#[cfg(test)]
mod plan;
mod progress;
mod undo;

pub mod journal;
pub mod overlay;
pub mod scan;
pub mod settings;

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::user_data::Set as UserDataSet;

pub use crate::domain::resolver::plan_with_mode;
#[cfg(any(test, feature = "bench"))]
pub use apply::apply;
pub use apply::apply_with_progress;
#[cfg(test)]
pub use apply::write_settings;
pub use error::DeployError;
use journal::Journal;
#[cfg(test)]
pub use plan::plan_from_disk;
#[cfg(test)]
pub use progress::fail_after;
pub use undo::undo;

#[cfg(test)]
use crate::domain::resolver::{DeployPlan, SettingsRow};
#[cfg(test)]
use crate::games::Deploy;
#[cfg(test)]
use journal::Entry;

use crate::context::data::JOURNAL_FILE;

/// Whether a prepared state holds an overlay layer. A missing journal, or one written by an old deployment that placed
/// files in the game folder, is not an overlay session; undo uses this to guard a running game.
pub fn state_is_overlay(state: &Path) -> bool {
    Journal::new(state.join(JOURNAL_FILE))
        .has_layer_prepared()
        .unwrap_or(false)
}

/// Whether the running game keeps `profile`'s deployment in `state` from being undone. Only an overlay layer that the
/// game was not started from can go while it runs; files placed in the game folder, or the layer of the last launch,
/// stay until it exits.
pub fn undo_blocked_while_running(
    state: &Path,
    profile: &str,
    last_launched: Option<&str>,
) -> bool {
    !state_is_overlay(state) || last_launched == Some(profile)
}

/// Places in the game and in the state folder that Gobbo writes to.
pub struct Target {
    /// The deployment root. The destinations in the plan are relative to it.
    pub root: PathBuf,
    /// The game folder, even when `root` points elsewhere (for example `%APPDATA%`). The overlay mounts `root` and
    /// starts the program from this folder.
    pub game_root: PathBuf,
    /// The game's order file, for example `mods.settings`. `None` when the game does not use one.
    pub settings: Option<PathBuf>,
    /// The Gobbo state folder: the journal and the overlay layer.
    pub state: PathBuf,
    /// The user-data sets of the active profile. Empty for a game that declares none. The overlay uses the redirect
    /// sets; the copy sets are handled around the launch.
    pub user_data: Vec<UserDataSet>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub files: usize,
}

/// Where a deployment root lies inside a game folder: empty for the game folder itself, `Data` for `{game}/Data`. `None`
/// for a root outside the game. Links are resolved when the paths exist, so a symlinked game folder does not look like
/// another place. The mod layer and the mounted view both follow this one rule.
pub fn root_in_game(root: &Path, game_root: &Path) -> Option<PathBuf> {
    let inside = |root: &Path, game: &Path| root.strip_prefix(game).ok().map(Path::to_path_buf);
    inside(root, game_root).or_else(|| {
        let (root, game) = (
            fs::canonicalize(root).ok()?,
            fs::canonicalize(game_root).ok()?,
        );
        inside(&root, &game)
    })
}

impl Target {
    /// Where the deployment root lies inside the game folder: empty for the game folder itself, `Data` for
    /// `{game}/Data`. `None` for a root outside the game. Links are resolved when the paths exist, so a symlinked game
    /// folder does not look like another place.
    pub fn root_in_game(&self) -> Option<PathBuf> {
        root_in_game(&self.root, &self.game_root)
    }

    /// The journal roots this deployment may touch. Undo refuses an entry outside them, because a journal is a file
    /// anyone can edit.
    pub fn journal_roots(&self) -> journal::JournalRoots {
        journal::JournalRoots::new(self.state.clone(), self.root.clone(), self.settings.clone())
    }
}

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------------------------
// The use cases and commands of this module.
// ---------------------------------------------------------------------------------------------

pub mod commands;
pub mod explorer;
pub mod model;
pub mod ops;
pub mod overwrite;
pub mod preview;
pub mod remove;
pub mod target;

pub use model::{ApplyView, DeployProgress, DeployStage};
