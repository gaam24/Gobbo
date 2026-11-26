//! Copying, replacing, merging and hashing a tree. The modes share one walk and one staging rule.

mod merge;
mod model;
mod sync;
mod tree;

#[cfg(test)]
mod tests;

pub use merge::replace_with_staged;
#[cfg(any(test, windows))]
pub use model::CopyError;
pub use model::CopyMode;
#[cfg(test)]
pub use model::{EntryKind, MergeConflict};
#[cfg(test)]
pub use sync::fail_after;
pub use sync::sync_copy;
pub use tree::{copy_tree, copy_tree_hashed, hash_tree};
