//! The snapshot views the window reads.

use serde::Serialize;

use super::manifest::{Manifest, SnapshotKind};
use crate::domain::user_data::Kind;

/// One set of files in a snapshot, with how many files and bytes it holds.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SnapshotSetView {
    pub id: String,
    pub kind: Kind,
    pub files: u32,
    pub bytes: u32,
}

/// One saved snapshot as the window lists it: when it was taken and what it holds.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SnapshotView {
    pub id: String,
    pub profile: String,
    pub profile_name: String,
    pub kind: SnapshotKind,
    pub created_at: u32,
    /// The number of mods in the saved profile list.
    pub mods: u32,
    pub files: u32,
    pub bytes: u32,
    pub overwrite_files: u32,
    pub overwrite_bytes: u32,
    pub sets: Vec<SnapshotSetView>,
}

/// What a restore would change in the profile and whether the disk has room for it.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RestorePreview {
    pub added: u32,
    pub replaced: u32,
    pub removed: u32,
    /// The bytes a restore writes. A file that is already the same is not counted.
    pub bytes: u32,
    pub free_kb: u32,
    pub enough_space: bool,
    pub running: bool,
    /// The number of mods in the snapshot's list.
    pub mods: u32,
    /// The profile does not exist any more and the restore will recreate it.
    pub recreates_profile: bool,
}

/// What a restore did, including the fresh backup it left so the restore can be undone.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RestoreOutcome {
    /// The fresh backup taken before the restore, so it can be undone. `None` when the profile did not exist.
    pub backup: Option<SnapshotView>,
    pub files: u32,
    /// The profile the data was restored into, which may be a recreated one.
    pub profile: String,
    /// The restore recreated a profile that no longer existed.
    pub profile_created: bool,
}

pub(super) fn view_of(manifest: &Manifest) -> SnapshotView {
    let overwrite_files = manifest
        .overwrite
        .as_ref()
        .map_or(0, |set| u32::try_from(set.files.len()).unwrap_or(u32::MAX));
    let overwrite_bytes = manifest.overwrite.as_ref().map_or(0, |set| set.bytes);
    SnapshotView {
        id: manifest.id.clone(),
        profile: manifest.profile.clone(),
        profile_name: manifest.profile_name.clone(),
        kind: manifest.kind,
        created_at: manifest.created_at,
        mods: manifest.profile_state.as_ref().map_or(0, |state| {
            u32::try_from(state.list.mods.len()).unwrap_or(u32::MAX)
        }),
        files: manifest
            .sets
            .iter()
            .map(|set| u32::try_from(set.files.len()).unwrap_or(u32::MAX))
            .fold(overwrite_files, u32::saturating_add),
        bytes: manifest
            .sets
            .iter()
            .map(|set| set.bytes)
            .fold(overwrite_bytes, u32::saturating_add),
        overwrite_files,
        overwrite_bytes,
        sets: manifest
            .sets
            .iter()
            .map(|set| SnapshotSetView {
                id: set.id.clone(),
                kind: set.kind,
                files: u32::try_from(set.files.len()).unwrap_or(u32::MAX),
                bytes: set.bytes,
            })
            .collect(),
    }
}
