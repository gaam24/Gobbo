//! One family as the Nexus requests need it, built from a generic source request.

use std::path::{Path, PathBuf};

use crate::domain::source::{SourceMeta, SourceRef};
use crate::error::AppError;
use crate::providers::SourceUpdateRequest;
use crate::providers::nexus::NexusRequiresSync;

/// One mod family to look up on Nexus.
#[derive(Clone)]
pub(super) struct Lookup {
    pub(super) family: String,
    pub(super) source: SourceRef,
    pub(super) nexus_id: u32,
    /// The version in use, for the update report.
    pub(super) current: Option<String>,
    /// The Nexus file id of the version in use, when the archive still names it.
    pub(super) current_file_id: Option<u32>,
    /// The page picture already saved for the family: the same address is not fetched again.
    pub(super) picture_url: Option<String>,
    /// The newest file of the family that was downloaded from Nexus, whose requirements Nexus lists.
    pub(super) file_id: Option<u32>,
    /// The persistent `mod_file_id` of this family, when the source names one.
    pub(super) part_id: Option<u32>,
    /// The archives of the family that came in without a Nexus link: their digest and the name they came in with.
    pub(super) unmatched: Vec<(String, String)>,
    /// The last read of those requirements, which a recent read of the same file makes unnecessary.
    pub(super) known: Option<NexusRequiresSync>,
    pub(super) saved_meta: Option<SourceMeta>,
    pub(super) pictures_dir: PathBuf,
}

pub(super) fn lookup(request: &SourceUpdateRequest) -> Result<Lookup, AppError> {
    let nexus_id: u32 = request
        .source
        .mod_id
        .parse()
        .map_err(|_| crate::providers::nexus::bad_id(&request.source.mod_id))?;
    let picture_url = request
        .saved_meta
        .as_ref()
        .filter(|meta| {
            meta.picture
                .as_ref()
                .is_some_and(|file| Path::new(&request.pictures_dir).join(file).is_file())
        })
        .and_then(|meta| meta.picture_url.clone());
    // Nexus file ids grow with each upload, so the largest one is the newest file.
    let file_id = request
        .source
        .file_id
        .as_deref()
        .and_then(|id| id.parse().ok())
        .or_else(|| {
            request
                .archives
                .iter()
                .filter_map(|archive| archive.file_id.as_deref()?.parse().ok())
                .max()
        });
    let part_id = request
        .source
        .part_id
        .as_deref()
        .and_then(|id| id.parse().ok())
        .or_else(|| {
            request
                .archives
                .iter()
                .find_map(|archive| archive.part_id.as_deref()?.parse().ok())
        });
    let unmatched = request
        .archives
        .iter()
        .filter(|archive| archive.file_id.is_none())
        .map(|archive| (archive.sha256.clone(), archive.original_name.clone()))
        .collect();
    let known = request
        .saved_sync
        .clone()
        .and_then(|sync| sync.try_into().ok());
    Ok(Lookup {
        family: request.family.clone(),
        source: request.source.clone(),
        nexus_id,
        current: request.current.clone(),
        current_file_id: request
            .current_file_id
            .as_deref()
            .and_then(|id| id.parse().ok()),
        picture_url,
        file_id,
        part_id,
        unmatched,
        known,
        saved_meta: request.saved_meta.clone(),
        pictures_dir: request.pictures_dir.clone(),
    })
}
