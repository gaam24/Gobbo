//! Preparing the next guided member and importing the file the user picked for it.

use std::path::Path;

use tauri::{AppHandle, Manager, State};

use crate::collections::download;
use crate::collections::model::{CollectionItemAction, CollectionSessionView};
use crate::commands::blocking;
use crate::context::Data;
use crate::error::AppError;
use crate::sources::downloads::Downloads;

/// The guided flow's next member: a Nexus file page, an automatic bundle/direct install, or a manual instruction.
#[tauri::command]
#[specta::specta]
pub async fn prepare_collection_item(
    app: AppHandle,
    game: String,
    profile: String,
    collection: String,
    item: String,
) -> Result<CollectionItemAction, AppError> {
    blocking(move || {
        let data = app.state::<Data>();
        let downloads = app.state::<crate::sources::downloads::Downloads>();
        let action = download::prepare_item(
            data.inner(),
            &game,
            &profile,
            &collection,
            &item,
            downloads.inner(),
        )?;

        match &action {
            CollectionItemAction::Waiting { url, .. }
            | CollectionItemAction::NeedsUser { url: Some(url), .. } => {
                tauri_plugin_opener::open_url(url, None::<&str>).map_err(AppError::unexpected)?;
            }
            _ => {}
        }

        Ok(action)
    })
    .await
}

/// Imports a file the user picked for one member, with the wizard's choices when the archive is a FOMOD.
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::too_many_arguments,
    reason = "a Tauri command takes each IPC argument and state as its own parameter"
)]
pub async fn import_collection_file(
    app: AppHandle,
    data: State<'_, Data>,
    downloads: State<'_, Downloads>,
    game: String,
    profile: String,
    collection: String,
    item: String,
    path: String,
    choices: Option<Vec<crate::domain::fomod::Choice>>,
) -> Result<CollectionSessionView, AppError> {
    let root = data.inner().clone();
    // The helper borrows `path` for the queue bookkeeping; the work owns a clone of it.
    let work_path = path.clone();
    crate::sources::downloads::import_with_queue(
        &app,
        downloads.inner(),
        &path,
        |_| {
            crate::commands::blocking(move || {
                download::import_file(
                    &root,
                    &game,
                    &profile,
                    &collection,
                    &item,
                    Path::new(&work_path),
                    choices,
                )
            })
        },
        |result| result.as_ref().map(|_| ()),
    )
    .await
}
