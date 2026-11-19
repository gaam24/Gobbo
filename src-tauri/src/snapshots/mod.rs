//! Named profile snapshots: the editable settings of a profile, the data it keeps for the game, and its overwrite layer,
//! with a manifest that records the size and the SHA-256 of every file. A snapshot is taken before a profile switch,
//! before a profile is deleted, before another profile's data replaces it, before a restore, and on demand from the game
//! menu.
//!
//! A snapshot never touches the real save folder. It reads the profile's store: the writable layer of a redirected set,
//! the copy of a copied set, the legacy per-profile data folder, and `state/<profile>/overwrite`. A restore writes those
//! stores back, restores the list as a draft, keeps one fresh backup first, and materializes the data for the profile
//! that currently owns it. A profile deleted after its `BeforeDelete` snapshot is created again.
//!
//! `manifest.rs` holds the manifest and its folder helpers, `capture.rs` taking a snapshot, `restore.rs` the
//! preview, restore and materialization, and `view.rs` the types the window reads. This file keeps the scope, the
//! path rules and the public entry points.

use std::fs;
use std::path::{Component, Path, PathBuf};

use capture::create_in;
use manifest::{LEGACY_ID, SETS_DIR};

use crate::context::Data;
use crate::context::Settings;
use crate::context::load_profiles;
use crate::context::profile::data as profile_data;
use crate::context::profile::state as profile_state;
use crate::context::profile::user_data;
use crate::context::running::game_running_at;
use crate::domain::naming;
use crate::domain::profiles::Profile;
use crate::error::{AppError, io_at};
use crate::games::GameDef;

mod capture;
pub mod commands;
mod manifest;
mod restore;
#[cfg(test)]
mod test_support;
mod view;

pub use capture::create;
pub use manifest::{SnapshotKind, list};
pub use restore::{preview, restore};
pub use view::{RestoreOutcome, RestorePreview, SnapshotView};

/// The two data folders a snapshot touches. A snapshot is stored in the app's folder
/// (`<app data>/snapshots/<game>/<id>`), while the profiles, their state and their user data live in the game's
/// folder. Both are [`Data`], so this type holds them together and no call can pass the wrong one.
struct SnapshotScope {
    game_id: String,
    /// The app data folder: `<app data>/snapshots/<game>/<id>` lives here.
    app: Data,
    /// The folder of the game: profiles, profile state and user data.
    game: Data,
}

impl SnapshotScope {
    /// The scope of `game_id` under the app's data folder.
    fn new(app: &Data, game_id: &str) -> Self {
        Self {
            game_id: game_id.to_string(),
            app: app.clone(),
            game: app.game(game_id),
        }
    }

    /// The folder with every snapshot of the game.
    fn root(&self) -> PathBuf {
        self.app.dir.join("snapshots").join(&self.game_id)
    }

    /// The folder of one snapshot.
    fn dir(&self, id: &str) -> PathBuf {
        self.root().join(id)
    }
}

/// The scope of `game_id`, after refusing an id that is not one plain folder name.
fn scope(app: &Data, game_id: &str) -> Result<SnapshotScope, AppError> {
    check_game_id(game_id)?;
    Ok(SnapshotScope::new(app, game_id))
}

/// Every snapshot entry reaches a folder named by the game id, so a path is refused before it touches the disk.
fn check_game_id(game_id: &str) -> Result<(), AppError> {
    if naming::is_plain_folder(game_id) {
        Ok(())
    } else {
        Err(AppError::user("snapshot.not_found"))
    }
}

/// Removes a snapshot folder. A name that is not a plain folder is refused, so a command cannot reach outside the
/// game's snapshot folder.
pub fn delete(app: &Data, game_id: &str, id: &str) -> Result<(), AppError> {
    let scope = scope(app, game_id)?;
    if !naming::is_plain_folder(id) {
        return Err(AppError::user("snapshot.not_found"));
    }
    let dir = scope.dir(id);
    if !dir.is_dir() {
        return Err(AppError::user("snapshot.not_found"));
    }
    fs::remove_dir_all(&dir).map_err(io_at(&dir))
}

/// A snapshot taken before the data owner changes. `app` is the app's data folder; the profile stores are read from
/// the game's data folder. The current real files are flushed into the stores first, so the snapshot holds what the
/// game wrote. A profile with nothing stored has nothing to protect.
pub fn before_switch(
    app: &Data,
    game: &GameDef,
    settings: &Settings,
    target: &Profile,
) -> Result<(), AppError> {
    let scope = scope(app, &game.id)?;
    let current = profile_data::owner(&scope.game)?.or(user_data::owner(&scope.game)?);
    let Some(current) = current.filter(|current| current != &target.id) else {
        return Ok(());
    };
    // A state file may still name a profile that was removed while the app was not looking. It has no store to save.
    if load_profiles(&scope.game)?.get(&current).is_err() {
        return Ok(());
    }
    profile_state::capture_current(&scope.game, game, settings)?;
    create_in(&scope, game, settings, &current, SnapshotKind::BeforeSwitch)?;
    Ok(())
}

/// A snapshot taken before another profile's data replaces this profile's data. `app` is the app's data folder; the
/// stores are read from the game's data folder.
pub fn before_copy(
    app: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &Profile,
) -> Result<(), AppError> {
    let scope = scope(app, &game.id)?;
    create_in(
        &scope,
        game,
        settings,
        &profile.id,
        SnapshotKind::BeforeCopy,
    )?;
    Ok(())
}

/// A snapshot taken before a profile is removed. `app` is the app's data folder; the stores are read from the game's
/// data folder. The caller has already switched the data away, so the stores are the last state of that profile.
pub fn before_delete(
    app: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &str,
) -> Result<(), AppError> {
    let scope = scope(app, &game.id)?;
    create_in(&scope, game, settings, profile, SnapshotKind::BeforeDelete)?;
    Ok(())
}

/// The public command for a manual snapshot: the game is loaded here and a running game is refused, because its save
/// files could change while they are read.
pub fn create_manual(app: &Data, game_id: &str, profile: &str) -> Result<SnapshotView, AppError> {
    check_game_id(game_id)?;
    let game = crate::games::load(game_id)?;
    let settings = crate::context::load_settings(&app.game(game_id));
    if game_running_at(game_id, &settings.game_root) {
        return Err(
            AppError::user("snapshot.game_running").recovery(crate::error::Recovery::new(
                crate::error::RecoveryAction::CloseGame,
            )),
        );
    }
    create(app, &game, &settings, profile, SnapshotKind::Manual)
}

/// The public preview: the game is loaded from its id.
pub fn preview_public(app: &Data, game_id: &str, id: &str) -> Result<RestorePreview, AppError> {
    check_game_id(game_id)?;
    let game = crate::games::load(game_id)?;
    let settings = crate::context::load_settings(&app.game(game_id));
    preview(app, &game, &settings, id)
}

/// The public restore: the game is loaded from its id.
pub fn restore_public(app: &Data, game_id: &str, id: &str) -> Result<RestoreOutcome, AppError> {
    check_game_id(game_id)?;
    let _state = crate::context::lock_state(&app.dir)?;
    let game = crate::games::load(game_id)?;
    let settings = crate::context::load_settings(&app.game(game_id));
    restore(app, &game, &settings, id)
}

/// A manifest path must stay below its set folder. A file edited by hand must not make a restore write elsewhere.
fn relative_path(path: &str) -> Result<PathBuf, AppError> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppError::user("snapshot.corrupt").with("file", path.display()));
    }
    Ok(path.to_path_buf())
}

/// The folder of one set inside a snapshot. The legacy store keeps a fixed name; every other set is named by its hash,
/// so a set id cannot reach outside the snapshot.
fn set_dir_in(snapshot_dir: &Path, set_id: &str) -> PathBuf {
    let name = if set_id == LEGACY_ID {
        LEGACY_ID.to_string()
    } else {
        naming::path_hash(set_id)
    };
    snapshot_dir.join(SETS_DIR).join(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshots::test_support::fixture;

    #[test]
    fn a_manifest_path_cannot_leave_its_set_folder() {
        assert!(relative_path("saves/slot.sav").is_ok());
        assert!(relative_path("../outside.sav").is_err());
        assert!(relative_path("/outside.sav").is_err());
        assert!(relative_path("").is_err());
    }

    #[test]
    fn a_game_id_that_is_a_path_reaches_no_snapshot_folder() {
        let (app, game, settings) = fixture("snapshot-game-id");
        assert!(list(&app, "../slot").is_empty());
        assert_eq!(
            delete(&app, "../slot", "1-other").unwrap_err().code,
            "snapshot.not_found"
        );

        let mut bad = game;
        bad.id = "../slot".to_string();
        assert_eq!(
            create(&app, &bad, &settings, "p1", SnapshotKind::Manual)
                .unwrap_err()
                .code,
            "snapshot.not_found"
        );
        assert_eq!(
            preview(&app, &bad, &settings, "1-other").unwrap_err().code,
            "snapshot.not_found"
        );
        assert_eq!(
            restore(&app, &bad, &settings, "1-other").unwrap_err().code,
            "snapshot.not_found"
        );
    }
}
