//! The definition folders and the process-wide roots they are resolved from.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The folders with game and loader definitions.
static DEFINITION_DIRS: OnceLock<DefinitionDirs> = OnceLock::new();
/// The user's own definitions under `<data>/definitions`. Kept apart from the bundled folders and set once at start.
static USER_DEFINITIONS: OnceLock<PathBuf> = OnceLock::new();
/// The official definitions downloaded into `<data>/catalog`. The same category of process-wide definition-folder
/// state as `USER_DEFINITIONS`, kept apart from the bundled folders and set once at start.
static CATALOG_DEFINITIONS: OnceLock<PathBuf> = OnceLock::new();

struct DefinitionDirs {
    games: PathBuf,
    loaders: PathBuf,
}

/// The repository's `games/` and `loaders/` folders, used in development and tests. A packaged app sets the bundled
/// resource folders in `run()`; they are not next to the binary.
pub(super) const DEV_GAMES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../games");
pub(super) const DEV_LOADERS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../loaders");

/// Sets the folders with game and loader definitions. Ignored when they were already set, so tests and the app can
/// call it more than once.
pub fn set_definition_dirs(games: PathBuf, loaders: PathBuf) {
    let _ = DEFINITION_DIRS.set(DefinitionDirs { games, loaders });
}

/// Sets the root of the user's own definitions: `<data>/definitions`, with `games/` and `loaders/` below it. Ignored
/// when it was already set.
pub fn set_user_definition_dirs(data_root: PathBuf) {
    let _ = USER_DEFINITIONS.set(data_root.join("definitions"));
}

/// Sets the root of the official downloaded definitions: `<data>/catalog`, with `games/` and `loaders/` below it.
/// Ignored when it was already set. When the app never sets a data root, the catalog folders are absent and the
/// layering degrades to bundled and user definitions only.
pub fn set_catalog_definition_dirs(data_root: PathBuf) {
    let _ = CATALOG_DEFINITIONS.set(data_root.join("catalog"));
}

/// The folder with user game definitions, when the app has set its data root.
pub fn user_games_dir() -> Option<PathBuf> {
    USER_DEFINITIONS.get().map(|root| root.join("games"))
}

/// The folder with user loader presets, when the app has set its data root.
pub(super) fn user_loaders_dir() -> Option<PathBuf> {
    USER_DEFINITIONS.get().map(|root| root.join("loaders"))
}

/// The folder with official game definitions, when the app has set its data root.
pub fn catalog_games_dir() -> Option<PathBuf> {
    CATALOG_DEFINITIONS.get().map(|root| root.join("games"))
}

/// The folder with official loader presets, when the app has set its data root.
pub(super) fn catalog_loaders_dir() -> Option<PathBuf> {
    CATALOG_DEFINITIONS.get().map(|root| root.join("loaders"))
}

/// The folder with game definitions: the bundled resource folder, or the repository folder in development.
pub fn games_dir() -> &'static Path {
    DEFINITION_DIRS
        .get()
        .map_or_else(|| Path::new(DEV_GAMES_DIR), |dirs| dirs.games.as_path())
}

/// The folder with loader presets: the bundled resource folder, or the repository folder in development.
pub(super) fn loaders_dir() -> &'static Path {
    DEFINITION_DIRS
        .get()
        .map_or_else(|| Path::new(DEV_LOADERS_DIR), |dirs| dirs.loaders.as_path())
}

/// Whether `id` can name a game folder: lowercase letters, digits and `-`. An id from the window never becomes a path
/// that leaves the definition folders.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The `game.toml` of one game by id. An id that cannot name a game folder gives an empty path. A user definition wins
/// over an official one, which wins over the bundled file.
pub fn definition_path(id: &str) -> PathBuf {
    definition_path_from(
        user_games_dir().as_deref(),
        catalog_games_dir().as_deref(),
        games_dir(),
        id,
    )
}

/// The effective `game.toml` of one game, searching the explicit folders in user, official, bundled order. A caller
/// that passes its own folders reads the same precedence without depending on the process-wide data root.
pub(super) fn definition_path_from(
    user: Option<&Path>,
    catalog: Option<&Path>,
    bundled: &Path,
    id: &str,
) -> PathBuf {
    if !is_valid_id(id) {
        return PathBuf::new();
    }

    for dir in [user, catalog].into_iter().flatten() {
        let path = dir.join(id).join("game.toml");

        if path.is_file() {
            return path;
        }
    }

    bundled.join(id).join("game.toml")
}
