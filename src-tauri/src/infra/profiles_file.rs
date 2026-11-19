//! The file of the game's profiles, `profiles.json`: reading it and saving it. The profile types are in
//! `domain::profiles`, and this is the only place that puts them on disk.

use std::path::Path;

use crate::domain::profiles::Profiles;
use crate::error::{AppError, ErrorKind, FileError};
use crate::infra::json::{self, ReadError};

impl Profiles {
    /// Reads `profiles.json`. A missing file gives `None` and the caller creates the first profile.
    pub fn read(path: &Path) -> Result<Option<Self>, AppError> {
        match json::read_json(path) {
            Ok(profiles) => profiles.map_or(Ok(None), |profiles: Self| profiles.check().map(Some)),
            Err(ReadError::Parse(error)) => {
                // Keep the damaged file aside before the first save replaces it, so a user can still recover it.
                json::keep_broken(path, &error);
                Err(AppError::new("profiles.parse_failed")
                    .with("detail", error)
                    .kind(ErrorKind::Permanent))
            }
            Err(ReadError::Io(error)) => Err(AppError::from(FileError::new(path, error))),
        }
    }

    /// Reads `profiles.json`, or starts the game with one empty profile. A missing file is saved, so the next read
    /// finds it. A damaged file was kept as `.json.broken` by `read`; it starts from the default profile instead of
    /// leaving the game screen unusable.
    pub fn read_or_default(path: &Path) -> Result<Self, AppError> {
        match Self::read(path) {
            Ok(Some(profiles)) => Ok(profiles),
            Ok(None) => Ok(Self::with_default(Vec::new())),
            Err(error) if error.code == "profiles.parse_failed" => {
                Ok(Self::with_default(Vec::new()))
            }
            Err(error) => Err(error),
        }
    }

    /// Saves `profiles.json` atomically.
    pub fn save(&self, path: &Path) -> Result<(), AppError> {
        json::write_json(path, self)
    }
}
