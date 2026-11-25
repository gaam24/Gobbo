//! The names of the library folders an import creates: the wanted name of each mod, and a free one when the name is taken.

use std::collections::BTreeSet;
use std::path::Path;

use crate::domain::naming::{first_free, numbered_name};
use crate::domain::pathkey::path_key;
use crate::infra::archive::Archive;

use super::Placed;
use super::errors::InstallError;

/// The name a mod's folder wants before a free one is found: the archive's name for an archive of one mod, otherwise
/// the archive's name and the name of the group.
pub(super) fn wanted_name(base_name: &str, mods: usize, group: &str) -> String {
    if mods == 1 {
        base_name.to_string()
    } else {
        format!("{base_name} - {}", crate::domain::naming::clean(group))
    }
}

/// A folder that is not taken in the library. `taken` holds the path keys of the folders the other mods of this archive
/// already got: they move into the library only after every name is chosen, so the library cannot tell them apart yet.
/// A taken name gets a suffix from the digest, and a second collision with the library is refused. A digest name that
/// another mod of the archive took is numbered (`name (digest) (2)`).
pub(super) fn free_folder_name(
    library: &Path,
    wanted: &str,
    digest: &str,
    taken: &BTreeSet<String>,
) -> Result<String, InstallError> {
    let wanted = crate::domain::naming::safe_limited_file_name(wanted);
    let free = |name: &str| !taken.contains(&path_key(name)) && !library.join(name).exists();

    if free(&wanted) {
        return Ok(wanted);
    }

    let other = with_digest(&wanted, digest);

    if library.join(&other).exists() {
        return Err(InstallError::AlreadyInLibrary { folder: other });
    }

    Ok(first_free(|number| numbered_name(&other, number), free))
}

/// Refuses an archive that places a file under a path Windows cannot store, such as `Mods/x/a:b.dat`. Only placed entries
/// are checked: the layout may skip a readme named `Read me: first.txt`, and a skipped entry is never written. The rule
/// is the same on every platform, so an archive imports the same way on both systems.
pub(super) fn refuse_unwritable(placed: &[Placed], archive: &Archive) -> Result<(), InstallError> {
    for item in placed {
        let storable = item
            .rel
            .split('/')
            .filter(|part| !part.is_empty())
            .all(crate::domain::naming::is_windows_component);

        if !storable {
            return Err(InstallError::UnsafeEntry {
                name: archive.entries()[item.index].name.clone(),
            });
        }
    }

    Ok(())
}

/// Whether `folder` is the name `wanted` gets in the library: the name itself, or the name with the digest after it.
pub(super) fn is_folder_for(folder: &str, wanted: &str, digest: &str) -> bool {
    let wanted = crate::domain::naming::safe_limited_file_name(wanted);
    folder == wanted || folder == with_digest(&wanted, digest)
}

/// The name with the start of the digest after it: the name a mod gets when the plain one is taken.
pub(super) fn with_digest(name: &str, digest: &str) -> String {
    let short = digest.get(..8).unwrap_or(digest);
    format!("{name} ({short})")
}

/// Splits an entry name into parts. An absolute path and a `..` part leave the library, so they are errors. Any other
/// name is kept, whatever the platform stores: the layout may skip the entry, and a skipped entry is never written.
/// `refuse_unwritable` checks the entries the layout places.
pub(super) fn split_name(name: &str) -> Result<Vec<&str>, InstallError> {
    let parts: Vec<&str> = name
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();

    if name.starts_with('/') || parts.contains(&"..") {
        return Err(InstallError::UnsafeEntry {
            name: name.to_string(),
        });
    }

    Ok(parts)
}
