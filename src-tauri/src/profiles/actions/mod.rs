//! Changing profiles: saving the list, priorities, separators, and creating, renaming, switching or deleting a profile.
//! The user-data move lives in `user_data.rs`, next to the list and lifecycle actions it belongs to.

mod create;
mod groups;
mod lifecycle;
mod mods;
#[cfg(test)]
mod tests;
mod user_data;

pub use create::{copy_profile_data, create_profile};
pub use groups::save_mod_groups;
pub use lifecycle::{delete_profile, rename_profile, switch_profile};
pub use mods::{save_profile, set_load_order, set_mod_version};
pub use user_data::{move_profile_user_data, set_profile_user_data, user_data_size};
