//! The confirmed install of a game's loader package: pick a copy in the library or download and verify the official
//! release, import it into the library as one mod, and make it the one enabled copy in the active profile. The game
//! folder is never written; the overlay serves the loader.

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

use super::LoaderChoice;

/// A loader release is small. The limit stops a damaged or hostile link from filling the disk.
const MAX_BYTES: u64 = 512 * 1024 * 1024;

/// Turns on one copy of the game's loader in the active profile and turns off every other copy, so two loaders never
/// fight over the same proxy. Only a user click calls this. `choice` decides the copy: a library copy the user picked
/// (for example a build a mod site publishes for this game), the official release (downloaded and verified when the
/// library does not hold it), or, for the quick action, a copy already in the library before a download. `progress`
/// gets the download percent.
pub fn install(
    data: &Data,
    game: &GameDef,
    settings: &Settings,
    choice: &LoaderChoice,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u32),
) -> Result<super::LoaderSetupView, AppError> {
    let install = game
        .install
        .as_ref()
        .ok_or_else(|| AppError::user("loader.none"))?;
    let game_data = data.game(&game.id);
    let library = crate::context::load_library_shared(&game_data, game)?;
    let profiles = crate::context::data::load_profiles_shared(&game_data)?;
    let copies = super::copies(&library, profiles.active(), install);
    drop((library, profiles));

    let picked = match choice {
        LoaderChoice::Library { folder } => Some(
            copies
                .iter()
                .find(|copy| copy.folder == *folder)
                .map(|copy| copy.folder.clone())
                .ok_or_else(|| AppError::user("loader.copy_unknown").with("folder", folder))?,
        ),
        LoaderChoice::Official => copies
            .iter()
            .find(|copy| copy.official)
            .map(|copy| copy.folder.clone()),
        // The copies are sorted: the enabled one, the official release, then the newest import.
        LoaderChoice::Auto => copies.first().map(|copy| copy.folder.clone()),
    };

    let folder = if let Some(folder) = picked {
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

    // A new mod joins the profiles turned off; adopt it, then make it the one enabled copy in the active profile.
    crate::status::sync_profiles(data, &game.id)?;
    let others: Vec<String> = copies
        .iter()
        .filter(|copy| copy.folder != folder)
        .map(|copy| copy.folder.clone())
        .collect();
    crate::profiles::edits::use_only(data, &game.id, &folder, &others)?;

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

            if let Some(percent) = crate::domain::progress::percent(received, limit) {
                progress(percent);
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

/// The archive's file name from its link, so the library names the mod after the release. The link's query and fragment
/// are not part of the name; a link without a file name gets a stable one for the download.
fn file_name(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    crate::domain::naming::file_name_from_url(path)
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
