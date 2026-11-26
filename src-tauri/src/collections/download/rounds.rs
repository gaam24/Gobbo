use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::import::{import_member, install_one};
use super::loaded::{ItemOutcome, LoadedCollection, load};
use super::prefetch::{may_download_in_parallel, prefetch_archives, provider_needs_page};
use super::provider_source;
use crate::acquire;
use crate::collections::model::{
    CollectionFailure, CollectionInstallView, CollectionItemAction, CollectionItemState,
    CollectionProgress, CollectionSessionView,
};
use crate::collections::session;
use crate::context::Data;
use crate::domain::collection::{CollectionItem, CollectionItemStatus, CollectionSource};
use crate::domain::library::LibraryView;
use crate::error::{AppError, Message};
use crate::infra::tasks::cancelled;
use crate::providers::{self, PAYMENT_REQUIRED};
use crate::sources::downloads::Downloads;

/// Installs every selected member that can be installed automatically. A FOMOD without stored choices is returned for
/// the window's wizard; a browse or manual member waits for the user. One member failing does not stop the others.
pub fn install_automatic(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    collection_key: &str,
    progress: &mut dyn FnMut(CollectionProgress),
    cancel: Option<&AtomicBool>,
) -> Result<CollectionInstallView, AppError> {
    let loaded = load(root, game_id, profile_id, collection_key)?;
    // One view for the whole operation: the member statuses and the exact-file checks share its indexes.
    let view = LibraryView::new(&loaded.library);
    let statuses = loaded.statuses(&view);
    let selected: Vec<&CollectionItem> = loaded
        .plan
        .ordered()
        .into_iter()
        .filter(|item| !loaded.origin.excluded.iter().any(|key| key == &item.key))
        .collect();
    let total = u32::try_from(selected.len()).unwrap_or(u32::MAX);
    let mut installed = Vec::new();
    let mut failed = Vec::new();
    let mut needs_choice = Vec::new();
    let mut needs_user = Vec::new();
    let mut skipped = Vec::new();
    let mut warnings = Vec::new();
    let mut manual = Vec::new();
    let mut cancelled_run = false;
    let mut prefetched = if may_download_in_parallel(&loaded) {
        // The download phase runs off-thread and cannot use the window's progress callback. Announce the members it
        // covers so the list shows them as downloading, then report the real result when each one is imported.
        for (index, item) in selected.iter().enumerate() {
            if matches!(item.source, CollectionSource::Provider { .. })
                && !statuses
                    .statuses
                    .get(&item.key)
                    .is_some_and(CollectionItemStatus::is_done)
            {
                progress(CollectionProgress {
                    key: item.key.clone(),
                    name: item.name.clone(),
                    index: u32::try_from(index + 1).unwrap_or(u32::MAX),
                    total,
                    percent: 0,
                    state: CollectionItemState::Downloading,
                    error: None,
                });
            }
        }

        prefetch_archives(&loaded, &selected, &statuses, &view, cancel)
    } else {
        (0..selected.len()).map(|_| None).collect()
    };

    for (index, item) in selected.iter().enumerate() {
        if cancelled(cancel) {
            cancelled_run = true;
            break;
        }

        let item_status = statuses
            .statuses
            .get(&item.key)
            .cloned()
            .unwrap_or(CollectionItemStatus::Missing);
        if item_status.is_done() {
            continue;
        }

        if item_status == CollectionItemStatus::Unsupported {
            progress(CollectionProgress {
                key: item.key.clone(),
                name: item.name.clone(),
                index: u32::try_from(index + 1).unwrap_or(u32::MAX),
                total,
                percent: 0,
                state: CollectionItemState::Skipped,
                error: None,
            });
            skipped.push(item.key.clone());
            continue;
        }

        if matches!(
            item.source,
            CollectionSource::Browse { .. } | CollectionSource::Manual
        ) {
            needs_user.push(item.key.clone());
            progress(CollectionProgress {
                key: item.key.clone(),
                name: item.name.clone(),
                index: u32::try_from(index + 1).unwrap_or(u32::MAX),
                total,
                percent: 0,
                state: CollectionItemState::NeedsUser,
                error: None,
            });
            continue;
        }

        let mut report = |state, percent, error| {
            progress(CollectionProgress {
                key: item.key.clone(),
                name: item.name.clone(),
                index: u32::try_from(index + 1).unwrap_or(u32::MAX),
                total,
                percent,
                state,
                error,
            });
        };

        if provider_needs_page(item) {
            // A free account cannot fetch this file: leave it pending so the controller can open its page and wait for
            // the owned download. It is not a manual pick and not a failure of the round.
            report(CollectionItemState::NeedsUser, 0, None);
            needs_user.push(item.key.clone());
            continue;
        }

        match install_one(
            &loaded,
            root,
            game_id,
            item,
            prefetched[index].take(),
            &mut |state, percent, error| report(state, percent, error),
            cancel,
        ) {
            Ok(ItemOutcome::Installed {
                manual: item_manual,
            }) => {
                report(CollectionItemState::Installed, 100, None);
                installed.push(item.key.clone());

                if item_manual {
                    manual.push(item.key.clone());
                }
            }
            Ok(ItemOutcome::NeedsChoice(choice)) => {
                needs_choice.push(*choice);
            }
            // The library already holds this exact file: nothing was downloaded. The finish step lists it.
            Ok(ItemOutcome::Existing) => {
                warnings
                    .push(Message::new("collection.already_in_library").with("name", &item.name));
                report(CollectionItemState::Skipped, 100, None);
            }
            Ok(ItemOutcome::NeedsUser) => {
                needs_user.push(item.key.clone());
            }
            Ok(ItemOutcome::Unsupported) => {
                skipped.push(item.key.clone());
            }
            Err(error) if error.is_cancelled() => {
                cancelled_run = true;
                break;
            }
            Err(error) if error.code == PAYMENT_REQUIRED => {
                // The account answered that it cannot download directly. The member waits for the provider's page; it
                // is not a failure and does not stop the other members of the round.
                report(CollectionItemState::NeedsUser, 0, None);
                needs_user.push(item.key.clone());
            }
            Err(error) => {
                report(CollectionItemState::Failed, 0, Some(error.clone()));
                failed.push(CollectionFailure {
                    key: item.key.clone(),
                    name: item.name.clone(),
                    error,
                });
            }
        }
    }

    let session = session::finish_with_manual(root, game_id, profile_id, collection_key, &manual)?;
    Ok(CollectionInstallView {
        profile: profile_id.to_string(),
        installed,
        failed,
        needs_choice,
        needs_user,
        skipped,
        warnings,
        cancelled: cancelled_run,
        session,
    })
}
/// The guided flow: opens the exact file page for a member whose provider needs an account and marks the download it
/// will produce. Members of a provider without an account, bundles and direct URLs are installed at once because they
/// need no browser.
pub fn prepare_item(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    collection_key: &str,
    item_key: &str,
    downloads: &Downloads,
) -> Result<CollectionItemAction, AppError> {
    let loaded = load(root, game_id, profile_id, collection_key)?;
    let item = loaded
        .plan
        .item(item_key)
        .ok_or_else(|| AppError::user("collection.item_missing").with("item", item_key))?;
    let view = LibraryView::new(&loaded.library);
    let statuses = loaded.statuses(&view);
    let status = statuses.statuses.get(item_key);

    if status.is_some_and(CollectionItemStatus::is_done) {
        return Ok(CollectionItemAction::Installed);
    }

    if status == Some(&CollectionItemStatus::Unsupported) {
        return Ok(CollectionItemAction::Unsupported {
            reason: Message::new("collection.unsupported_source").with("name", &item.name),
        });
    }
    // The exact file is already in the library. Opening its page would ask the user to download a copy the import
    // could only refuse; let the window say so and leave the member for the user to enable from the mod list.
    if matches!(item.source, CollectionSource::Provider { .. })
        && provider_source(item)
            .is_some_and(|source| LoadedCollection::has_library_file(&view, &source))
    {
        return Ok(CollectionItemAction::AlreadyInLibrary {
            name: item.name.clone(),
        });
    }

    match &item.source {
        CollectionSource::Provider {
            provider: provider_id,
            mod_id,
            file_id,
            ..
        } if providers::require(provider_id.as_str())?
            .credential_account()
            .is_some() =>
        {
            let source = provider_source(item)
                .ok_or_else(|| AppError::user("collection.item_missing").with("item", &item.key))?;
            let game_key = loaded
                .game
                .provider_key(provider_id.as_str())
                .ok_or_else(|| providers::no_game_key(provider_id.as_str()))?;
            let provider = providers::require(provider_id.as_str())?;
            let url = provider
                .file_page_url(game_key, mod_id, file_id)
                .ok_or_else(|| {
                    AppError::user("collection.file_page_missing").with("name", &item.name)
                })?;
            let owner = format!("collection:{profile_id}:{item_key}");
            let _ = downloads.adopt(game_id, &source, &owner);
            downloads.expect_file(game_id, source, owner.clone());
            Ok(CollectionItemAction::Waiting { owner, url })
        }
        // A provider that needs no account (Thunderstore) downloads at once, like a bundle or a direct URL: there is
        // no page to visit and nothing the user has to click.
        CollectionSource::Provider { .. }
        | CollectionSource::Bundle { .. }
        | CollectionSource::Direct { .. } => {
            let outcome =
                match install_one(&loaded, root, game_id, item, None, &mut |_, _, _| {}, None) {
                    Ok(outcome) => outcome,
                    Err(error) => return Ok(CollectionItemAction::Failed { error }),
                };
            let manual = match &outcome {
                ItemOutcome::Installed { manual } => (*manual)
                    .then(|| item_key.to_string())
                    .into_iter()
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            let _ =
                session::finish_with_manual(root, game_id, profile_id, collection_key, &manual)?;
            match outcome {
                ItemOutcome::Installed { .. } => Ok(CollectionItemAction::Installed),
                ItemOutcome::Existing => Ok(CollectionItemAction::AlreadyInLibrary {
                    name: item.name.clone(),
                }),
                ItemOutcome::NeedsChoice(choice) => Ok(CollectionItemAction::NeedsChoice {
                    path: choice.choice.path.display().to_string(),
                    module_name: choice.choice.module_name,
                    steps: choice.choice.steps,
                }),
                ItemOutcome::NeedsUser => Ok(CollectionItemAction::NeedsUser {
                    url: None,
                    instructions: item.instructions.clone(),
                }),
                ItemOutcome::Unsupported => Ok(CollectionItemAction::Unsupported {
                    reason: Message::new("collection.unsupported_source").with("name", &item.name),
                }),
            }
        }
        CollectionSource::Browse { url } => Ok(CollectionItemAction::NeedsUser {
            url: Some(url.clone()),
            instructions: item.instructions.clone(),
        }),
        CollectionSource::Manual => Ok(CollectionItemAction::NeedsUser {
            url: None,
            instructions: item.instructions.clone(),
        }),
        CollectionSource::Unsupported { .. } => Ok(CollectionItemAction::Unsupported {
            reason: Message::new("collection.unsupported_source").with("name", &item.name),
        }),
    }
}
/// Imports a file the user downloaded by hand for one member, then closes the member. `choices` overrides the stored
/// FOMOD choices; the window passes them after its wizard.
pub fn import_file(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    collection_key: &str,
    item_key: &str,
    path: &Path,
    choices: Option<Vec<crate::domain::fomod::Choice>>,
) -> Result<CollectionSessionView, AppError> {
    let loaded = load(root, game_id, profile_id, collection_key)?;
    let item = loaded
        .plan
        .item(item_key)
        .ok_or_else(|| AppError::user("collection.item_missing").with("item", item_key))?;
    if !crate::infra::archive::is_archive(
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default(),
    ) {
        return Err(AppError::user("collection.bad_archive").with("path", path.display()));
    }
    // The download a provider wrote for this exact file knows more than the plan (the part's name and category): it
    // wins, so a member imported here gets the same family as the same file downloaded through a link.
    let source = provider_source(item).map(|planned| {
        crate::infra::sidecar::read_source(path)
            .filter(|downloaded| downloaded.same_file(&planned))
            .unwrap_or(planned)
    });
    // A file in the game's downloads folder is the queue's; a duplicate of a library file leaves it there.
    let origin = if path.starts_with(loaded.data.downloads()) {
        acquire::Origin::Download(path)
    } else {
        acquire::Origin::File(path)
    };
    let outcome = import_member(
        &loaded,
        root,
        game_id,
        origin,
        source,
        item,
        choices,
        &mut |_, _, _| {},
        None,
    )?;

    if let ItemOutcome::NeedsChoice(_) = outcome {
        return Err(AppError::user("collection.needs_choice").with("name", &item.name));
    }

    let manual = match item.source {
        CollectionSource::Provider { .. } => Vec::new(),
        _ => vec![item_key.to_string()],
    };
    session::finish_with_manual(root, game_id, profile_id, collection_key, &manual)
}
