//! What the library keeps about one mod: its folder, its info, its family and its versions.

mod archive;
mod family;
mod mod_info;
mod version;

#[cfg(test)]
mod tests;

pub use archive::{Archive, ArchiveLabel, SkipReason, SkippedFile};
pub use family::{AdoptPolicy, Family, adopt_fields};
pub use mod_info::{ModInfo, ModInfoEdit};
pub use version::{Integrity, ModVersion};
