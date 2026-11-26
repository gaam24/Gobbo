//! The one-command automatic collection install and its stop action.

use tauri::State;
use tauri::ipc::Channel;

use crate::collections::download;
use crate::collections::model::CollectionProgress;
use crate::commands::helpers::channel_sink_with_progress;
use crate::context::Data;
use crate::error::AppError;
use crate::infra::tasks::TaskFlags;
use crate::infra::tasks::TaskKey;
use crate::infra::tasks::kind;

/// Installs the selected members automatically. A FOMOD without stored choices is returned in `needs_choice`; the
/// window imports it with its wizard and calls the command again until the list is empty.
#[tauri::command]
#[specta::specta]
pub async fn install_collection(
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    profile: String,
    collection: String,
    progress: Channel<CollectionProgress>,
) -> Result<super::model::CollectionInstallView, AppError> {
    let key = TaskKey::collection_install(&game, &profile, &collection);
    crate::commands::helpers::run_task(
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
            download::install_automatic(root, game, &profile, &collection, &mut send, Some(flag))
        },
    )
    .await
}
