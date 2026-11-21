//! Installing catalog definitions: download a checked file, validate the merged definition in a throwaway folder and
//! only then move the files into the data folder. Nothing a rejected id downloaded reaches the live folders.

use std::path::{Path, PathBuf};

use sha2::Sha256;

use crate::error::{AppError, FileError, io_at};
use crate::infra::http::{PartError, RateRetry, download_part};
use crate::infra::io::digest_file;
use crate::infra::store;

use super::manifest::{self, RemoteCatalog};
use super::{CatalogInstallOutcome, CatalogInstallResult, CatalogInstallStep, Folders};

/// Downloads and places every selected id, reporting one outcome per id. A refused id does not stop the batch; only a
/// failure to read the manifest at all fails the whole command.
pub(super) fn install(
    manifest_url: &str,
    base_url: &str,
    folders: &Folders,
    ids: &[String],
    progress: &mut dyn FnMut(CatalogInstallStep),
) -> Result<Vec<CatalogInstallOutcome>, AppError> {
    let catalog_games = folders.catalog_games.as_deref().ok_or_else(|| {
        AppError::new("catalog.unsupported").with(
            "detail",
            "the data folder is not set up for catalog definitions",
        )
    })?;
    let catalog = manifest::fetch_catalog(manifest_url)?;

    // The staging folder sits under the data folder so the final moves are renames on one filesystem, not copies.
    let work_root = catalog_games.parent().unwrap_or_else(|| Path::new("."));
    let work = tempfile::Builder::new()
        .prefix(".catalog-work-")
        .tempdir_in(work_root)
        .map_err(AppError::unexpected)?;

    let mut outcomes = Vec::with_capacity(ids.len());

    for id in ids {
        let (result, note) = match install_one(
            id,
            base_url,
            folders,
            catalog_games,
            &catalog,
            work.path(),
            progress,
        ) {
            Ok(note) => (CatalogInstallResult::Installed, note),
            Err(error) => (CatalogInstallResult::Failed { error }, None),
        };
        outcomes.push(CatalogInstallOutcome {
            id: id.clone(),
            result,
            note,
        });
    }

    drop(work);
    if outcomes
        .iter()
        .any(|outcome| matches!(outcome.result, CatalogInstallResult::Installed))
    {
        super::super::forget_all_definitions();
    }
    Ok(outcomes)
}

/// Downloads, validates and places one id. `Ok(note)` is an install with a harmless detail, such as a loader the
/// manifest does not list. The returned note is empty for a plain install.
fn install_one(
    id: &str,
    base_url: &str,
    folders: &Folders,
    catalog_games: &Path,
    catalog: &RemoteCatalog,
    work: &Path,
    progress: &mut dyn FnMut(CatalogInstallStep),
) -> Result<Option<String>, AppError> {
    if !super::super::is_valid_id(id) {
        return Err(AppError::user("catalog.invalid").with("id", id).with(
            "detail",
            "an id can only contain lowercase letters, digits and '-'",
        ));
    }

    let entry = catalog
        .game(id)
        .ok_or_else(|| AppError::user("catalog.unknown").with("id", id))?;

    let game_staged = work.join("game.toml");
    download_verified(
        &join_url(base_url, &entry.file),
        &entry.sha256,
        &game_staged,
        id,
        progress,
    )?;

    let icon_staged = match (entry.icon.as_deref(), entry.icon_sha256.as_deref()) {
        (Some(icon), Some(sha)) => {
            let staged = work.join("icon.webp");
            download_verified(&join_url(base_url, icon), sha, &staged, id, progress)?;
            Some(staged)
        }
        _ => None,
    };

    let mut note = None;
    let mut loader_staged: Option<(&str, PathBuf)> = None;
    if let Some(loader_id) = entry.loader.as_deref() {
        if !super::super::is_valid_id(loader_id) {
            return Err(AppError::user("catalog.invalid")
                .with("id", loader_id)
                .with(
                    "detail",
                    "a loader id can only contain lowercase letters, digits and '-'",
                ));
        }

        match catalog.loader(loader_id) {
            None => {
                note = Some(format!("loader {loader_id} is not in the catalog"));
            }
            Some(loader) if loader_needs_install(loader_id, loader, folders)? => {
                let loaders = work.join("loaders");
                std::fs::create_dir_all(&loaders).map_err(io_at(&loaders))?;
                let staged = loaders.join(format!("{loader_id}.toml"));
                download_verified(
                    &join_url(base_url, &loader.file),
                    &loader.sha256,
                    &staged,
                    id,
                    progress,
                )?;
                loader_staged = Some((loader_id, loaders));
            }
            Some(_) => {}
        }
    }

    // Validate exactly the folder order the app will serve after the install: user, this download, official, bundled.
    let mut loader_dirs: Vec<PathBuf> = Vec::with_capacity(4);
    if let Some(user) = &folders.user_loaders {
        loader_dirs.push(user.clone());
    }
    if let Some((_, loaders)) = &loader_staged {
        loader_dirs.push(loaders.clone());
    }
    if let Some(official) = &folders.catalog_loaders {
        loader_dirs.push(official.clone());
    }
    loader_dirs.push(folders.bundled_loaders.clone());

    if let Err(error) = super::super::read_definition_with(&game_staged, &loader_dirs) {
        return Err(AppError::user("catalog.invalid")
            .with("file", game_staged.display())
            .with(
                "detail",
                error
                    .params
                    .get("detail")
                    .cloned()
                    .unwrap_or_else(|| error.code.clone()),
            ));
    }

    progress(CatalogInstallStep::Installing);

    // The loader goes first: a game placed without the preset it names would not load.
    if let Some((loader_id, loaders)) = loader_staged {
        let official = folders.catalog_loaders.as_deref().ok_or_else(|| {
            AppError::new("catalog.unsupported")
                .with("detail", "the catalog loaders folder is not available")
        })?;
        let file = format!("{loader_id}.toml");
        std::fs::create_dir_all(official).map_err(io_at(official))?;
        store::replace_file_clean(&loaders.join(&file), &official.join(&file))?;
    }

    let game_dir = catalog_games.join(id);
    std::fs::create_dir_all(&game_dir).map_err(io_at(&game_dir))?;
    store::replace_file_clean(&game_staged, &game_dir.join("game.toml"))?;

    if let Some(icon_staged) = icon_staged {
        store::replace_file_clean(&icon_staged, &game_dir.join("icon.webp"))?;
    }

    super::mark_app_version(catalog_games)?;
    Ok(note)
}

/// Whether a game's loader preset is missing locally or no longer matches the catalog. A user preset still wins at
/// run time, but the official copy is kept in step with the catalog so the other games get the current one too.
fn loader_needs_install(
    id: &str,
    expected: &manifest::RemoteLoader,
    folders: &Folders,
) -> Result<bool, AppError> {
    let dirs = [
        folders.user_loaders.as_deref(),
        folders.catalog_loaders.as_deref(),
        Some(folders.bundled_loaders.as_path()),
    ];
    let local = dirs
        .into_iter()
        .flatten()
        .map(|dir| dir.join(format!("{id}.toml")))
        .find(|path| path.is_file());

    match local {
        None => Ok(true),
        Some(path) => {
            let sha = digest_file::<Sha256>(&path, &mut |_| {})
                .map_err(|error| FileError::new(&path, error))?;
            Ok(!sha.eq_ignore_ascii_case(&expected.sha256))
        }
    }
}

/// Downloads one file, stages it at `staged` and checks its SHA-256. A mismatch usually means GitHub served a newer
/// file than the cached manifest named; it is a retryable error, and a later check sees the two in step again.
fn download_verified(
    url: &str,
    sha: &str,
    staged: &Path,
    id: &str,
    progress: &mut dyn FnMut(CatalogInstallStep),
) -> Result<(), AppError> {
    let mut report = |received: u64, total: u64| {
        progress(CatalogInstallStep::Downloading {
            percent: percent(received, total),
        });
        true
    };
    let dir = staged.parent().unwrap_or_else(|| Path::new("."));
    let part = download_part(dir, url, &mut RateRetry::default(), &mut || {}, &mut report)
        .map_err(part_error)?;
    part.keep_as(staged)?;
    let digest = digest_file::<Sha256>(staged, &mut |_| {})
        .map_err(|error| FileError::new(staged, error))?;

    if digest.eq_ignore_ascii_case(sha) {
        Ok(())
    } else {
        Err(AppError::retryable("catalog.changed").with("id", id))
    }
}

/// The share of a file received, or `None` when the server sent no length.
fn percent(received: u64, total: u64) -> Option<u8> {
    if total == 0 {
        None
    } else {
        Some(
            u8::try_from(crate::domain::progress::percent(received, total).unwrap_or(100))
                .unwrap_or(100),
        )
    }
}

/// A file download that failed or was refused is a retryable catalog read, whatever the transport said.
fn part_error(error: PartError) -> AppError {
    match error {
        PartError::Failed(error) => manifest::fetch_error(error),
        PartError::Body(problem) => manifest::fetch_error(format!("refused body: {problem:?}")),
    }
}

/// Joins a catalog-relative file path to the base address without doubling a slash.
fn join_url(base: &str, file: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        file.trim_start_matches('/')
    )
}
