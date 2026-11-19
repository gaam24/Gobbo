//! Downloading and installing one provider file: a single update or a requirement.

use std::sync::atomic::AtomicBool;

use crate::acquire;
use crate::context::Data;
use crate::domain::fomod::Choice;
use crate::error::AppError;
use crate::games;
use crate::import::IMPORT_CANCELLED;
use crate::infra::tasks::{cancelled, stop_if};
use crate::providers;

use super::{UpdateProgress, UpdateStage};

/// Downloads the newest file of one source and installs it as the active profile's selected version. The account's
/// download route is checked first: a provider that cannot download, a missing key or a saved account without direct
/// downloads becomes its usual stable error (`source.payment_required` for the last one), and the window opens the files
/// page. A flag stops the download or the import of the new version; a stopped update installs nothing and leaves the
/// selected version as it was.
pub fn update_mod_cancellable(
    root: &Data,
    game_id: &str,
    id: &str,
    provider_id: &str,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(UpdateProgress),
) -> Result<(), AppError> {
    stop_if(cancel, IMPORT_CANCELLED)?;
    let (source_ref, latest) = super::with_family(root, game_id, id, |_, _, family| {
        let provider = providers::require(provider_id)?;
        let source = family
            .sources
            .get(&provider.id())
            .ok_or_else(|| crate::context::library::not_in_library(id))?;
        let latest = family
            .latest
            .get(&provider.id())
            .cloned()
            .ok_or_else(|| AppError::user("source.no_version").with("id", id))?;
        let mut source_ref = source.source(&provider.id());
        if let Some(file_id) = family.latest_file_ids.get(&provider.id()) {
            source_ref = source_ref.with_file(file_id.clone());
        }
        Ok((source_ref, latest))
    })?;

    // The new version is installed with the FOMOD options of the version the profile uses now; an installer the user
    // never answered for this mod asks the window for the wizard instead of installing every option.
    let kept = selected_choices(root, game_id, id)?;
    let folders = download_and_import(
        root,
        game_id,
        provider_id,
        source_ref,
        Some(latest.as_str()),
        kept,
        cancel,
        progress,
    )?;
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = crate::context::load_library(&data, &game)?;
    let folder = folders
        .iter()
        .find(|folder| {
            library
                .versions
                .get(*folder)
                .is_some_and(|version| version.family == id)
        })
        .or_else(|| folders.first())
        .cloned()
        .ok_or_else(|| AppError::new("import.empty"))?;

    progress(UpdateProgress::at(UpdateStage::Selecting, 1, 1));
    crate::profiles::actions::set_mod_version(root, game_id, id, &folder)
}

/// The FOMOD choices of the version the active profile selects for `family`, empty when it has none.
fn selected_choices(root: &Data, game_id: &str, family: &str) -> Result<Vec<Choice>, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = crate::context::load_library_shared(&data, &game)?;
    let profiles = crate::context::load_profiles(&data)?;
    Ok(profiles
        .active()
        .mods
        .iter()
        .find(|entry| entry.id == family)
        .and_then(|entry| library.versions.get(&entry.folder))
        .and_then(|version| version.installer_choices())
        .map(<[Choice]>::to_vec)
        .unwrap_or_default())
}

/// Downloads one source and imports it through the shared `acquire` path, and answers the library folders that hold
/// it: the new ones, or the folder that already had this exact file. `update_mod` and a requirement both need it, so
/// the provider check and the progress rule live here once. An installer archive without `kept` choices fails with
/// `import.needs_choice`, naming the downloaded file for the window's wizard.
#[allow(
    clippy::too_many_arguments,
    reason = "a download has the source, its archive name, the kept choices, cancellation and progress"
)]
pub fn download_and_import(
    root: &Data,
    game_id: &str,
    provider_id: &str,
    source: crate::domain::source::SourceRef,
    archive_name: Option<&str>,
    kept: Vec<Choice>,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(UpdateProgress),
) -> Result<Vec<String>, AppError> {
    stop_if(cancel, IMPORT_CANCELLED)?;
    let provider = providers::require(provider_id)?;
    if let Some(error) = providers::browser_route_error(provider) {
        return Err(error);
    }
    progress(UpdateProgress::at(UpdateStage::Downloading, 0, 0));
    let request = acquire::Request {
        origin: acquire::Origin::Fetch { archive_name },
        source: Some(source),
        choices: acquire::Choices::Stored(kept),
    };
    let acquired = acquire::acquire(root, game_id, request, cancel, &mut |phase, percent| {
        let stage = match phase {
            acquire::Phase::Downloading => UpdateStage::Downloading,
            acquire::Phase::Importing(_) => UpdateStage::Importing,
        };
        progress(UpdateProgress { stage, percent });
    })
    .map_err(|error| cancelled_as_import(error, cancel))?;
    match acquired {
        acquire::Acquired::Imported(view) => Ok(view.folders),
        acquire::Acquired::Existing { folder } => Ok(vec![folder]),
        acquire::Acquired::NeedsChoice { path, .. } => Err(acquire::needs_choice_error(&path)),
    }
}

/// A stopped download reports the import's stop code, because the window treats an update as one import.
fn cancelled_as_import(error: AppError, cancel: Option<&AtomicBool>) -> AppError {
    if cancelled(cancel) {
        AppError::user(IMPORT_CANCELLED)
    } else {
        error
    }
}
