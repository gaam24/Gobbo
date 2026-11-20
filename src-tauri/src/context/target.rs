//! Path resolution shared by deployment, profiles and user data.

use std::path::{Path, PathBuf};

use crate::context::Settings;
use crate::error::AppError;
use crate::games::paths;
use crate::games::{GAME_TEMPLATE, GameDef};

/// The game's order file, for example `mods.settings`, in the folder the user chose. `None` when the game has no
/// order file or the folder has not been chosen yet. An empty folder must never turn into a path at the root.
pub fn settings_file(game: &GameDef, settings: &Settings) -> Option<PathBuf> {
    let dir = settings.settings_dir.trim();
    let order = game.load_order.as_ref()?;
    (!dir.is_empty()).then(|| order.file_in(Path::new(dir)))
}

/// The values the game's path templates resolve with for these settings.
pub fn path_context(game: &GameDef, settings: &Settings) -> paths::Context {
    let dir = settings.settings_dir.trim();
    game.path_context(
        Path::new(settings.game_root.trim()),
        &paths::home(),
        (!dir.is_empty()).then(|| Path::new(dir)),
    )
}

/// The folder that holds the game's mods. A user override wins; otherwise the definition's template is
/// resolved against the game folder and its Proton prefix.
pub fn deployment_root(game: &GameDef, settings: &Settings) -> Result<PathBuf, AppError> {
    let override_path = settings.mods_root.trim();
    if !override_path.is_empty() {
        return Ok(PathBuf::from(override_path));
    }
    let context = path_context(game, settings);
    // `root_linux` is for a native Linux install only; on Windows the `root` template always applies.
    let native =
        !cfg!(windows) && (game.detect.steam_app_ids.is_empty() || context.prefix.is_none());
    let template = game.root_template_for(native);
    if template.trim() == GAME_TEMPLATE {
        return Ok(context.game_root);
    }
    if template.contains(paths::PREFIX_VARIABLE) && context.prefix.is_none() {
        return Err(paths::prefix_missing());
    }
    context.resolve(template)
}
