//! Pure logic: no disk, no network, no Tauri. The rules of the mod list, the deployment plan and the names,
//! so each of them can be tested without a game.

/// The longest description Gobbo keeps from a provider page or an `info.json`.
pub const DESCRIPTION_MAX: usize = 8_000;

/// The version of the on-disk state files (`library.json`, `profiles.json`, `deployment.json`). A newer file is never
/// overwritten by an older build; an older file is upgraded in memory before the next save.
pub const STATE_SCHEMA: u32 = 1;

/// The serde default for a field that did not exist before the version was added.
pub const fn state_schema() -> u32 {
    STATE_SCHEMA
}

pub mod archive;
pub mod arguments;
pub mod collection;
pub mod deploy;
pub mod deployment;
pub mod fomod;
pub mod library;
pub mod mod_steps;
pub mod naming;
pub mod pack;
pub mod pathkey;
pub mod profiles;
pub mod progress;
pub mod proton_options;
pub mod rate;
pub mod resolver;
pub mod source;
pub mod storage;
pub mod user_data;
pub mod version;
