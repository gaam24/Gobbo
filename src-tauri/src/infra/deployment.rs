//! The deployment files in a profile's state folder: this module owns their names and reads the completion marker. The
//! record type is in `domain::deployment_record`; `deploy::journal` writes the file after the last file and the settings
//! are in place.
//!
//! The journal records what undo has to revert; the completion marker says the apply reached its end; the mount file
//! tells the overlay hook which folders this session covers.

use std::path::Path;

/// The journal of a deployment, in the state folder of a profile. It records what to undo; it does not by itself mean
/// the deployment finished. The completion marker below is the deployed flag.
pub const JOURNAL_FILE: &str = "journal.jsonl";

/// Written after the files and the settings are in place. A journal without this marker is an interrupted apply, not a
/// deployment the game may start from.
pub const DEPLOYMENT_FILE: &str = "deployment.json";

/// The name of the JSON file the hook reads instead of the three single-root environment variables. It is written
/// before every launch, so a profile switch never leaves a stale mount set behind.
pub const MOUNTS_FILE: &str = "overlay-mounts.json";

use crate::domain::deployment_record::DeploymentRecord;
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
