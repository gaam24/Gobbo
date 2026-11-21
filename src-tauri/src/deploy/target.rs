//! Where a game's mods go: the deployment target, built from the game definition and the settings.

pub use crate::context::target::{deployment_root, settings_file};

use crate::context::Data;
use crate::context::Settings;
use crate::context::is_dir;
use crate::context::profile::user_data;
use crate::deploy::Target;
use crate::deploy::journal::JournalRoots;
use crate::domain::profiles::Profile;
use crate::domain::resolver::{DeployPlan, ModInput, PlanMode};
use crate::domain::user_data::Set as UserDataSet;
use crate::error::{AppError, Recovery, RecoveryAction, game_folder_missing};
use crate::games::GameDef;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Which folders the game, its settings and its mods need are in place, and the deployment root the mods use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderReadiness {
    pub game: bool,
    pub settings: bool,
    pub mods: bool,
    pub mods_root: String,
}

impl FolderReadiness {
    /// Whether every folder the deployment needs exists.
    pub const fn all(&self) -> bool {
        self.game && self.settings && self.mods
    }
}

/// The one folder readiness rule. `settings_redirected` is true when an isolated user-data set owns the game's order
/// file, so a settings folder is not needed. `status` and `target_for` both ask this function.
pub fn folder_readiness(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    settings_redirected: bool,
) -> FolderReadiness {
    let mods_root = deployment_root(data, game, settings)
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    FolderReadiness {
        game: is_dir(&settings.game_root),
        settings: !game.has_load_order() || is_dir(&settings.settings_dir) || settings_redirected,
        mods: if settings.mods_root.trim().is_empty() {
            !game.has_external_root() || !mods_root.is_empty()
        } else {
            is_dir(&settings.mods_root)
        },
        mods_root,
    }
}

/// Deployment places from the settings. Refuses when folders are missing.
pub fn target_for(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    state: PathBuf,
    profile: &Profile,
) -> Result<Target, AppError> {
    if !is_dir(&settings.game_root) {
        return Err(game_folder_missing());
    }
    let user_data: Vec<UserDataSet> = user_data::sets(data, game, settings, profile, &state)?;
    let ready = folder_readiness(
        data,
        game,
        settings,
        settings_redirected(game, settings, &user_data),
    );
    if !ready.settings {
        return Err(AppError::user("settings.settings_folder_missing")
            .recovery(Recovery::new(RecoveryAction::SetSettingsFolder)));
    }
    if !ready.mods {
        return Err(AppError::user("settings.mods_folder_missing")
            .recovery(Recovery::new(RecoveryAction::SetModsFolder)));
    }
    let settings_path =
        settings_file(game, settings).map(|path| user_data::write_path(&user_data, &path));
    Ok(Target {
        root: deployment_root(data, game, settings)?,
        game_root: PathBuf::from(settings.game_root.trim()),
        settings: settings_path,
        state,
        user_data,
    })
}

/// The journal roots for one profile's state folder. It resolves only settings, not existing folders, so an undo still
/// works after the game folder or the settings folder was moved or removed.
pub fn journal_roots(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    state: PathBuf,
) -> Result<JournalRoots, AppError> {
    Ok(JournalRoots::new(
        state,
        deployment_root(data, game, settings)?,
        settings_file(game, settings),
    ))
}

/// Builds the plan in `Full` or `Summary` mode; the status and the preview use `Summary`.
pub fn build_plan_with_mode(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &Profile,
    winners: &BTreeMap<String, String>,
    inputs: &[ModInput],
    mode: PlanMode,
) -> Result<DeployPlan, AppError> {
    target_for(
        data,
        game,
        settings,
        data.profile_state(&profile.id),
        profile,
    )?;
    Ok(crate::deploy::plan_with_mode(
        &game.deploy,
        inputs,
        winners,
        mode,
    ))
}

/// Whether a user-data set redirects the game's settings file: the settings folder then does not have to exist.
pub fn settings_redirected(game: &GameDef, settings: &Settings, sets: &[UserDataSet]) -> bool {
    settings_file(game, settings).is_some_and(|file| user_data::is_redirected(sets, &file))
}

/// The settings point to folders that exist for `profile`, whose state folder is `state`. A settings folder that the
/// profile's user data redirects counts as ready, as it does for an apply. An automatically resolved mods root may be
/// created on deploy.
pub fn is_ready(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    profile: &Profile,
    state: &Path,
) -> bool {
    let sets = user_data::sets(data, game, settings, profile, state).unwrap_or_default();
    folder_readiness(
        data,
        game,
        settings,
        settings_redirected(game, settings, &sets),
    )
    .all()
        && deployment_root(data, game, settings).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::path::Path;

    /// Settings whose game folder and settings folder exist under `base`.
    fn folders(base: &Path) -> Settings {
        let game_root = base.join("game");
        let settings_dir = base.join("settings");
        std::fs::create_dir_all(&game_root).unwrap();
        std::fs::create_dir_all(&settings_dir).unwrap();
        Settings {
            game_root: game_root.display().to_string(),
            settings_dir: settings_dir.display().to_string(),
            ..Settings::default()
        }
    }

    #[test]
    fn a_redirected_order_file_makes_the_settings_folder_optional() {
        let base = scratch!("redirected-settings");
        let game = crate::test_support::games::slot();
        let mut settings = folders(&base);
        settings.settings_dir = base.join("missing-settings").display().to_string();
        let data = Data::new(base.join("data"));
        let without = folder_readiness(&data, &game, &settings, false);
        assert!(!without.settings);
        let redirected = folder_readiness(&data, &game, &settings, true);
        assert!(redirected.settings);
        assert!(without.game && redirected.game);
    }

    #[test]
    fn a_target_is_ready_only_when_its_folders_exist() {
        let base = scratch!("is-ready");
        let game = crate::test_support::games::slot();
        let settings = folders(&base);
        let profile = Profile::new("p1".into(), "Default".into(), Vec::new());
        let state = base.join("state");
        let data = Data::new(base.join("data"));
        let is_ready = |settings: &Settings| is_ready(&data, &game, settings, &profile, &state);
        assert!(is_ready(&settings));
        assert!(!is_ready(&Settings::default()));

        let no_game = Settings {
            game_root: base.join("no-game").display().to_string(),
            ..settings.clone()
        };
        assert!(!is_ready(&no_game));

        let no_settings = Settings {
            settings_dir: base.join("no-settings").display().to_string(),
            ..settings.clone()
        };
        assert!(!is_ready(&no_settings));

        let no_mods = Settings {
            mods_root: base.join("no-mods").display().to_string(),
            ..settings
        };
        assert!(!is_ready(&no_mods));
    }
}
