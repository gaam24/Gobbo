//! The requirement install: the whole plan or one row, and the download that resolves a source into a family.

use std::sync::atomic::AtomicBool;

use crate::context::profile::lock_profiles;
use crate::context::{Data, load_library, load_profiles};
use crate::domain::source::SourceRef;
use crate::error::AppError;
use crate::games::{self, GameDef};
use crate::import::IMPORT_CANCELLED;
use crate::infra::tasks::stop_if;
use crate::library::{LibraryView, sync};
use crate::providers;
use crate::sources::update::{UpdateProgress, download_and_import};

use super::model::{RequirementAction, RequirementInstallView};
use super::plan::plan;

/// Installs every installable step of the plan and saves the profile list once, so the whole closure is one undo.
/// A blocked step is reported; a failed download does not stop the rest. A flag is checked before each download and
/// before the one profile save, so a stopped run enables nothing and is never left half applied.
pub fn install_cancellable(
    root: &Data,
    game_id: &str,
    folder: &str,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(UpdateProgress),
) -> Result<RequirementInstallView, AppError> {
    stop_if(cancel, IMPORT_CANCELLED)?;
    let plan = plan(root, game_id, folder)?;
    let game = games::load(game_id)?;
    let mut resolved: Vec<(String, Option<String>)> = Vec::new();
    let mut failed = Vec::new();

    for step in &plan.steps {
        match step.action {
            RequirementAction::Enable => {
                if let Some(family) = &step.family {
                    resolved.push((family.clone(), None));
                }
            }
            RequirementAction::Download => {
                stop_if(cancel, IMPORT_CANCELLED)?;
                let Some(source) = &step.source else {
                    continue;
                };

                match resolve_cancellable(root, game_id, source.clone(), cancel, progress) {
                    Ok(item) => resolved.push((item.family, item.folder)),
                    Err(error) => failed.push(error.into()),
                }
            }
            RequirementAction::AlreadyOn | RequirementAction::Blocked => {}
        }
    }

    stop_if(cancel, IMPORT_CANCELLED)?;

    let installed = u32::try_from(resolved.len()).unwrap_or(u32::MAX);

    if !resolved.is_empty() {
        enable_all(root, &game, &resolved)?;
    }

    let blocked = plan
        .steps
        .iter()
        .filter(|step| step.action == RequirementAction::Blocked)
        .filter_map(|step| step.message.clone())
        .chain(plan.cycles.iter().cloned())
        .collect();
    Ok(RequirementInstallView {
        status: crate::status::status(root, game_id)?,
        installed,
        blocked,
        failed,
    })
}

/// Installs one requirement or enables one already in the library, and selects its newest version. This is the
/// single-row action; `install_cancellable` above does the same for a whole plan with one profile save. A flag is
/// checked around the download and before the profile save.
pub fn install_one_cancellable(
    root: &Data,
    game_id: &str,
    source: SourceRef,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(UpdateProgress),
) -> Result<(), AppError> {
    stop_if(cancel, IMPORT_CANCELLED)?;
    let game = games::load(game_id)?;
    let resolved = resolve_cancellable(root, game_id, source, cancel, progress)?;
    stop_if(cancel, IMPORT_CANCELLED)?;
    enable_all(root, &game, &[(resolved.family, resolved.folder)])
}

/// Enables every resolved family in the active profile and saves the list once, so the whole closure is one undo.
fn enable_all(
    root: &Data,
    game: &GameDef,
    resolved: &[(String, Option<String>)],
) -> Result<(), AppError> {
    let data = root.game(&game.id);
    let library = load_library(&data, game)?;
    let view = LibraryView::new(&library);
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    sync::sync_profiles(&mut profiles, &library, &view);

    for (family, folder) in resolved {
        let entry = profiles.active_entry_mut(family)?;
        entry.enabled = true;

        if let Some(folder) = folder {
            entry.folder.clone_from(folder);
        }
    }

    crate::context::profile::history::save_profiles(&data, &profiles)
}

struct Resolved {
    family: String,
    folder: Option<String>,
}

/// The family of a requirement, downloading and importing it when the library does not have it yet. The shared
/// download respects the flag at each chunk and at the import files.
fn resolve_cancellable(
    root: &Data,
    game_id: &str,
    source: SourceRef,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(UpdateProgress),
) -> Result<Resolved, AppError> {
    stop_if(cancel, IMPORT_CANCELLED)?;
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let provider = providers::require(source.provider.as_str())?;
    let library = load_library(&data, &game)?;
    let view = LibraryView::new(&library);
    // The exact provider file is the one "already present" rule when the requirement names a file. A family from a
    // different file of the same page is a different version, so that file is still fetched and imported through
    // `acquire`. A requirement without a file id names only a page, so the family that came from that page is the one
    // to enable; it cannot name a version to compare with.
    let existing = if source.file_id.is_some() {
        crate::acquire::existing_file(&view, &source)
            .and_then(|folder| view.version(&folder).map(|version| version.family.clone()))
    } else {
        view.family_of_source(&source.provider, &source.mod_id)
            .map(str::to_string)
    };

    if let Some(family) = existing {
        return Ok(Resolved {
            family,
            folder: None,
        });
    }

    let folders = download_and_import(
        root,
        game_id,
        provider.id().as_str(),
        source,
        None,
        Vec::new(),
        cancel,
        progress,
    )?;
    let folder = folders
        .into_iter()
        .next()
        .ok_or_else(|| AppError::new("import.empty"))?;
    let family = load_library(&data, &game)?.version(&folder)?.family.clone();
    Ok(Resolved {
        family,
        folder: Some(folder),
    })
}
