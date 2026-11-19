//! Downloading the mods that a pack lists but does not include.

use super::PACK_CANCELLED;
use super::import::import_pack_with_progress;
use super::inspect::open_pack;
use super::model::PackImportView;
use crate::acquire::{Acquired, BatchRequest, Choices, Origin, existing_file};
use crate::context::Data;
use crate::context::Loaded;
use crate::context::load_game;
use crate::domain::pack::PackMod;
use crate::domain::source::{DownloadRoute, SourceRef};
use crate::error::AppError;
use crate::import::ImportRequest;
use crate::infra::sidecar::delete_download;
use crate::infra::tasks::{cancelled, stop_if};
use crate::library::LibraryView;
use crate::providers;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// The owned parts of a batched download request, kept alive while `acquire` borrows them.
type OwnedDownload = (
    PathBuf,
    Option<SourceRef>,
    Option<Vec<crate::domain::fomod::Choice>>,
    Option<String>,
);

/// The download of one mod of a pack, for the progress bar.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PackDownload {
    pub name: String,
    /// Which mod of how many, from 1.
    pub index: u32,
    pub total: u32,
    pub percent: u32,
}

/// A source of a pack mod that its provider downloads directly right now, with that provider. The route is the one
/// every download reads: an account the backend routed to the files page (a free Nexus account, or one that just
/// answered `payment_required`) is left for the user to fetch by hand, as is a provider that cannot download or a mod
/// with no source.
fn download_source(entry: &PackMod) -> Option<SourceRef> {
    entry.sources.keys().find_map(|provider| {
        let adapter = providers::provider(provider.as_str())?;
        if providers::download_route(adapter) != DownloadRoute::Direct {
            return None;
        }
        entry.source_ref(provider)
    })
}

/// Deletes the downloads of a pack that were not imported yet. A file that an import already removed is missing, and
/// the delete does nothing for it.
fn discard_downloads(downloads: &[(String, ImportRequest)]) {
    for (_, request) in downloads {
        let _ = delete_download(&request.zip);
    }
}

/// Stops the pack download with `pack.cancelled` when the flag asks for it, and deletes the downloads of this run that
/// were not imported yet first. Without a stop it does nothing.
fn stop_downloads(
    cancel: Option<&AtomicBool>,
    downloads: &[(String, ImportRequest)],
) -> Result<(), AppError> {
    if cancelled(cancel) {
        discard_downloads(downloads);
    }
    stop_if(cancel, PACK_CANCELLED)
}

/// Imports the archives a pack download brought in through the shared `acquire` path, in one batch. Returns the names
/// of the mods that failed. A successful import forgets its download; a file the library already holds is deleted as a
/// duplicate by `acquire`, and does not count as a failure; any other failed one, including an archive that still needs
/// wizard answers, keeps the file for another try. A stopped batch deletes the downloads it has not imported.
pub fn import_downloaded(
    root: &Data,
    game_id: &str,
    downloads: Vec<(String, ImportRequest)>,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<String>, AppError> {
    if downloads.is_empty() {
        return Ok(Vec::new());
    }
    let mut labels: Vec<String> = Vec::with_capacity(downloads.len());
    let mut owned: Vec<OwnedDownload> = Vec::with_capacity(downloads.len());
    for (label, request) in downloads {
        labels.push(label);
        owned.push((request.zip, request.source, request.choices, request.staged));
    }
    let batch: Vec<BatchRequest<'_>> = owned
        .iter()
        .map(|(path, source, choices, staged)| BatchRequest {
            origin: Origin::Download(path),
            source: source.clone(),
            choices: choices.clone().map_or(Choices::Ask, Choices::Given),
            staged: staged.clone(),
        })
        .collect();
    let results = crate::acquire::acquire_batch(root, game_id, batch, cancel, &mut |_, _| {})
        .map_err(super::import::pack_stop)?;
    let mut failed = Vec::new();
    for (label, result) in labels.into_iter().zip(results) {
        match result {
            Ok(Acquired::Imported(_) | Acquired::Existing { .. }) => {}
            Ok(Acquired::NeedsChoice { .. }) | Err(_) => failed.push(label),
        }
    }
    Ok(failed)
}

/// Downloads the mods of a pack that the library lacks, straight from a provider that allows it (for Nexus, a premium
/// account is required), imports them in one batch with the FOMOD choices the pack keeps, and then imports the pack, so
/// its profiles come out complete.
pub fn download_pack_mods(
    root: &Data,
    game_id: &str,
    path: &Path,
    progress: &mut dyn FnMut(PackDownload),
    cancel: Option<&AtomicBool>,
) -> Result<PackImportView, AppError> {
    let Loaded {
        game,
        data,
        library,
        ..
    } = load_game(root, game_id)?;
    let view = LibraryView::new(&library);
    let opened = open_pack(game_id, path)?;
    let mut seen = BTreeSet::new();
    // Only the exact provider file decides whether a mod is already here. A family that came from another file of the
    // same page is not this mod, so it is fetched like any other missing one.
    let wanted: Vec<(&PackMod, SourceRef)> = opened
        .manifest
        .profiles
        .iter()
        .flat_map(|profile| &profile.mods)
        .filter(|entry| seen.insert(entry.family.as_str()))
        .filter_map(|entry| Some((entry, download_source(entry)?)))
        .filter(|(_, source)| existing_file(&view, source).is_none())
        .filter(|(entry, _)| {
            !entry
                .archive
                .as_ref()
                .is_some_and(|archive| opened.archives.contains_key(&archive.sha256))
        })
        .collect();
    let total = u32::try_from(wanted.len()).unwrap_or(u32::MAX);
    let mut failed = Vec::new();
    let mut downloads = Vec::new();
    for (index, (entry, source)) in wanted.iter().enumerate() {
        stop_downloads(cancel, &downloads)?;
        let report = &mut |percent| {
            progress(PackDownload {
                name: entry.name.clone(),
                index: u32::try_from(index + 1).unwrap_or(u32::MAX),
                total,
                percent,
            });
        };
        let fetched = providers::fetch::fetch(
            providers::fetch::Fetch {
                game_key: game.provider_key(source.provider.as_str()),
                source: source.clone(),
                directory: &data.downloads(),
                archive_name: entry.archive.as_ref().map(|archive| archive.name.as_str()),
                download_key: None,
            },
            cancel,
            report,
        );
        let Ok(downloaded) = fetched else {
            stop_downloads(cancel, &downloads)?;
            failed.push(entry.name.clone());
            continue;
        };
        report(100);
        let choices = entry.installer_choices().map(<[_]>::to_vec);
        downloads.push((
            entry.name.clone(),
            ImportRequest::new(downloaded.path)
                .with_choices(choices)
                .with_source(Some(downloaded.source)),
        ));
    }
    failed.extend(import_downloaded(root, game_id, downloads, cancel)?);
    stop_if(cancel, PACK_CANCELLED)?;
    let mut view = import_pack_with_progress(root, game_id, path, &mut |_, _| {}, cancel)?;
    view.failed.extend(failed);
    Ok(view)
}
