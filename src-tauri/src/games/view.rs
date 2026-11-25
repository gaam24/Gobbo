//! The game as the interface gets it. Only what the UI shows; the rest of the definition stays in the core.

use serde::Serialize;
use specta::Type;

use super::GameOrigin;
use super::Verified;
use super::definition::GameDef;

/// A game in the form the frontend gets.
#[derive(Debug, Serialize, Type)]
pub struct GameView {
    pub id: String,
    pub name: String,
    /// Whether the definition ships with Gobbo or the user added it.
    pub origin: GameOrigin,
    /// The test evidence from `[verified]`, when the definition has it.
    pub verified: Option<Verified>,
    pub steam_app_ids: Vec<u32>,
    /// Places in the game where mods will end up, for example `Mods` or `archive/pc/mod`.
    pub places: Vec<String>,
    /// The loader preset the game uses, when it names one.
    pub loader: Option<GameLoaderView>,
    /// Whether the game needs the folder of its order file (for example `mods.settings`).
    pub needs_settings_dir: bool,
    /// Whether the mods live outside the game folder and the user can point to that folder.
    pub needs_mods_root: bool,
    /// The external mod providers the game is on, with the provider's key for the game.
    pub providers: Vec<GameProviderView>,
}

/// One external provider the game is on.
#[derive(Debug, Serialize, Type)]
pub struct GameProviderView {
    pub id: String,
    pub key: String,
}

/// The loader preset a game uses, as the interface shows it.
#[derive(Debug, Serialize, Type)]
pub struct GameLoaderView {
    /// The preset id, the name of its file without `.toml`.
    pub id: String,
    /// The display name from the preset, for example `BepInEx`.
    pub name: String,
    /// The variant merged into the game: the one the game named, or the preset's default.
    pub variant: Option<String>,
    /// Whether the variant came from the preset's `default_variant`, not from the game's line.
    pub default_variant: bool,
}

impl GameDef {
    #[cfg(test)]
    pub fn view(&self) -> GameView {
        self.view_with_origin(GameOrigin::Bundled)
    }

    pub fn view_with_origin(&self, origin: GameOrigin) -> GameView {
        GameView {
            id: self.id.clone(),
            name: self.name.clone(),
            origin,
            verified: self.verified.clone(),
            steam_app_ids: self.detect.steam_app_ids.clone(),
            places: self.places(),
            loader: self.resolved_loader.as_ref().map(|loader| GameLoaderView {
                id: loader.id.clone(),
                name: loader.name.clone(),
                variant: loader.variant.clone(),
                default_variant: loader.default_variant,
            }),
            needs_settings_dir: self.has_settings_file(),
            needs_mods_root: self.has_external_root(),
            providers: self
                .providers
                .iter()
                .filter_map(|(id, game)| {
                    Some(GameProviderView {
                        id: id.clone(),
                        key: game.key.clone()?,
                    })
                })
                .collect(),
        }
    }
}
