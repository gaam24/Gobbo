//! The game library: every imported mod version in `library/`, archive copies in `archives/`, and the
//! `library.json` index. A family is one mod across versions; a version is one folder in the library.
//! App start reads the index and does not walk the folders. A scan happens after an import, at deployment
//! (enabled versions only), and on request ("Rescan library"). The module also owns the commands and the library
//! mutations: rescan, delete, metadata and the manual source id.

mod fields;
pub mod integrity;
pub mod relations;
pub mod sync;
pub mod waiting;

pub use crate::context::library::{
    family_id_from, lock_index, mod_info, not_in_library, scan_mod, version_from_import,
};
pub use crate::domain::library::{
    Archive, ArchiveLabel, Family, Integrity, Library, LibraryView, ModInfo, ModInfoEdit,
    ModVersion,
};
pub use fields::FamilyFields;

use crate::error::AppError;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------------------------
// The use cases and commands of this module.
// ---------------------------------------------------------------------------------------------

mod delete;
mod metadata;
mod rescan;
mod source_id;

use crate::context::Data;
use crate::domain::naming::is_plain_folder;
use crate::domain::profiles::Profiles;
use crate::domain::profiles::entry::ProfileMod;
use crate::error::io_at;
use crate::games::GameDef;

pub use delete::{delete_mod, delete_mod_version, delete_page, delete_unused_versions};
pub use metadata::{
    RelationKind, Relations, add_relation, add_requirement, remove_relation, remove_requirement,
    set_file_winner, set_mod_info, set_mod_instructions, set_mod_note, set_mod_relations,
    set_mod_tags, set_requirement_ignored,
};
pub use rescan::rescan;
pub use source_id::set_mod_source;

/// The version string a profile entry selects, when the library has that version.
pub fn version_of(library: &Library, entry: &crate::domain::profiles::Entry) -> Option<String> {
    library
        .versions
        .get(&entry.folder)
        .and_then(|version| version.info.version.clone())
}

/// One profile's list, with each entry's library record and slot resolved.
pub fn mods_of(
    profile: &crate::domain::profiles::Profile,
    library: &Library,
    game: &GameDef,
) -> Vec<ProfileMod> {
    profile
        .mods
        .iter()
        .map(|entry| {
            let record = library.versions.get(&entry.folder);
            ProfileMod {
                id: entry.id.clone(),
                folder: entry.folder.clone(),
                slot: record.map_or_else(
                    || game.slot_for_folder(&entry.folder),
                    |record| record.slot.clone(),
                ),
                enabled: entry.enabled,
                priority: entry.priority,
            }
        })
        .collect()
}

/// The active profile's list, with each entry's library record and slot resolved.
pub fn active_mods(profiles: &Profiles, library: &Library, game: &GameDef) -> Vec<ProfileMod> {
    mods_of(profiles.active(), library, game)
}

/// Removes one version folder from the library. A missing folder is fine.
fn remove_version_folder(data: &Data, folder: &str) -> Result<(), AppError> {
    if !is_plain_folder(folder) {
        return Err(AppError::user("library.invalid_folder").with("folder", folder));
    }
    let path = data.library().join(folder);
    crate::infra::fs::remove_dir_if_exists(&path).map_err(io_at(&path))
}
