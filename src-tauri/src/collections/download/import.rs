use std::sync::atomic::AtomicBool;

use super::loaded::{ItemOutcome, LoadedCollection, PreparedDownload};
use super::source::provider_source;
use crate::acquire;
use crate::collections::model::{CollectionChoiceNeeded, CollectionItemState};
use crate::collections::{bundle, external, patch};
use crate::context::Data;
use crate::domain::collection::{CollectionItem, CollectionSource};
use crate::domain::source::SourceRef;
use crate::error::AppError;
use crate::infra::tasks::cancelled;

pub(super) fn install_one(
    loaded: &LoadedCollection,
    root: &Data,
    game_id: &str,
    item: &CollectionItem,
    prepared: Option<PreparedDownload>,
    progress: &mut dyn FnMut(CollectionItemState, u32, Option<AppError>),
    cancel: Option<&AtomicBool>,
) -> Result<ItemOutcome, AppError> {
    match &item.source {
        CollectionSource::Provider { .. } => match prepared {
            Some(PreparedDownload::Ready(downloaded)) => import_member(
                loaded,
                root,
                game_id,
                acquire::Origin::Download(&downloaded.path),
                Some(downloaded.source),
                item,
                None,
                progress,
                cancel,
            ),
            Some(PreparedDownload::Failed(error)) => Err(error),
            None => import_member(
                loaded,
                root,
                game_id,
                acquire::Origin::Fetch {
                    archive_name: item.file_name.as_deref(),
                },
                provider_source(item),
                item,
                None,
                progress,
                cancel,
            ),
        },
        CollectionSource::Bundle { .. } => {
            let archive = loaded
                .cache_archive
                .as_deref()
                .ok_or_else(|| AppError::user("collection.no_archive"))?;
            progress(CollectionItemState::Importing, 0, None);
            let bundle = bundle::materialize(archive, item, &loaded.cache_dir)?;
            let outcome = import_member(
                loaded,
                root,
                game_id,
                acquire::Origin::File(&bundle),
                None,
                item,
                None,
                progress,
                cancel,
            )?;
            Ok(match outcome {
                ItemOutcome::Installed { .. } => ItemOutcome::Installed { manual: true },
                other => other,
            })
        }
        CollectionSource::Direct { url } => {
            let target = loaded.cache_dir.join("external");
            let downloaded = external::download(
                url,
                &target,
                item.size_kb,
                item.md5.as_deref(),
                &mut |received, total| {
                    if cancelled(cancel) {
                        return false;
                    }
                    progress(
                        CollectionItemState::Downloading,
                        crate::domain::progress::percent(received, total).unwrap_or(0),
                        None,
                    );
                    true
                },
            )?;
            if !downloaded.is_archive {
                return Ok(ItemOutcome::NeedsUser);
            }
            let outcome = import_member(
                loaded,
                root,
                game_id,
                acquire::Origin::File(&downloaded.path),
                None,
                item,
                None,
                progress,
                cancel,
            )?;
            Ok(match outcome {
                ItemOutcome::Installed { .. } => ItemOutcome::Installed { manual: true },
                other => other,
            })
        }
        CollectionSource::Browse { .. } | CollectionSource::Manual => Ok(ItemOutcome::NeedsUser),
        CollectionSource::Unsupported { .. } => Ok(ItemOutcome::Unsupported),
    }
}

/// The one import of a collection member, for the automatic, the guided and the hand-picked file alike. It runs the
/// shared `acquire` path, so a member ends in the library exactly as the same file from a link does: `choices` (the
/// window's) or the curator's stored ones pick the FOMOD options, else the wizard is asked for; a file the library
/// already holds is not fetched again. The member's patches are applied to the new folder.
#[allow(
    clippy::too_many_arguments,
    reason = "a member import has the collection, the archive, its source, the choices, progress and cancellation"
)]
pub(super) fn import_member(
    loaded: &LoadedCollection,
    root: &Data,
    game_id: &str,
    origin: acquire::Origin<'_>,
    source: Option<SourceRef>,
    item: &CollectionItem,
    choices: Option<Vec<crate::domain::fomod::Choice>>,
    progress: &mut dyn FnMut(CollectionItemState, u32, Option<AppError>),
    cancel: Option<&AtomicBool>,
) -> Result<ItemOutcome, AppError> {
    let request = acquire::Request {
        origin,
        source,
        choices: choices.map_or_else(
            || acquire::Choices::Stored(item.choices.clone()),
            acquire::Choices::Given,
        ),
    };
    let acquired = acquire::acquire(root, game_id, request, cancel, &mut |phase, percent| {
        let state = match phase {
            acquire::Phase::Downloading => CollectionItemState::Downloading,
            acquire::Phase::Importing(_) => CollectionItemState::Importing,
        };
        progress(state, percent, None);
    })?;
    let view = match acquired {
        acquire::Acquired::Imported(view) => view,
        acquire::Acquired::Existing { .. } => return Ok(ItemOutcome::Existing),
        acquire::Acquired::NeedsChoice {
            path,
            module_name,
            steps,
        } => {
            progress(CollectionItemState::NeedsChoice, 0, None);
            return Ok(ItemOutcome::NeedsChoice(Box::new(CollectionChoiceNeeded {
                key: item.key.clone(),
                name: item.name.clone(),
                path: path.display().to_string(),
                module_name,
                steps,
            })));
        }
    };
    if !item.patches.is_empty()
        && let Some(folder) = view.folders.first()
        && let Some(archive) = &loaded.cache_archive
    {
        progress(CollectionItemState::Patching, 0, None);
        let mut issues = Vec::new();
        // A patch is an improvement on top of an installed mod: the member stays installed, and the failure is logged.
        if let Err(error) = patch::apply(
            archive,
            &item.name,
            &loaded.data.library().join(folder),
            &item.patches,
            &mut issues,
        ) {
            crate::infra::log::warn(format!(
                "collection patches of {} were not applied: {error}",
                item.name
            ));
        }
        for issue in issues {
            crate::infra::log::warn(format!(
                "collection patch {}: {}",
                issue.message.code, item.name
            ));
        }
    }
    Ok(ItemOutcome::Installed { manual: false })
}
