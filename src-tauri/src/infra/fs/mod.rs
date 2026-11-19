//! Walking a folder tree. The scan of a mod folder, the record of the game folder and the copy of profile data all
//! need the same recursion and the same rule for a symlink; only what they do with an entry differs.

mod copy;
mod remove;
mod walk;

#[cfg(test)]
pub use copy::fail_after;
pub use copy::{CopyMode, copy_tree, copy_tree_hashed, hash_tree, replace_with_staged, sync_copy};
pub use remove::{ignore_not_found, remove_dir_if_exists, remove_file_if_exists, remove_path};
pub use walk::{WalkErrors, file_sizes, folder_size, list_files, list_files_with_sizes, walk};

// The Windows redirect merge matches on the copy error by name; Linux code propagates it without naming the type.
#[cfg(windows)]
pub use copy::CopyError;
