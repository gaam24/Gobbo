//! Path resolution shared by deployment, profiles and user data.

use std::path::{Path, PathBuf};

use crate::context::Data;
use crate::context::Settings;
use crate::error::AppError;
use crate::games::paths;
use crate::games::prefix::{effective_source, prefix_of};
use crate::games::runtime::Runtime;
use crate::games::{GAME_TEMPLATE, GameDef};

/// The game's order file, for example `mods.settings`, in the folder the user chose. `None` when the game has no
/// order file or the folder has not been chosen yet. An empty folder must never turn into a path at the root.
pub fn settings_file(game: &GameDef, settings: &Settings) -> Option<PathBuf> {
    let dir = settings.settings_dir.trim();
    let order = game.settings_order()?;
    (!dir.is_empty()).then(|| order.file_in(Path::new(dir)))
}

/// How this install starts on this computer. Every caller asks here, so launch, tools, steps and the deployment root
/// never disagree.
pub fn runtime(game: &GameDef, settings: &Settings) -> Runtime {
    crate::games::runtime::of(
        game,
        Path::new(settings.game_root.trim()),
        effective_source(&settings.source, &game.detect.steam_app_ids),
    )
}

/// The Wine prefix this install's paths resolve with, when it has one: the Steam prefix for a Steam install under
/// Proton, the chosen or owned prefix for another Windows install, none for a native install.
pub fn prefix(data: &Data, game: &GameDef, settings: &Settings) -> Option<PathBuf> {
    prefix_of(
        game,
        runtime(game, settings),
        Path::new(settings.game_root.trim()),
        &settings.prefix_dir,
        data,
    )
}

/// The values the game's path templates resolve with for these settings.
pub fn path_context(data: &Data, game: &GameDef, settings: &Settings) -> paths::Context {
    let dir = settings.settings_dir.trim();
    paths::Context {
        game_root: PathBuf::from(settings.game_root.trim()),
        prefix: prefix(data, game, settings),
        under_proton: runtime(game, settings) != Runtime::Native,
        home: paths::home(),
        settings: (!dir.is_empty()).then(|| PathBuf::from(dir)),
    }
}

/// The folder that holds the game's mods. A user override wins; otherwise the definition's template is
/// resolved against the game folder and its Proton prefix.
pub fn deployment_root(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
) -> Result<PathBuf, AppError> {
    let override_path = settings.mods_root.trim();
    if !override_path.is_empty() {
        return Ok(PathBuf::from(override_path));
    }
    let context = path_context(data, game, settings);
    // `root_linux` is for a native Linux install only; on Windows the `root` template always applies.
    let native = !crate::platform::is_windows() && runtime(game, settings) == Runtime::Native;
    let template = game.root_template_for(native);
    if template.trim() == GAME_TEMPLATE {
        return Ok(context.game_root);
    }
    if template.contains(paths::PREFIX_VARIABLE) && context.prefix.is_none() {
        return Err(paths::prefix_missing());
    }
    context.resolve(template)
}
