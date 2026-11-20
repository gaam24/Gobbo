//! Unpacking a mod again from its archive copy.

use crate::context::Data;
use crate::context::edit_library;
use crate::context::load_library;
use crate::context::load_profiles;
use crate::context::profile::lock_profiles;
use crate::domain::profiles::Profiles;
use crate::error::AppError;
use crate::error::ErrorKind;
use crate::error::io_at;
use crate::games;
use crate::import::install::Naming;
use crate::import::model::ZipInspect;
use crate::infra::archive;
use crate::library::Library;

use super::archive_base_name;
use super::wizard::fomod_allowed;

/// Unpacks a mod again from its archive copy, for example after a file in its folder changed or the folder is gone.
/// The old folder is set aside and dropped only once the new one is in place; a failure puts it back. The index keeps
/// the mod's data: its tags, notes, edits, and the profiles that use it. A game with a prepared deployment is refused.
/// The application reinstalls through `reinstall_mod_with` (the deploy module undoes the deployment first); this
/// convenience wrapper is for tests, like `install_zip`.
#[cfg(test)]
pub fn reinstall_mod(root: &Data, game_id: &str, folder: &str) -> Result<(), AppError> {
    reinstall_mod_with(root, game_id, folder, None, |data, _| data.ensure_idle())
}

/// Unpacks a mod again, as `reinstall_mod` does. `choices` are new FOMOD answers; `None` keeps the ones the version was
/// installed with. `undo_deployments` runs once the archive is known to be usable and before the folder is replaced,
/// with the profiles, and the profiles are saved after it: a prepared deployment that holds the mod is undone there.
/// The deploy module supplies it (`deploy::remove::reinstall_mod`).
pub fn reinstall_mod_with(
    root: &Data,
    game_id: &str,
    folder: &str,
    choices: Option<&[crate::import::fomod::Choice]>,
    undo_deployments: impl FnOnce(&Data, &mut Profiles) -> Result<(), AppError>,
) -> Result<(), AppError> {
    let _state = crate::context::lock_state(&root.dir)?;
    edit_library(root, game_id, |data, game, library| {
        let (record, zip, file) = archive_for(data, library, folder)?;
        let digest = record
            .archive
            .clone()
            .ok_or_else(|| archive_error(folder, "library.no_archive"))?;
        // The FOMOD answers come from the shared acquire rule: answers the user gave now win, otherwise the ones the
        // version was installed with are kept. A reinstall cannot show a wizard: an empty kept list is an installer
        // with no picked options, not a request to ask, so it is applied as given. A plain archive has no kept list and
        // is unpacked whole.
        let answers = choices.map_or_else(
            || match record.installer_choices() {
                Some([]) => crate::acquire::Choices::Given(Vec::new()),
                Some(kept) => crate::acquire::Choices::Stored(kept.to_vec()),
                None => crate::acquire::Choices::Ask,
            },
            |choices| crate::acquire::Choices::Given(choices.to_vec()),
        );
        let selection = match crate::acquire::resolve_choices(root, game_id, &zip, answers)? {
            crate::acquire::ResolvedChoices::Chosen(Some(choices)) => {
                Some(fomod_allowed(&archive::Archive::open(&zip)?, &choices)?)
            }

            crate::acquire::ResolvedChoices::Chosen(None)
            | crate::acquire::ResolvedChoices::NeedsChoice { .. } => None,
        };
        let base_name = archive_base_name(&file);
        let naming = Naming {
            base_name: &base_name,
            digest: &digest,
            only: Some(folder),
        };

        let _profiles = lock_profiles();
        let mut profiles = load_profiles(data)?;
        undo_deployments(data, &mut profiles)?;
        data.save_profiles(&profiles)?;

        let library_dir = data.library();
        let target = library_dir.join(folder);
        let set_aside = library_dir.join(format!(".reinstall-{folder}"));
        // A set-aside copy without the folder is what an interrupted reinstall left: it is the only copy, so it goes
        // back first. With the folder in place it is a stale duplicate.
        if set_aside.is_dir() && !target.is_dir() {
            std::fs::rename(&set_aside, &target).map_err(io_at(&set_aside))?;
        }

        crate::infra::fs::remove_dir_if_exists(&set_aside).map_err(io_at(&set_aside))?;
        let had_folder = target.is_dir();

        if had_folder {
            std::fs::rename(&target, &set_aside).map_err(io_at(&target))?;
        }

        let settings = crate::context::load_settings(data);
        let roots = crate::context::target::deployment_root(game, &settings)
            .ok()
            .map(|root| crate::import::install::top_level_folders(&root))
            .unwrap_or_default();
        let result = crate::import::install::install_zip_named_with_roots(
            &zip,
            &library_dir,
            game,
            naming,
            selection.as_ref().map(|selection| &selection.allowed),
            selection.as_ref().map(|selection| &selection.virtual_paths),
            &roots,
            &mut |_| {},
        )
        .map_err(AppError::from)
        .and_then(|installed| {
            if let Some(record) = library.archives.get_mut(&digest) {
                record.skipped = installed
                    .skipped_files
                    .iter()
                    .filter(|file| file.reason == crate::domain::library::SkipReason::NoRule)
                    .take(1000)
                    .cloned()
                    .collect();
                record.skipped_total = u32::try_from(
                    installed
                        .skipped_files
                        .iter()
                        .filter(|file| file.reason == crate::domain::library::SkipReason::NoRule)
                        .count(),
                )
                .unwrap_or(u32::MAX);
            }

            library.rescan_version(&library_dir, game, folder)
        });

        if result.is_ok() {
            if let Some(choices) = choices
                && let Some(record) = library.versions.get_mut(folder)
            {
                record.fomod = choices.to_vec();
                record.fomod_installer = true;
            }

            let _ = std::fs::remove_dir_all(&set_aside);
        } else if had_folder {
            let restored = crate::infra::fs::remove_dir_if_exists(&target)
                .and_then(|()| std::fs::rename(&set_aside, &target));
            if let Err(error) = restored {
                // The next reinstall puts the set-aside copy back; until then the log says where it is.
                crate::infra::log::warn(format!(
                    "could not restore {} after a failed reinstall; the previous copy is at {}: {error}",
                    target.display(),
                    set_aside.display()
                ));
            }
        }

        result
    })
}

/// Reads the FOMOD steps of a version's archive copy, for the "change installer options" wizard. A plain archive
/// answers `Plain`.
pub fn reinstall_inspect(root: &Data, game_id: &str, folder: &str) -> Result<ZipInspect, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = load_library(&data, &game)?;
    let (_, zip, _) = archive_for(&data, &library, folder)?;
    crate::import::inspect(root, game_id, &zip)
}

/// The wizard's answer for the installer of a version's archive copy under a selection, for the "change installer
/// options" wizard. `None` is the default selection.
pub fn reinstall_fomod_state(
    root: &Data,
    game_id: &str,
    folder: &str,
    selection: Option<&[crate::import::fomod::Choice]>,
) -> Result<crate::import::fomod::FomodState, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = load_library(&data, &game)?;
    let (_, zip, _) = archive_for(&data, &library, folder)?;
    crate::import::fomod_state(root, game_id, &zip, selection)
}

/// The index record and archive copy of one version: the record for its installer choices, the zip for the new unpack.
fn archive_for(
    data: &Data,
    library: &Library,
    folder: &str,
) -> Result<
    (
        crate::domain::library::ModVersion,
        std::path::PathBuf,
        String,
    ),
    AppError,
> {
    let record = library.version(folder)?.clone();
    let digest = record
        .archive
        .clone()
        .ok_or_else(|| archive_error(folder, "library.no_archive"))?;
    let file = library
        .archive_file_of(&digest)
        .ok_or_else(|| archive_error(folder, "library.archive_missing"))?;
    let zip = data.archives().join(&file);

    if !zip.is_file() {
        return Err(archive_error(folder, "library.archive_missing"));
    }

    Ok((record, zip, file))
}

/// A missing archive or record: a user action, since the file can be restored or the mod imported again.
fn archive_error(folder: &str, code: &str) -> AppError {
    AppError::new(code)
        .with("folder", folder)
        .kind(ErrorKind::UserAction)
}
