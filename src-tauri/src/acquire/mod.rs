//! Getting mod archives into the library: the single path that a link download, a collection member, an update, a
//! requirement, a picked file and a pack all take. Each feature decides what happens around it (the profile, a
//! curator's patch, the version to select); the steps that make a file a library mod run here once, so the same file
//! ends in the same library state whichever feature brought it.
//!
//! The steps, in order: the library is asked whether it already has the exact provider file (then nothing is fetched or
//! imported again); the file is fetched when the caller has only its source; an installer archive gets its FOMOD
//! choices (given by the user, kept by a recipe, or asked for); the archive is imported; a fetched or downloaded file
//! that turned out to be a duplicate, or whose work was stopped, leaves the downloads folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::context::{Data, load_library_shared};
use crate::domain::fomod::Choice;
use crate::domain::library::LibraryView;
use crate::domain::source::SourceRef;
use crate::error::AppError;
use crate::import::model::{FomodStepView, ImportStage, ImportView, ZipInspect};
use crate::import::{ALREADY_IN_LIBRARY, ImportRequest, import_batch};
use crate::infra::sidecar::delete_download;
use crate::infra::tasks::cancelled;
use crate::providers::fetch::{Fetch, fetch};

/// The stable error code of an installer archive that was imported without choices where none can be asked for.
pub const NEEDS_CHOICE: &str = "import.needs_choice";

/// Where the archive comes from.
pub enum Origin<'a> {
    /// A file the user picked or a feature staged. It is never deleted here.
    File(&'a Path),
    /// A file in the game's downloads folder: a duplicate or a stopped import removes it.
    Download(&'a Path),
    /// A provider file that is fetched first. `archive_name` helps a provider that picks its file by name.
    Fetch { archive_name: Option<&'a str> },
}

/// Which FOMOD choices an installer archive is imported with. A plain archive ignores them.
pub enum Choices {
    /// The user answered the wizard for this archive.
    Given(Vec<Choice>),
    /// A recipe or an earlier version kept these. An empty list means nothing was kept, so the user is asked.
    Stored(Vec<Choice>),
    /// Nothing is known: an installer archive comes back as `Acquired::NeedsChoice`.
    Ask,
}

/// One archive to bring into the library.
pub struct Request<'a> {
    pub origin: Origin<'a>,
    /// The provider file it is. Required for `Origin::Fetch`; for a file it wins over the sidecar.
    pub source: Option<SourceRef>,
    pub choices: Choices,
}

/// One archive of a batch passed to [`acquire_batch`]. It is a local file or a provider file to fetch, with the same
/// source and choices as a single [`Request`]. A staged archive carries the digest its caller already computed.
pub struct BatchRequest<'a> {
    pub origin: Origin<'a>,
    /// The provider file it is. Required for `Origin::Fetch`; for a file it wins over the sidecar.
    pub source: Option<SourceRef>,
    pub choices: Choices,
    /// The SHA-256 of a temporary copy the caller staged and hashed, such as an archive taken out of a pack. The
    /// import moves that copy instead of hashing it a second time.
    pub staged: Option<String>,
}

/// What happened to the archive.
pub enum Acquired {
    /// The archive is in the library now.
    Imported(ImportView),
    /// The library already had this exact provider file or archive, in `folder`. Nothing was fetched or imported again.
    Existing { folder: String },
    /// The archive has a FOMOD installer and no choices: the window shows the wizard for the file at `path`.
    NeedsChoice {
        path: PathBuf,
        module_name: String,
        steps: Vec<FomodStepView>,
    },
}

/// The FOMOD answers a caller should import a local archive with.
pub enum ResolvedChoices {
    /// Import with these answers. `None` means a plain archive, or a caller that cannot show a wizard.
    Chosen(Option<Vec<Choice>>),
    /// The installer needs answers the caller did not give; the window shows the wizard for `path`.
    NeedsChoice {
        path: PathBuf,
        module_name: String,
        steps: Vec<FomodStepView>,
    },
}

/// Which part of the work a progress percentage belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Downloading,
    Importing(ImportStage),
}

/// The library folder that already holds this exact provider file, when one does. Only a file id identifies a file: a
/// page can carry several parts and versions. Features that want to skip a download before they start one ask this.
pub fn existing_file(view: &LibraryView<'_>, source: &SourceRef) -> Option<String> {
    let file_id = source.file_id.as_deref()?;
    view.version_of_file(&source.provider, &source.mod_id, file_id)
        .map(|version| version.folder.clone())
}

/// Resolves the FOMOD answers for the local archive at `path`. `Given` answers are used as they are; `Stored` answers
/// are used when the archive kept them and otherwise ask, because a recipe's answers may not fit a different archive;
/// `Ask` always asks. A plain archive needs no answer and comes back as `Chosen(None)`. A reinstall, which cannot show
/// a wizard, treats `NeedsChoice` as "unpack the whole archive".
pub fn resolve_choices(
    root: &Data,
    game_id: &str,
    path: &Path,
    choices: Choices,
) -> Result<ResolvedChoices, AppError> {
    let (given, kept) = match choices {
        Choices::Given(choices) => (Some(choices), Vec::new()),
        Choices::Stored(choices) => (None, choices),
        Choices::Ask => (None, Vec::new()),
    };
    let selected = match given {
        Some(choices) => Some(choices),
        None => match crate::import::inspect(root, game_id, path)? {
            ZipInspect::Plain => None,
            ZipInspect::Fomod { .. } if !kept.is_empty() => Some(kept),
            ZipInspect::Fomod { module_name, steps } => {
                return Ok(ResolvedChoices::NeedsChoice {
                    path: path.to_path_buf(),
                    module_name,
                    steps,
                });
            }
        },
    };
    Ok(ResolvedChoices::Chosen(selected))
}

/// Brings one archive into the library. See the module documentation for the steps. A single acquire is a batch of
/// one, so the duplicate, installer and cancellation rules are the same whichever path a feature takes.
pub fn acquire(
    root: &Data,
    game_id: &str,
    request: Request<'_>,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(Phase, u32),
) -> Result<Acquired, AppError> {
    let Request {
        origin,
        source,
        choices,
    } = request;
    let mut results = acquire_batch(
        root,
        game_id,
        vec![BatchRequest {
            origin,
            source,
            choices,
            staged: None,
        }],
        cancel,
        progress,
    )?;
    results
        .pop()
        .unwrap_or_else(|| Err(AppError::unexpected("an acquire produced no result")))
}

/// Brings a batch of archives into the library through the same rules as [`acquire`], with one import batch underneath.
/// Returns one outcome per request, in order; one archive's failure stays in its own slot while the rest import. A flag
/// stops the work between groups, and the groups already committed stay.
pub fn acquire_batch(
    root: &Data,
    game_id: &str,
    requests: Vec<BatchRequest<'_>>,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(Phase, u32),
) -> Result<Vec<Result<Acquired, AppError>>, AppError> {
    let game = crate::games::load(game_id)?;
    let library = load_library_shared(&root.game(game_id), &game)?;
    let view = LibraryView::new(&library);
    let mut slots: Vec<Option<Result<Acquired, AppError>>> = Vec::with_capacity(requests.len());
    let mut pending: Vec<(usize, ImportRequest, bool, PathBuf, bool)> = Vec::new();

    for request in requests {
        let BatchRequest {
            origin,
            source,
            choices,
            staged,
        } = request;
        let stored = matches!(choices, Choices::Stored(_));

        if let Some(source) = &source
            && let Some(folder) = existing_file(&view, source)
        {
            if let Origin::Download(path) = origin {
                let _ = delete_download(path);
            }

            slots.push(Some(Ok(Acquired::Existing { folder })));
            continue;
        }

        let (path, source, removable) = match origin {
            Origin::File(path) => (path.to_path_buf(), source, false),
            Origin::Download(path) => (path.to_path_buf(), source, true),
            Origin::Fetch { archive_name } => {
                let source =
                    source.ok_or_else(|| AppError::unexpected("a fetch without a source"))?;
                let downloaded = fetch(
                    Fetch {
                        game_key: game.provider_key(source.provider.as_str()),
                        source,
                        directory: &root.game(game_id).downloads(),
                        archive_name,
                        download_key: None,
                    },
                    cancel,
                    &mut |percent| progress(Phase::Downloading, percent),
                )?;
                (downloaded.path, Some(downloaded.source), true)
            }
        };
        let chosen = match resolve_choices(root, game_id, &path, choices) {
            Ok(ResolvedChoices::Chosen(chosen)) => chosen,
            Ok(ResolvedChoices::NeedsChoice {
                path,
                module_name,
                steps,
            }) => {
                slots.push(Some(Ok(Acquired::NeedsChoice {
                    path,
                    module_name,
                    steps,
                })));
                continue;
            }
            Err(error) => {
                slots.push(Some(Err(error)));
                continue;
            }
        };
        let mut import = ImportRequest::new(&path)
            .with_choices(chosen)
            .with_source(source);
        if let Some(digest) = staged {
            import = import.staged(digest);
        }

        pending.push((slots.len(), import, removable, path, stored));
        slots.push(None);
    }

    if !pending.is_empty() {
        let requests: Vec<ImportRequest> = pending
            .iter()
            .map(|(_, request, ..)| request.clone())
            .collect();
        let results = match import_batch(
            root,
            game_id,
            requests,
            &mut |stage, percent| progress(Phase::Importing(stage), percent),
            cancel,
        ) {
            Ok(results) => results,
            Err(error) => {
                if cancelled(cancel) {
                    for (_, _, removable, path, _) in &pending {
                        if *removable {
                            let _ = delete_download(path);
                        }
                    }
                }

                return Err(error);
            }
        };

        for (result, (slot, _, removable, path, stored)) in results.into_iter().zip(pending) {
            let acquired = match result {
                Ok(view) => Ok(Acquired::Imported(view)),
                Err(error) if error.code == ALREADY_IN_LIBRARY => Ok(Acquired::Existing {
                    folder: error.params.get("folder").cloned().unwrap_or_default(),
                }),
                // Choices a recipe kept may not fit this archive: the user answers the wizard instead.
                Err(error) if stored && error.code.starts_with("fomod.") => {
                    match crate::import::inspect(root, game_id, &path) {
                        Ok(ZipInspect::Fomod { module_name, steps }) => Ok(Acquired::NeedsChoice {
                            path: path.clone(),
                            module_name,
                            steps,
                        }),
                        _ => Err(error),
                    }
                }
                Err(error) => Err(error),
            };
            // One cleanup rule for everything that came from the downloads folder: a duplicate or a stopped import
            // leaves; an archive that still needs wizard answers stays for the window.
            let duplicate = matches!(acquired, Ok(Acquired::Existing { .. }));
            let stopped = acquired.as_ref().is_err_and(|_| cancelled(cancel));

            if removable && (duplicate || stopped) {
                let _ = delete_download(&path);
            }

            slots[slot] = Some(acquired);
        }
    }

    Ok(slots
        .into_iter()
        .map(|slot| {
            slot.unwrap_or_else(|| Err(AppError::unexpected("an acquire produced no result")))
        })
        .collect())
}

/// The import of an archive that needed choices where no wizard can be shown, as an error the window acts on: it
/// opens the wizard for the file at `path`.
pub fn needs_choice_error(path: &Path) -> AppError {
    AppError::user(NEEDS_CHOICE).with("path", path.display())
}
