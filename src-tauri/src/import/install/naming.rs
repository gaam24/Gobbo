//! The names of the library folders an import creates: the wanted name of each mod, and a free one when the name is taken.

use std::path::Path;

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

/// A folder that is not taken in the library. A taken name gets a suffix from the digest; a second collision is refused.
pub(super) fn free_folder_name(
    library: &Path,
    wanted: &str,
    digest: &str,
) -> Result<String, InstallError> {
    let wanted = crate::domain::naming::safe_limited_file_name(wanted);

    if !library.join(&wanted).exists() {
        return Ok(wanted);
    }

    let other = with_digest(&wanted, digest);

    if library.join(&other).exists() {
        return Err(InstallError::AlreadyInLibrary { folder: other });
    }

    Ok(other)
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

/// Splits an entry name into parts. `..`, an absolute path and a drive letter are errors.
pub(super) fn split_name(name: &str) -> Result<Vec<&str>, InstallError> {
    let unsafe_name = || InstallError::UnsafeEntry {
        name: name.to_string(),
    };

    if name.starts_with('/') || name.contains(':') {
        return Err(unsafe_name());
    }

    let parts: Vec<&str> = name
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.contains(&"..")
        || parts
            .iter()
            .any(|part| !crate::domain::naming::is_safe_component(part))
    {
        return Err(unsafe_name());
    }

    Ok(parts)
}
