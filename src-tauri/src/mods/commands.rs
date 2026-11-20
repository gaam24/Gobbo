//! Commands on the mod list: the status of a game, mod details, importing archives and the per-mod edits.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::acquire;
use crate::commands::run_task;
use crate::context::Data;
use crate::domain::source::{SourceRef, SourceRequirement};
use crate::error::AppError;
use crate::import::fomod;
use crate::import::{ArchiveReport, ImportProgress, ImportView, ZipInspect};
use crate::infra::tasks::{TaskFlags, TaskKey, kind};
use crate::library::{ModInfoEdit, RelationKind, Relations};
use crate::sources::downloads::Downloads;
use crate::status::details;
use crate::status::views::{ModDetails, ModExtras, ModUsage, Status, VersionView};

/// The archive a FOMOD wizard reads. An import reads a loose archive; a reinstall reads the archive copy of an installed
/// version. The window passes this identity with a click so the backend can apply the group rules and answer with a
/// fresh view.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FomodSource {
    /// A loose archive being imported.
    Path { path: String },
    /// The archive copy of an installed version.
    Folder { folder: String },
}

/// The wizard's answer for one archive under a selection. `None` is the default selection.
fn fomod_answer(
    root: &Data,
    game: &str,
    source: &FomodSource,
    selection: Option<&[fomod::Choice]>,
) -> Result<fomod::FomodState, AppError> {
    match source {
        FomodSource::Path { path } => {
            crate::import::fomod_state(root, game, Path::new(path), selection)
        }
        FomodSource::Folder { folder } => {
            crate::import::reinstall_fomod_state(root, game, folder, selection)
        }
    }
}

crate::commands::game_commands! {
    query get_status() -> Status = crate::status::status;

    /// Brings every profile in step with the library and answers the fresh status. A change, not a query: it saves
    /// the profiles when the library gained or lost a mod.
    change sync_profiles() = crate::status::sync_profiles;

    /// What the game needs before the first apply: folders, loader, overlay support and free space.
    query preflight() -> crate::status::preflight::PreflightReport =
        crate::status::preflight::preflight;

    /// One list of the things that block an apply and the warnings that do not, with the next action for each.
    query health_check() -> crate::status::health::HealthReport =
        crate::status::health::health;

    query mod_details(folder: String) -> ModDetails = |root, game| details::mod_details(root, game, &folder);

    /// The page text, notes and relation details of the rows in the table's viewport. The list row keeps only what it
    /// draws, so the optional columns and the inspector ask for the rest here.
    query mod_extras(folders: Vec<String>) -> Vec<ModExtras> = |root, game| details::mod_extras(root, game, &folders);

    /// Every installed version of a mod family, for the table's version picker.
    query mod_versions(folder: String) -> Vec<VersionView> = |root, game| details::mod_versions(root, game, &folder);

    /// Preview of an archive: a plain zip, or a FOMOD installer with the steps for the wizard.
    query inspect_zip(path: String) -> ZipInspect = |root, game| crate::import::inspect(root, game, Path::new(&path));

    /// The FOMOD wizard's answer for an archive's installer under the user's picks: the visible steps, the group rules,
    /// the option kinds and the picks the import takes. `null` gives the default selection. Read like `inspect_zip`.
    query fomod_state(path: String, selection: Option<Vec<fomod::Choice>>) -> fomod::FomodState =
        |root, game| crate::import::fomod_state(root, game, Path::new(&path), selection.as_deref());

    /// The same answer for a version's archive copy, for the "change installer options" wizard.
    query reinstall_fomod_state(folder: String, selection: Option<Vec<fomod::Choice>>) -> fomod::FomodState =
        |root, game| crate::import::reinstall_fomod_state(root, game, &folder, selection.as_deref());

    /// The wizard's answer after one option click. The backend applies the group rules to the current picks and answers
    /// with a fresh view; the window only renders it. Read-only: nothing is saved until the wizard confirms the install.
    query fomod_toggle_pick(
        source: FomodSource,
        selection: Vec<fomod::Choice>,
        step: u32,
        group: u32,
        option: String,
    ) -> fomod::FomodState = |root, game| {
        let view = fomod_answer(root, game, &source, Some(&selection))?;
        let next = view.toggle_pick(step, group, &option);
        fomod_answer(root, game, &source, Some(&next))
    };

    /// The wizard's answer when a reinstall starts from saved choices: the saved answers laid over the installer
    /// defaults, with the same validation a click applies. Read-only until the wizard saves.
    query fomod_selection_from_choices(source: FomodSource, choices: Vec<fomod::Choice>) -> fomod::FomodState =
        |root, game| {
            let defaults = fomod_answer(root, game, &source, None)?;
            let next = defaults.selection_from_choices(&choices);
            fomod_answer(root, game, &source, Some(&next))
        };

    change rescan_library() = crate::library::rescan;

    change delete_mod(id: String) = |root, game| crate::deploy::remove::delete_mod(root, game, &id);

    /// Removes every part of the source page the family belongs to.
    change delete_mod_page(id: String) = |root, game| crate::deploy::remove::delete_page(root, game, &id);

    /// Profiles that use a mod family, with the selected version and whether a prepared deployment contains it.
    query mod_usage(id: String) -> ModUsage = |root, game| details::mod_usage(root, game, &id);

    /// Switches the version of one mod in the active profile.
    change set_mod_version(id: String, folder: String) =
        |root, game| crate::profiles::actions::set_mod_version(root, game, &id, &folder);

    /// Removes one installed version. `replacement` takes over in every profile that used it; without one a used
    /// version must be the last of its family, which removes the whole mod.
    change delete_mod_version(folder: String, replacement: Option<String>) =
        |root, game| crate::deploy::remove::delete_version(root, game, &folder, replacement.as_deref());

    /// Unpacks one mod again from its archive copy. `choices` are new FOMOD answers; `None` keeps the saved ones. The
    /// index keeps the user's data for it.
    change reinstall_mod(folder: String, choices: Option<Vec<fomod::Choice>>) =
        |root, game| crate::deploy::remove::reinstall_mod(root, game, &folder, choices.as_deref());

    /// Reads the FOMOD steps of a version's archive copy, for the "change installer options" wizard.
    query reinstall_inspect(folder: String) -> ZipInspect =
        |root, game| crate::import::reinstall_inspect(root, game, &folder);

    /// Compares the installed folder with its archive copy and lists what the import left out.
    query archive_report(folder: String) -> ArchiveReport =
        |root, game| crate::import::archive_report(root, game, &folder);

    /// Checks one installed version against the checksum recorded at import. The first check of an older version
    /// records the current files as the baseline.
    query verify_integrity(folder: String) -> crate::library::integrity::IntegrityStatus =
        |root, game| crate::library::integrity::verify(root, game, &folder);

    /// Removes every version that no profile selects.
    change delete_unused_versions() = crate::deploy::remove::delete_unused_versions;

    /// Saves the mod's tags (given by the user, for filtering the list).
    change set_mod_tags(id: String, tags: Vec<String>) =
        |root, game| crate::library::set_mod_tags(root, game, &id, tags);

    /// Saves the user's note about the mod.
    change set_mod_note(id: String, note: String) = |root, game| crate::library::set_mod_note(root, game, &id, note);

    /// Saves the name, version and author typed for one version of a mod. An empty field goes back to `info.json`.
    change set_mod_info(id: String, folder: String, info: ModInfoEdit) =
        |root, game| crate::library::set_mod_info(root, game, &id, &folder, info);

    /// Saves the instructions for players of a mod. An empty text removes them.
    change set_mod_instructions(id: String, text: String) =
        |root, game| crate::library::set_mod_instructions(root, game, &id, &text);

    /// Saves the mods a mod requires, the mods it does not work with, and the mods it overrides.
    change set_mod_relations(id: String, relations: Relations) =
        |root, game| crate::library::set_mod_relations(root, game, &id, &relations);

    /// Adds one mod to a relation list of a mod. A relation already listed is left as it is.
    change add_relation(id: String, kind: RelationKind, other: String) =
        |root, game| crate::library::add_relation(root, game, &id, kind, &other);

    /// Removes one mod from a relation list of a mod.
    change remove_relation(id: String, kind: RelationKind, other: String) =
        |root, game| crate::library::remove_relation(root, game, &id, kind, &other);

    /// Adds one requirement to a mod. A mod already listed under the same provider and id is left as it is.
    change add_requirement(id: String, requirement: SourceRequirement) =
        |root, game| crate::library::add_requirement(root, game, &id, &requirement);

    /// Removes the requirement for one provider mod from a mod.
    change remove_requirement(id: String, provider: String, req_mod_id: String) =
        |root, game| crate::library::remove_requirement(root, game, &id, &provider, &req_mod_id);

    /// Sets whether the requirement for one provider mod is ignored.
    change set_requirement_ignored(id: String, provider: String, req_mod_id: String, ignored: bool) =
        |root, game| crate::library::set_requirement_ignored(root, game, &id, &provider, &req_mod_id, ignored);

    /// Saves the load priority the user set for a mod, or clears it with `null`.
    change set_mod_priority(id: String, priority: Option<u32>) =
        |root, game| crate::profiles::actions::set_mod_priority(root, game, &id, priority);

    /// Saves the steps a mod asks for. Each step must be one the game defines for that phase.
    change set_mod_steps(folder: String, before_start: Vec<String>, after_end: Vec<String>) =
        |root, game| crate::mods::steps::set_mod_steps(root, game, &folder, &before_start, &after_end);
}

/// Import runs in the background, so the window does not freeze. Progress goes through a channel to the window.
/// `choices` are the choices from the FOMOD wizard; `None` is a normal, full import, while an empty list is an
/// installer with nothing to pick (its required options still decide the files). `source` names the provider file a
/// download came from; without it the sidecar or the archive name may name one. A download of the queue shows its
/// import there too.
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::too_many_arguments,
    reason = "a Tauri command takes each IPC argument and state as its own parameter"
)]
pub async fn import_zip(
    app: AppHandle,
    data: State<'_, Data>,
    downloads: State<'_, Downloads>,
    tasks: State<'_, TaskFlags>,
    game: String,
    path: String,
    choices: Option<Vec<fomod::Choice>>,
    source: Option<SourceRef>,
    progress: Channel<ImportProgress>,
) -> Result<ImportView, AppError> {
    let key = TaskKey::import(&game, &path);
    // The helper borrows `path` for the queue bookkeeping; the work owns a clone of it.
    let work_path = path.clone();
    crate::sources::downloads::import_with_queue(
        &app,
        &downloads,
        &path,
        |from_queue| {
            run_task(
                data,
                &tasks,
                key.as_str().to_string(),
                game,
                kind::IMPORT,
                move |root, game, flag, handle| {
                    let report = &mut |stage, percent| {
                        let _ = progress.send(ImportProgress { stage, percent });
                        handle.progress(percent);
                    };
                    // A picked file and a queue download take the shared `acquire` path, like a link and a collection member.
                    let path = Path::new(&work_path);
                    let request = acquire::Request {
                        origin: if from_queue {
                            acquire::Origin::Download(path)
                        } else {
                            acquire::Origin::File(path)
                        },
                        source,
                        choices: choices.map_or(acquire::Choices::Ask, acquire::Choices::Given),
                    };
                    let acquired = acquire::acquire(
                        root,
                        game,
                        request,
                        Some(flag),
                        &mut |phase, percent| {
                            if let acquire::Phase::Importing(stage) = phase {
                                report(stage, percent);
                            }
                        },
                    )?;

                    match acquired {
                        acquire::Acquired::Imported(view) => Ok(view),
                        acquire::Acquired::Existing { folder } => {
                            Err(crate::import::already_in_library(&folder))
                        }
                        acquire::Acquired::NeedsChoice { path, .. } => {
                            Err(acquire::needs_choice_error(&path))
                        }
                    }
                },
            )
        },
        |result| {
            // A duplicate of a library file has left the downloads folder: its queue entry is done, not waiting for a retry.
            match result {
                Err(error) if error.code == crate::import::ALREADY_IN_LIBRARY => Ok(()),
                other => other.as_ref().map(|_| ()),
            }
        },
    )
    .await
}

/// Stops a loose import at the next chunk or file. A stopped import commits nothing.
#[tauri::command]
#[specta::specta]
pub fn cancel_import(tasks: State<'_, TaskFlags>, game: String, path: String) {
    tasks.cancel(TaskKey::import(&game, &path).as_str());
}
