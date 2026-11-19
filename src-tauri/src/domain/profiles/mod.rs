//! Game profiles (modpacks). Each profile has a name and its own ordered list of mod families, each with the
//! version selected for that profile. Mods are shared, because they live in the library.
//! Saved in `profiles.json` in the game folder; the active profile is remembered too.

pub mod diff;
pub mod entry;
pub mod group;
pub mod history;
pub mod view;

mod profile;
mod set;

pub use group::{GroupKind, ModGroup};
pub use profile::{Entry, ListState, PackOrigin, Profile, Row, Separator};
#[cfg(test)]
use set::{DEFAULT_NAME, MAX_NAME_CHARS};
pub use set::{
    MoveSide, PageKeys, Profiles, add_group, move_block, next_group_id, normalize_groups,
    remove_group, set_block_enabled, set_mod_group, set_separator,
};
pub use view::ProfilesView;

/// The file in the game data folder.
pub const PROFILES_FILE: &str = "profiles.json";

#[cfg(test)]
mod tests;
