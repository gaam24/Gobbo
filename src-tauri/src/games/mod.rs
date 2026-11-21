//! Game definitions from `game.toml` files. A game is data, and this folder turns it into checked Rust values:
//! - `definition/`: what a `game.toml` may contain. Unknown keys are errors. The deploy rules themselves are the
//!   pure types in `domain::deploy`, which the resolver uses without depending on this loader.
//! - `validate/`: the rules that need several fields at once.
//! - `loaders.rs`: the shared presets in the `loaders` folder, merged under a game's own rules.
//! - `layout.rs`: where mods go in the game.
//! - `detect.rs`: finding the game in Steam libraries.
//! - `steam.rs`: the VDF values, compatibility tool and Proton prefix shared by detection and launch.
//! - `paths.rs`: the path templates such as `{documents}` that the definitions use.
//! - `view.rs`: the part of a definition the interface shows.
//! - `catalog.rs`: the list of every definition, its icon and the shareable zip.
//!
//! This file loads definitions from the games folder (one subfolder per game, with `game.toml` and an optional icon)
//! and resolves the loader presets from the loaders folder (`loaders/<id>.toml`). The module also owns the catalog
//! commands (`commands.rs`): the list, the icon and the scan of the Steam libraries.

mod catalog;
mod definition;
pub mod detect;
pub mod editor;
mod layout;
mod loaders;
pub mod paths;
pub mod proxy;
mod remote;
pub mod steam;
mod stores;
mod validate;
mod view;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::error::{AppError, ErrorKind, io_at};

/// The prerequisites are read on Linux only, where Proton installs them.
#[cfg_attr(
    windows,
    allow(
        unused_imports,
        reason = "prerequisites are only read on Linux, where Proton installs them"
    )
)]
pub use definition::Prerequisite;
pub use definition::{
    Executable, GameDef, LoadOrder, LogKind, LogSource, Step, Tool, UserData, Verified,
};
pub use layout::{GAME_TEMPLATE, is_game_relative};
pub use view::GameView;

pub use catalog::{GameCatalog, GameOrigin, catalog, definitions, export, icon};
#[cfg(test)]
pub use catalog::{definitions_from, icon_file, icon_file_from, write_export};

pub use remote::{
    CatalogInstallOutcome, CatalogInstallStep, CatalogStatus, check_catalog, drop_stale_catalog,
    install_catalog,
};

pub use crate::domain::deploy::{Deploy, Mirror, Slot};
pub use crate::domain::mod_steps::Phase;

impl GameDef {
    /// The user-data sets declared by this game, in definition order.
    pub fn user_data_sets(&self) -> &[UserData] {
        &self.user_data
    }

    /// Parses a definition without resolving its loader or checking it. `load` and `load_dir` do both.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    /// The values this game's path templates resolve with, for an installation in `game_root`: its Proton prefix when
    /// it has one, the user's `home`, and the folder of the order file the user chose.
    pub fn path_context(
        &self,
        game_root: &Path,
        home: &Path,
        settings: Option<&Path>,
    ) -> paths::Context {
        paths::Context {
            game_root: game_root.to_path_buf(),
            prefix: steam::proton_prefix(self, game_root),
            home: home.to_path_buf(),
            settings: settings.map(Path::to_path_buf),
        }
    }

    /// The folder of the game's order file on this computer, when the game has one and the folder already exists.
    pub fn find_order_dir(&self, game_root: &Path, home: &Path) -> Option<PathBuf> {
        let order = self.load_order.as_ref()?;
        let file = self
            .path_context(game_root, home, None)
            .resolve(&order.file)
            .ok()?;
        let dir = file.parent()?;
        dir.is_dir().then(|| dir.to_path_buf())
    }
}

impl GameDef {
    /// The tool of the game that is this mod on the provider, when the definition names one.
    pub fn tool_for_source(&self, provider: &str, mod_id: &str) -> Option<&Tool> {
        self.tools.iter().find(|tool| {
            tool.source.as_ref().is_some_and(|source| {
                source.provider.as_str() == provider && source.mod_id == mod_id
            })
        })
    }
}

impl LoadOrder {
    /// The order file in the folder the user chose. It keeps the file name from the definition.
    pub fn file_in(&self, dir: &Path) -> PathBuf {
        let name = self.file.rsplit(['/', '\\']).next().unwrap_or(&self.file);
        dir.join(name)
    }
}

/// The folders with game and loader definitions.
static DEFINITION_DIRS: OnceLock<DefinitionDirs> = OnceLock::new();
/// The user's own definitions under `<data>/definitions`. Kept apart from the bundled folders and set once at start.
static USER_DEFINITIONS: OnceLock<PathBuf> = OnceLock::new();
/// The official definitions downloaded into `<data>/catalog`. The same category of process-wide definition-folder
/// state as `USER_DEFINITIONS`, kept apart from the bundled folders and set once at start.
static CATALOG_DEFINITIONS: OnceLock<PathBuf> = OnceLock::new();
/// Parsed definitions by path, keyed by length and modification time. The cache keeps one shared `Arc` per unchanged
/// `game.toml`, so the hot read paths hand the same value to every caller.
static DEFINITIONS: OnceLock<crate::infra::file_cache::FileCache<GameDef>> = OnceLock::new();

struct DefinitionDirs {
    games: PathBuf,
    loaders: PathBuf,
}

/// The repository's `games/` and `loaders/` folders, used in development and tests. A packaged app sets the bundled
/// resource folders in `run()`; they are not next to the binary.
const DEV_GAMES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../games");
const DEV_LOADERS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../loaders");

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
fn user_loaders_dir() -> Option<PathBuf> {
    USER_DEFINITIONS.get().map(|root| root.join("loaders"))
}

/// The folder with official game definitions, when the app has set its data root.
pub fn catalog_games_dir() -> Option<PathBuf> {
    CATALOG_DEFINITIONS.get().map(|root| root.join("games"))
}

/// The folder with official loader presets, when the app has set its data root.
fn catalog_loaders_dir() -> Option<PathBuf> {
    CATALOG_DEFINITIONS.get().map(|root| root.join("loaders"))
}

/// The folder with game definitions: the bundled resource folder, or the repository folder in development.
pub fn games_dir() -> &'static Path {
    DEFINITION_DIRS
        .get()
        .map_or_else(|| Path::new(DEV_GAMES_DIR), |dirs| dirs.games.as_path())
}

/// The folder with loader presets: the bundled resource folder, or the repository folder in development.
fn loaders_dir() -> &'static Path {
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

/// Drops the cached parse of a definition that was just written, so the next read sees the new file even when the
/// clock resolution makes its length and modification time look unchanged.
pub fn forget_definition(path: &Path) {
    if let Some(cache) = DEFINITIONS.get() {
        cache.invalidate(path);
    }
}

/// Drops every parsed definition. A user loader preset is merged into each game that names it, and the cache is keyed
/// by `game.toml` only, so a written preset must reach every game that uses it.
pub fn forget_all_definitions() {
    if let Some(cache) = DEFINITIONS.get() {
        cache.clear();
    }
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
fn definition_path_from(
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

/// Loads the definition of one game by id, for example `the-witcher-3-wild-hunt`. Its loader preset is merged first and
/// the result is checked. This is the owned copy for a caller that changes the value; a read-only caller uses
/// `load_shared`. A change to a shipped loader needs a restart.
pub fn load(id: &str) -> Result<GameDef, AppError> {
    load_shared(id).map(|game| (*game).clone())
}

/// The definition of one game as the shared value: a repeated read of an unchanged `game.toml` returns the same `Arc`
/// instead of parsing and cloning the definition again. The hot paths read through here. A loader preset is read at the
/// same time; a change to one needs a restart, as shipped presets do not change while the app runs.
pub fn load_shared(id: &str) -> Result<Arc<GameDef>, AppError> {
    if !is_valid_id(id) {
        return Err(AppError::user("games.unknown").with("id", id));
    }

    let path = definition_path(id);
    read_cached(&path, loaders_dir())
}

/// Reads one definition through the process cache when it uses the app's loader folders.
fn read_cached(path: &Path, loaders: &Path) -> Result<Arc<GameDef>, AppError> {
    read_through(
        DEFINITIONS.get_or_init(crate::infra::file_cache::FileCache::default),
        path,
        loaders,
    )
}

/// Reads one definition through `cache` when it uses the app's loader folders; a caller that passes its own loader
/// folder reads directly, so a temporary loader cannot be served from another test's cache. The cache is passed in so a
/// test can use one of its own instead of the process-wide entries other tests share.
fn read_through(
    cache: &crate::infra::file_cache::FileCache<GameDef>,
    path: &Path,
    loaders: &Path,
) -> Result<Arc<GameDef>, AppError> {
    if loaders != loaders_dir() {
        return read_definition(path, loaders).map(Arc::new);
    }

    cache.load(path, || read_definition(path, loaders))
}

/// Loads `game.toml` from every subfolder of `dir` that is not a `_` folder (the template), sorted by id. Kept for the
/// tests that check a whole folder; the app uses `definitions`, which also reads the user folder.
#[cfg(test)]
pub fn load_dir(dir: &Path) -> Result<Vec<GameDef>, AppError> {
    let entries = std::fs::read_dir(dir).map_err(io_at(dir))?;
    let mut games = Vec::new();

    for entry in entries {
        let entry = entry.map_err(io_at(dir))?;

        if entry.file_name().to_string_lossy().starts_with('_') {
            continue;
        }

        let path = entry.path().join("game.toml");

        if path.is_file() {
            match read_cached(&path, loaders_dir()) {
                Ok(game) => games.push((*game).clone()),
                Err(error) => {
                    // One bad definition must not hide every other game. The error is in the log; the catalog keeps the
                    // definitions that load, and a user definition can be fixed without a reinstall.
                    crate::infra::log::error(format!(
                        "game definition {} was skipped: {error}",
                        path.display()
                    ));
                }
            }
        }
    }

    games.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(games)
}

/// Reads one `game.toml`, merges its loader preset and checks the result. Every problem is reported in one error. The
/// loader folders are searched in user, official, bundled order, so a user preset still wins over an official one.
fn read_definition(path: &Path, loaders_dir: &Path) -> Result<GameDef, AppError> {
    let dirs = loader_dirs(
        user_loaders_dir().as_deref(),
        catalog_loaders_dir().as_deref(),
        loaders_dir,
    );
    read_definition_with(path, &dirs)
}

/// The loader folders in precedence order: user, official catalog, bundled.
fn loader_dirs(user: Option<&Path>, catalog: Option<&Path>, bundled: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::with_capacity(3);

    if let Some(user) = user {
        dirs.push(user.to_path_buf());
    }

    if let Some(catalog) = catalog {
        dirs.push(catalog.to_path_buf());
    }

    dirs.push(bundled.to_path_buf());
    dirs
}

/// Reads one definition with an explicit set of loader folders. The caller owns the precedence, so a test can pass
/// scratch folders without touching the process-wide definition roots.
fn read_definition_with(path: &Path, loader_dirs: &[PathBuf]) -> Result<GameDef, AppError> {
    let text = std::fs::read_to_string(path).map_err(io_at(path))?;
    let mut game = GameDef::parse(&text).map_err(|e| invalid_definition(path, e))?;
    // A merge replaces an entry with the same key, so it would hide a duplicate that is already in this file.
    let duplicates = validate::duplicate_problems(&game.deploy);

    if !duplicates.is_empty() {
        return Err(invalid_definition(path, duplicates.join("; ")));
    }

    loaders::apply(&mut game, loader_dirs).map_err(|detail| invalid_definition(path, detail))?;
    let problems = game.validate();

    if problems.is_empty() {
        Ok(game)
    } else {
        Err(invalid_definition(path, problems.join("; ")))
    }
}

/// The code of a game definition that is not valid. Its kind says who can fix it.
const INVALID_DEFINITION: &str = "games.invalid_definition";

/// A user's definition or draft that is not valid: the user fixes it in the editor or in the folder.
fn invalid_user_definition(detail: impl ToString) -> AppError {
    AppError::user(INVALID_DEFINITION).with("detail", detail)
}

/// A game or loader definition that is shipped with the app is broken. This is a bug in Gobbo, not a user action.
fn invalid_definition(path: &Path, detail: impl ToString) -> AppError {
    AppError::new(INVALID_DEFINITION)
        .with("path", path.display())
        .with("detail", detail)
        .kind(ErrorKind::Bug)
}

// ---------------------------------------------------------------------------------------------
// The use cases and commands of this module.
// ---------------------------------------------------------------------------------------------

pub mod commands;
