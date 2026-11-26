//! Driving the backend-owned install rounds and resolving the prompt that pauses one.

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::collections::controller::{CollectionDecision, CollectionRoundView};
use crate::collections::model::CollectionProgress;
use crate::commands::helpers::channel_sink_with_progress;
use crate::context::Data;
use crate::error::AppError;
use crate::infra::tasks::{TaskFlags, TaskKey, kind};
use crate::sources::downloads::{Downloads, publish};

/// Asks the window to open the provider page a waiting round names. The controller already registered the matching
/// download expectation, so the window only has to open the page through its normal source action.
fn emit_page_request(app: &AppHandle, round: &CollectionRoundView) {
    if let Some(page) = &round.waiting {
        let _ = app.emit(super::controller::COLLECTION_PAGE_EVENT, page);
    }
}

/// Drives one full automatic round of a collection install and answers the fresh session together with the single
/// prompt that pauses it. The backend owns the loop; the window calls this once and then only sends a decision.
#[tauri::command]
#[specta::specta]
pub async fn collection_run_round(
    app: AppHandle,
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    profile: String,
    collection: String,
    progress: Channel<CollectionProgress>,
) -> Result<CollectionRoundView, AppError> {
    let key = TaskKey::collection_install(&game, &profile, &collection);
    let work_app = app.clone();
    let result = crate::commands::helpers::run_task(
        data,
        &tasks,
        key,
        game,
        kind::COLLECTION,
        move |root, game, flag, handle| {
            let mut send =
                channel_sink_with_progress(progress, handle, |step: &CollectionProgress| {
                    step.percent
                });
            let downloads = work_app.state::<Downloads>();
            super::controller::run_round(
                root,
                game,
                &profile,
                &collection,
                downloads.inner(),
                &mut send,
                Some(flag),
            )
        },
    )
    .await;

    if let Ok(round) = &result {
        emit_page_request(&app, round);
    }

    result
}

/// Resolves the prompt a round paused on and drives the next round. `item` is the member the prompt named; the
/// decision carries only the wizard choices, the picked path or the skip.
#[tauri::command]
#[specta::specta]
#[expect(
    clippy::too_many_arguments,
    reason = "a Tauri command takes each IPC argument and state as its own parameter"
)]
pub async fn collection_resolve_prompt(
    app: AppHandle,
    data: State<'_, Data>,
    downloads: State<'_, Downloads>,
    tasks: State<'_, TaskFlags>,
    game: String,
    profile: String,
    collection: String,
    item: String,
    decision: CollectionDecision,
    progress: Channel<CollectionProgress>,
) -> Result<CollectionRoundView, AppError> {
    // A picked file or a provider download takes the queue-aware import when the decision names a path. A FOMOD
    // choice re-imports the archive the round downloaded, which is the queue's file for this member.
    let key = TaskKey::collection_install(&game, &profile, &collection);
    let queue = downloads.inner();
    let tracked = match &decision {
        CollectionDecision::Choice { path, .. } | CollectionDecision::File { path } => {
            let was_queued = queue.importing(path).then(|| path.clone());

            if was_queued.is_some() {
                publish(&app, queue);
            }

            was_queued
        }
        CollectionDecision::MarkDone | CollectionDecision::Skip { .. } => None,
    };
    let work_app = app.clone();
    let check_item = item.clone();
    let result = crate::commands::helpers::run_task(
        data,
        &tasks,
        key,
        game,
        kind::COLLECTION,
        move |root, game, flag, handle| {
            let mut send =
                channel_sink_with_progress(progress, handle, |step: &CollectionProgress| {
                    step.percent
                });
            let downloads = work_app.state::<Downloads>();
            super::controller::resolve_prompt(
                root,
                game,
                &profile,
                &collection,
                &item,
                decision,
                downloads.inner(),
                &mut send,
                Some(flag),
            )
        },
    )
    .await;

    if let Ok(round) = &result {
        emit_page_request(&app, round);
    }

    if let Some(path) = tracked {
        // A hand-picked FOMOD still needs the wizard: keep the queue entry until the choices come back.
        let needs_wizard = result.as_ref().is_ok_and(|round| {
            round
                .install
                .needs_choice
                .iter()
                .any(|choice| choice.key == check_item)
        });

        if !needs_wizard {
            queue.imported(&path, result.as_ref().map(|_| ()));
        }

        publish(&app, queue);
    }

    result
}

/// Stops the running collection round at the next member or download chunk. The round owns the same task flag as the
/// one-command install, so this is the stop action of the Working row.
#[tauri::command]
#[specta::specta]
pub fn collection_stop(
    tasks: State<'_, TaskFlags>,
    game: String,
    profile: String,
    collection: String,
) {
    tasks.cancel(TaskKey::collection_install(&game, &profile, &collection).as_str());
}
