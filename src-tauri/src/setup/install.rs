//! The confirmed install of a game's loader package: download and verify the release, import it into the library as one
//! mod, and enable it in the active profile. The game folder is never written; the overlay serves the loader.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use sha2::Sha256;

use crate::acquire::{Acquired, Choices, Origin, Request, acquire};
use crate::context::{Data, Settings};
use crate::domain::library::ModInfoEdit;
use crate::error::{AppError, io_at};
use crate::games::{GameDef, Install};
use crate::infra::http::download;
use crate::infra::io::digest_file;
use crate::infra::tasks::{cancelled, stop_if};

/// A loader release is small. The limit stops a damaged or hostile link from filling the disk.
const MAX_BYTES: u64 = 512 * 1024 * 1024;

/// Installs the game's loader package. Only a user click calls this. A copy already in the library is enabled as it is;
/// otherwise the verified release is downloaded and imported first. `progress` gets the download percent.
pub fn install(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u32),
) -> Result<super::LoaderSetupView, AppError> {
    let install = game
        .install
        .as_ref()
        .ok_or_else(|| AppError::user("loader.none"))?;
    let game_data = data.game(&game.id);
    let library = crate::context::load_library_shared(&game_data, game)?;
    // A copy of this version, or one the user imported by hand without a version, is used as it is.
    let reusable = super::library_copy(&library, install)
        .filter(|copy| {
            copy.info
                .version
                .as_deref()
                .is_none_or(|version| version == install.version)
        })
        .map(|copy| copy.folder.clone());
    drop(library);

    let folder = if let Some(folder) = reusable {
        folder
    } else {
        let folder = download_and_import(data, game, install, cancel, progress)?;
        let family = family_of(data, game, &folder)?;
        crate::library::set_mod_info(
            data,
            &game.id,
            &family,
            &folder,
            ModInfoEdit {
                name: Some(install.name.clone()),
                version: Some(install.version.clone()),
                ..ModInfoEdit::default()
            },
        )?;
        folder
    };

    // A new mod joins the profiles turned off; adopt it, then turn it on in the active profile.
    crate::status::sync_profiles(data, &game.id)?;
    let family = family_of(data, game, &folder)?;
    crate::profiles::edits::set_mod_enabled(data, &game.id, &family, true)?;

    super::view(data, game, settings)?
        .ok_or_else(|| AppError::unexpected("the installed loader disappeared from the definition"))
}

/// Downloads the release, checks its size and SHA-256, and brings it into the library through `acquire`, like every
/// other archive. The importer recognises the release and keeps its game paths. An archive that is already in the
/// library answers with that copy's folder.
fn download_and_import(
    data: &Data,
    game: &GameDef,
    install: &Install,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u32),
) -> Result<String, AppError> {
    let downloads = data.game(&game.id).dir.join("loader-downloads");
    let result = (|| {
        fs::create_dir_all(&downloads).map_err(io_at(&downloads))?;
        stop_if(Some(cancel), crate::error::DOWNLOAD_CANCELLED)?;
        let path = downloads.join(file_name(&install.url));
        let limit = install.size.unwrap_or(MAX_BYTES).min(MAX_BYTES);
        let mut too_large = false;
        let downloaded = download(&install.url, &path, &mut |received, _| {
            if received > limit {
                too_large = true;
                return false;
            }

            if let Some(percent) = received.saturating_mul(100).checked_div(limit) {
                progress(u32::try_from(percent).unwrap_or(100));
            }

            !cancelled(Some(cancel))
        });

        if too_large {
            return Err(AppError::user("loader.too_large").with("path", &install.url));
        }

        downloaded?;
        verify(&path, install)?;
        let request = Request {
            origin: Origin::Download(&path),
            source: None,
            choices: Choices::Ask,
        };

        match acquire(data, &game.id, request, Some(cancel), &mut |_, _| {})? {
            Acquired::Imported(view) => view
                .folders
                .into_iter()
                .next()
                .ok_or_else(|| AppError::unexpected("the loader import added no folder")),
            Acquired::Existing { folder } => Ok(folder),
            Acquired::NeedsChoice { .. } => Err(AppError::unexpected(
                "a loader release has no installer to answer",
            )),
        }
    })();
    let _ = fs::remove_dir_all(&downloads);
    result
}

fn family_of(data: &Data, game: &GameDef, folder: &str) -> Result<String, AppError> {
    crate::context::load_library_shared(&data.game(&game.id), game)?
        .versions
        .get(folder)
        .map(|version| version.family.clone())
        .ok_or_else(|| AppError::unexpected("the imported loader is not in the library"))
}

/// The archive's file name from its link, so the library names the mod after the release.
fn file_name(url: &str) -> String {
    url.split(['?', '#'])
        .next()
        .and_then(|url| url.rsplit('/').next())
        .filter(|name| !name.is_empty())
        .unwrap_or("loader.zip")
        .to_string()
}

fn verify(path: &Path, install: &Install) -> Result<(), AppError> {
    let bytes = fs::metadata(path).map_err(io_at(path))?.len();

    if let Some(expected) = install.size
        && expected != bytes
    {
        return Err(AppError::user("loader.size")
            .with("expected", expected.to_string())
            .with("actual", bytes.to_string()));
    }

    let digest = digest_file::<Sha256>(path, &mut |_| {}).map_err(io_at(path))?;

    if !digest.eq_ignore_ascii_case(&install.sha256) {
        return Err(AppError::user("loader.hash")
            .with("expected", &install.sha256)
            .with("actual", digest));
    }

    Ok(())
}
