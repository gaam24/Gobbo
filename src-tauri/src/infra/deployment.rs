//! The deployment completion marker on disk: reading it and checking that this build can trust it. The record type is
//! in `domain::deployment`; `deploy::journal` writes the file after the last file and the settings are in place.

use std::path::Path;

use crate::domain::deployment::DeploymentRecord;
use crate::infra::json::{self, ReadError};

/// Reads the completion marker at `path`. `None` means the apply did not provably reach its end: the marker is
/// missing, empty, damaged, unreadable or from a newer build. Such a marker stays on disk for diagnosis, and the next
/// apply or undo repairs the state; a read here never moves or deletes it.
pub fn read_complete(path: &Path) -> Option<DeploymentRecord> {
    match json::read_json::<DeploymentRecord>(path) {
        Ok(Some(record)) if record.is_supported() => Some(record),
        Ok(Some(record)) => {
            crate::infra::log::warn(format!(
                "deployment marker {} has schema version {} of a newer build (supported {}); the deployment is treated as unfinished",
                path.display(),
                record.schema_version,
                crate::domain::STATE_SCHEMA
            ));
            None
        }
        Ok(None) => None,
        Err(ReadError::Parse(error)) => unreadable(path, &error),
        Err(ReadError::Io(error)) => unreadable(path, &error),
    }
}

/// Reports a marker this build cannot read and treats the apply as unfinished. The file is left where it is.
fn unreadable(path: &Path, error: &impl std::fmt::Display) -> Option<DeploymentRecord> {
    crate::infra::log::warn(format!(
        "deployment marker {} could not be read ({error}); the deployment is treated as unfinished",
        path.display()
    ));
    None
}
