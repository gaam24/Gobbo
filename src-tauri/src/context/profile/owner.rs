//! The small file that says which profile currently owns the data materialized outside the game folder. Both the
//! legacy `profile_data` copy list and the definition-driven `user_data` sets keep one; they share the read, write
//! and error policy.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::context::Data;
use crate::error::AppError;
use crate::error::ErrorKind;
use crate::infra::json;

#[derive(Debug, Serialize, Deserialize)]
struct Owner {
    profile: String,
}

/// The owner file in the game's `state` folder.
pub(super) fn path(data: &Data, file: &str) -> PathBuf {
    data.dir.join("state").join(file)
}

/// The profile named by the owner file. A missing file means no profile owns the data yet.
pub(super) fn read(path: &Path, invalid_code: &'static str) -> Result<Option<String>, AppError> {
    match json::read_json::<Owner>(path) {
        Ok(owner) => Ok(owner.map(|owner| owner.profile)),
        Err(json::ReadError::Parse(error)) => Err(AppError::new(invalid_code)
            .with("path", path.display())
            .with("detail", error)
            .kind(ErrorKind::Permanent)),
        Err(json::ReadError::Io(error)) => Err(crate::error::io_at(path)(error)),
    }
}

/// Records `profile` as the owner.
pub(super) fn write(path: &Path, profile: &str) -> Result<(), AppError> {
    json::write_json(
        path,
        &Owner {
            profile: profile.to_string(),
        },
    )
}

/// Reads the owner file named by `file` in the game's `state` folder.
pub(super) fn read_owner(
    data: &Data,
    file: &str,
    invalid_code: &'static str,
) -> Result<Option<String>, AppError> {
    read(&path(data, file), invalid_code)
}

/// Records `profile` as the owner of the file named by `file`.
pub(super) fn write_owner(data: &Data, file: &str, profile: &str) -> Result<(), AppError> {
    write(&path(data, file), profile)
}
