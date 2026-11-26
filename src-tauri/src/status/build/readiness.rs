//! Which folders the game, its settings and its mods need are in place.

use crate::context::Data;
use crate::context::Settings;
use crate::deploy::target::folder_readiness;
use crate::games::GameDef;

/// Which folders the game, its settings and its mods need are in place. The window asks for the ones that are not.
pub(super) struct Readiness {
    pub(super) game_ready: bool,
    pub(super) settings_ready: bool,
    pub(super) mods_ready: bool,
    pub(super) mods_root: String,
}

/// The folder checks that decide how ready the game, its settings and its mods are.
pub(super) fn readiness(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    settings_redirected: bool,
) -> Readiness {
    let ready = folder_readiness(data, game, settings, settings_redirected);
    Readiness {
        game_ready: ready.game,
        settings_ready: ready.settings,
        mods_ready: ready.mods,
        mods_root: ready.mods_root,
    }
}
