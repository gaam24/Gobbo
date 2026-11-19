//! Scans a mod folder in the library. The walk itself lives in `infra::fs`, because the library index and the
//! overwrite view use the same one.

use std::path::Path;

use crate::deploy::DeployError;
use crate::error::FileError;
use crate::infra::fs;

/// Paths of all files in a mod folder, relative to that folder, with `/` as the separator.
pub fn list_files(dir: &Path) -> Result<Vec<String>, DeployError> {
    fs::list_files(dir).map_err(|error| FileError::new(dir, error).into())
}
