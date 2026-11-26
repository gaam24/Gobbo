//! The definition folders in precedence order. `dirs.rs` owns the paths; this module composes them with their origins
//! once, so the catalog, the loader merge, the editor and the remote installer all read the same layering.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::dirs::{
    catalog_games_dir, catalog_loaders_dir, games_dir, loaders_dir, user_games_dir,
    user_loaders_dir,
};

/// Where a game definition came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum GameOrigin {
    /// Shipped with Gobbo in `games/`.
    Bundled,
    /// Downloaded from the official catalog into `<data>/catalog/games/`. Read-only, like a bundled definition.
    Official,
    /// Added by the user under `<data>/definitions/games/`.
    User,
}

/// One definition folder and the origin of the definitions it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionLayer {
    /// The definition folder.
    pub path: PathBuf,
    /// The origin of the definitions in the folder.
    pub origin: GameOrigin,
}

/// The game and loader definition folders, each in its own precedence order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionLayers {
    /// Game folders in catalog order: official, bundled then user. A user file never replaces a catalog or bundled
    /// one in the catalog listing.
    pub games: Vec<DefinitionLayer>,
    /// Loader folders in merge order: user, official then bundled. A user preset replaces a later copy of the same id.
    pub loaders: Vec<DefinitionLayer>,
}

impl DefinitionLayers {
    /// The process-wide layers, from the data root and the bundled resource folders set at start.
    pub(super) fn current() -> Self {
        Self::from_paths(
            user_games_dir().as_deref(),
            catalog_games_dir().as_deref(),
            games_dir(),
            user_loaders_dir().as_deref(),
            catalog_loaders_dir().as_deref(),
            loaders_dir(),
        )
    }

    /// The current loader layers with explicit game folders. The catalog reads its own game folders in a test without
    /// a data root; the order of both lists is still decided here.
    pub(super) fn with_game_paths(
        user: Option<&Path>,
        catalog: Option<&Path>,
        bundled: &Path,
    ) -> Self {
        Self::from_paths(
            user,
            catalog,
            bundled,
            user_loaders_dir().as_deref(),
            catalog_loaders_dir().as_deref(),
            loaders_dir(),
        )
    }

    /// The current loader layers with an explicit bundled folder. A caller that owns its bundled presets keeps the
    /// same user / official / bundled precedence as the app.
    pub(super) fn with_bundled_loaders(bundled: &Path) -> Self {
        Self::from_paths(
            None,
            None,
            Path::new(""),
            user_loaders_dir().as_deref(),
            catalog_loaders_dir().as_deref(),
            bundled,
        )
    }

    /// Builds both ordered lists. The only place that decides which origin comes first for each kind of definition.
    fn from_paths(
        user_games: Option<&Path>,
        catalog_games: Option<&Path>,
        bundled_games: &Path,
        user_loaders: Option<&Path>,
        catalog_loaders: Option<&Path>,
        bundled_loaders: &Path,
    ) -> Self {
        let mut games = Vec::with_capacity(3);

        if let Some(catalog) = catalog_games {
            games.push(DefinitionLayer {
                path: catalog.to_path_buf(),
                origin: GameOrigin::Official,
            });
        }

        games.push(DefinitionLayer {
            path: bundled_games.to_path_buf(),
            origin: GameOrigin::Bundled,
        });

        if let Some(user) = user_games {
            games.push(DefinitionLayer {
                path: user.to_path_buf(),
                origin: GameOrigin::User,
            });
        }

        let mut loaders = Vec::with_capacity(3);

        if let Some(user) = user_loaders {
            loaders.push(DefinitionLayer {
                path: user.to_path_buf(),
                origin: GameOrigin::User,
            });
        }

        if let Some(catalog) = catalog_loaders {
            loaders.push(DefinitionLayer {
                path: catalog.to_path_buf(),
                origin: GameOrigin::Official,
            });
        }

        loaders.push(DefinitionLayer {
            path: bundled_loaders.to_path_buf(),
            origin: GameOrigin::Bundled,
        });

        Self { games, loaders }
    }

    /// The game folders in catalog order, without their origins.
    pub(super) fn game_paths(&self) -> Vec<PathBuf> {
        self.games.iter().map(|layer| layer.path.clone()).collect()
    }

    /// The loader folders in merge order, without their origins.
    pub(super) fn loader_paths(&self) -> Vec<PathBuf> {
        self.loaders
            .iter()
            .map(|layer| layer.path.clone())
            .collect()
    }

    /// The game folders with their origins, in catalog order.
    pub(super) fn game_layers(&self) -> &[DefinitionLayer] {
        &self.games
    }

    /// The loader folders with their origins, in merge order.
    pub(super) fn loader_layers(&self) -> &[DefinitionLayer] {
        &self.loaders
    }

    /// The game folder of one origin, when the app has that layer.
    pub(super) fn game_path(&self, origin: GameOrigin) -> Option<&Path> {
        self.games
            .iter()
            .find(|layer| layer.origin == origin)
            .map(|layer| layer.path.as_path())
    }

    /// The loader folder of one origin, when the app has that layer.
    pub(super) fn loader_path(&self, origin: GameOrigin) -> Option<&Path> {
        self.loaders
            .iter()
            .find(|layer| layer.origin == origin)
            .map(|layer| layer.path.as_path())
    }
}

/// The loader folders from explicit paths, in merge order. Keeps the loader test seam on the same order as the app
/// without giving the tests the process-wide data root.
#[cfg(test)]
pub(super) fn loader_dirs(
    user: Option<&Path>,
    catalog: Option<&Path>,
    bundled: &Path,
) -> Vec<PathBuf> {
    DefinitionLayers::from_paths(None, None, Path::new(""), user, catalog, bundled).loader_paths()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_are_in_precedence_order_and_each_appears_once() {
        let layers = DefinitionLayers::from_paths(
            Some(Path::new("/user/games")),
            Some(Path::new("/catalog/games")),
            Path::new("/bundled/games"),
            Some(Path::new("/user/loaders")),
            Some(Path::new("/catalog/loaders")),
            Path::new("/bundled/loaders"),
        );

        let paths = |names: &[&str]| names.iter().map(PathBuf::from).collect::<Vec<_>>();
        // The catalog lists official first so a user file cannot replace a downloaded or bundled one.
        assert_eq!(
            layers.game_paths(),
            paths(&["/catalog/games", "/bundled/games", "/user/games"])
        );
        // A loader merge reads the user's preset first, so it wins over the official and bundled copies.
        assert_eq!(
            layers.loader_paths(),
            paths(&["/user/loaders", "/catalog/loaders", "/bundled/loaders"])
        );
        assert_eq!(
            layers
                .game_layers()
                .iter()
                .map(|layer| layer.origin)
                .collect::<Vec<_>>(),
            [GameOrigin::Official, GameOrigin::Bundled, GameOrigin::User]
        );
        assert_eq!(
            layers
                .loader_layers()
                .iter()
                .map(|layer| layer.origin)
                .collect::<Vec<_>>(),
            [GameOrigin::User, GameOrigin::Official, GameOrigin::Bundled]
        );

        for (kind, count) in [
            ("game", layers.games.len()),
            ("loader", layers.loaders.len()),
        ] {
            let mut paths = if kind == "game" {
                layers.game_paths()
            } else {
                layers.loader_paths()
            };
            paths.sort();
            paths.dedup();
            assert_eq!(paths.len(), count, "each {kind} folder appears once");
        }
    }
}
