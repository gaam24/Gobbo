//! Game definitions from `game.toml` files. A game is data, and this folder turns it into checked Rust values:
//! - `definition/`: what a `game.toml` may contain. Unknown keys are errors. The deploy rules themselves are the
//!   pure types in `domain::deploy`, which the resolver uses without depending on this loader.
//! - `validate/`: the rules that need several fields at once.
//! - `loaders/`: the shared presets, merged under a game's own rules.
//! - `layout.rs`: where mods go in the game.
//! - `detect/`: the marker rule and the scan that asks every store for a game's installs.
//! - `stores/`: one adapter per store: Steam's libraries, VDF, compatibility tool and Proton prefix in `steam.rs`, and
//!   the install lists of Epic, GOG, Heroic, Legendary and Lutris.
//! - `prefix.rs`: the store of an install and the Proton prefix its paths resolve with.
//! - `runtime.rs`: how an install starts on this computer, from the files in its folder.
//! - `paths/`: the path templates such as `{documents}` that the definitions use.
//! - `view.rs`: the part of a definition the interface shows.
//! - `catalog.rs`: the list of every definition, its icon and the shareable zip.
//! - `dirs.rs` / `layers.rs`: the definition folders, and their user / official / bundled precedence in one place.
//!
//! This file loads definitions from the games folder (one subfolder per game, with `game.toml` and an optional icon)
//! and resolves the loader presets from the loaders folder (`loaders/<id>.toml`). The module also owns the catalog
//! commands (`commands.rs`): the list, the icon and the scan of the Steam libraries.

mod cache;
mod catalog;
pub mod definition;
pub mod detect;
mod dirs;
pub mod editor;
mod game;
mod layers;
mod layout;
mod load;
mod loaders;
pub mod paths;
pub mod prefix;
pub mod proxy;
mod remote;
pub mod runtime;
pub mod stores;
mod validate;
mod view;

#[cfg(test)]
mod tests;

// Private names that sibling modules and the tests reach through this module root.
use cache::read_cached;
#[cfg(test)]
use cache::read_through;
use dirs::definition_path_from;
#[cfg(test)]
use dirs::{DEV_GAMES_DIR, DEV_LOADERS_DIR};
use dirs::{catalog_loaders_dir, loaders_dir, user_loaders_dir};
#[cfg(test)]
use layers::loader_dirs;
use load::invalid_user_definition;
#[cfg(test)]
use load::read_definition;
use load::read_definition_with;
#[cfg(test)]
use std::path::PathBuf;

/// The prerequisites are read on Linux only, where Proton installs them.
#[cfg_attr(
    windows,
    allow(
        unused_imports,
        reason = "prerequisites are only read on Linux, where Proton installs them"
    )
)]
pub use definition::Prerequisite;
#[cfg(test)]
pub use definition::UserData;
pub use definition::{
    Executable, GameDef, LoaderPackage, LogKind, LogSource, Step, Tool, Verified,
};
pub use layout::{GAME_TEMPLATE, is_game_relative};
pub use view::GameView;

pub use catalog::{GameCatalog, catalog, definitions, export, icon};
#[cfg(test)]
pub use catalog::{definitions_from, icon_file, icon_file_from, write_export};
pub use layers::GameOrigin;

pub use remote::{
    CatalogInstallOutcome, CatalogInstallStep, CatalogStatus, check_catalog, drop_stale_catalog,
    install_catalog,
};

pub use crate::domain::deploy::{Deploy, Mirror, Slot};
pub use crate::domain::mod_steps::Phase;

pub use cache::{forget_all_definitions, forget_definition};
pub use dirs::{
    catalog_games_dir, definition_path, games_dir, is_valid_id, set_catalog_definition_dirs,
    set_definition_dirs, set_user_definition_dirs, user_games_dir,
};
#[cfg(test)]
pub use load::load_dir;
pub use load::{load, load_shared};

// ---------------------------------------------------------------------------------------------
// The use cases and commands of this module.
// ---------------------------------------------------------------------------------------------

pub mod commands;
