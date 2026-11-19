//! Names that Gobbo gives to folders in the library and in the game. Library folders keep the archive name;
//! game folders use a stable, clean name with the slot's prefix.

mod file_names;
mod hashes;
mod import;
mod keys;
mod labels;
mod paths;
mod unique;

pub use file_names::{is_safe_component, safe_file_name, safe_limited_file_name};
pub use hashes::{path_hash, short_hash};
pub use import::imported;
pub use keys::{clean, compact_component, deploy_name, match_key, slug};
pub use labels::requirement_label;
pub use paths::{
    is_game_relative, is_plain_folder, is_safe_relative_path, normalized_relative_path,
};
pub use unique::{first_free, numbered_file_name, numbered_name};
