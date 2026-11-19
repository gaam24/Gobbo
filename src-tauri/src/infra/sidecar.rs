//! The `<archive>.source.json` sidecar: writing it, reading it, and deleting a download with it. A download carries
//! the provider, mod and file it came from next to the archive, so the import does not have to guess from the name.

use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::source::SourceRef;
use crate::error::{AppError, FileError};

/// The suffix that keeps a download's source beside its archive, so an import never has to guess it from the file name.
pub const SOURCE_SUFFIX: &str = ".source.json";

/// Writes the sidecar. A failed write loses little: the import then reads the mod from the archive's name.
pub fn write_source(archive: &Path, source: SourceRef) {
    let _ = crate::infra::json::write_json(&source_file(archive), &source);
}

/// The sidecar path of an archive. It keeps the archive's full name, so two archives with the same file name in
/// different folders do not share one sidecar.
pub fn source_file(archive: &Path) -> PathBuf {
    let mut name = archive.as_os_str().to_owned();
    name.push(SOURCE_SUFFIX);
    PathBuf::from(name)
}

/// The source of a download when it has a sidecar. An archive Gobbo did not download has none, which is how the caller
/// knows to fall back to the archive name.
pub fn read_source(archive: &Path) -> Option<SourceRef> {
    crate::infra::json::read_json_or_none(&source_file(archive))
}

/// Deletes a downloaded archive and its sidecar, ignoring a missing sidecar.
pub fn delete_download(archive: &Path) -> Result<(), AppError> {
    crate::infra::fs::remove_file_if_exists(archive)
        .map_err(|error| AppError::from(FileError::new(archive, error)))?;
    let _ = fs::remove_file(source_file(archive));
    Ok(())
}

/// Removes a download once its import is done: the library keeps its own copy of the archive.
pub fn forget_download(downloads: &Path, archive: &Path) {
    if archive.parent() == Some(downloads) {
        let _ = delete_download(archive);
    }
}
