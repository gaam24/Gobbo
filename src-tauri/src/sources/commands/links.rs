use std::sync::{Mutex, PoisonError};

use tauri::{App, AppHandle, Emitter, Manager, State};
use tauri_plugin_deep_link::DeepLinkExt;

use crate::commands::blocking;
use crate::context::Data;
use crate::error::AppError;
use crate::providers;
use crate::sources::downloads::{DOWNLOADS_EVENT, DownloadEvent, DownloadView, Downloads, publish};

/// One download in progress. The window merges it into the queue it already has instead of receiving every entry again.
const DOWNLOAD_PROGRESS_EVENT: &str = "downloads-progress";

/// The event that tells the window a link's download was imported by the backend.
const DOWNLOAD_IMPORTED_EVENT: &str = "download-imported";

/// The event that carries a collection link to the window.
pub const COLLECTION_LINK_EVENT: &str = "collection-link";

/// A collection link that arrived before the window listened, so a second start does not lose it.
#[derive(Default)]
pub struct CollectionLinkPending(pub Mutex<Option<String>>);

/// Takes the pending collection link, if a deep link arrived before the window asked for it.
#[tauri::command]
#[specta::specta]
pub fn take_collection_link(pending: State<'_, CollectionLinkPending>) -> Option<String> {
    pending
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

/// Drops a collection download expectation when the user skips or cancels the member.
#[tauri::command]
#[specta::specta]
pub fn forget_collection_download(downloads: State<'_, Downloads>, owner: String) {
    downloads.forget(&owner);
}

/// Takes each link that reaches the app: a pack link goes to the pack dialog, and a provider link starts its download
/// on a thread of its own. The window hears about the queue as it changes.
fn receive_links(app: &AppHandle, urls: impl IntoIterator<Item = String>) {
    for url in urls {
        if crate::packs::link::is_pack_link(&url) {
            crate::packs::commands::queue_pack_link(app, &url);
            continue;
        }

        if providers::collection_link(&url).is_some() {
            if let Some(state) = app.try_state::<CollectionLinkPending>() {
                *state.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(url.clone());
            }

            let _ = app.emit(COLLECTION_LINK_EVENT, url);
            continue;
        }

        if !providers::is_source_link(&url) {
            continue;
        }

        let app = app.clone();
        std::thread::spawn(move || {
            let data = app.state::<Data>();
            let changed = |event: DownloadEvent| match event {
                DownloadEvent::Queue(queue) => {
                    let _ = app.emit(DOWNLOADS_EVENT, queue);
                }
                DownloadEvent::Progress(view) => {
                    let _ = app.emit(DOWNLOAD_PROGRESS_EVENT, view);
                }
                DownloadEvent::Imported(imported) => {
                    let _ = app.emit(DOWNLOAD_IMPORTED_EVENT, &*imported);
                }
            };
            app.state::<Downloads>()
                .download_link(&data, &url, &changed);
        });
    }
}

/// Downloads every `nxm://` link that reaches the app, from its start or from a second start. The queue lives in the
/// app, so a link that comes before the window, or during a reload of it, is not lost.
pub fn watch_links(app: &App) {
    app.manage(Downloads::load(&app.state::<Data>()));
    app.manage(CollectionLinkPending::default());

    if let Ok(Some(urls)) = app.deep_link().get_current() {
        receive_links(app.handle(), urls.iter().map(ToString::to_string));
    }

    let handle = app.handle().clone();
    app.deep_link().on_open_url(move |event| {
        receive_links(&handle, event.urls().iter().map(ToString::to_string));
    });
}

/// The download queue: what comes in from the providers, what waits for its import, and what this run installed.
#[tauri::command]
#[specta::specta]
pub fn list_downloads(downloads: State<'_, Downloads>) -> Vec<DownloadView> {
    downloads.list()
}

/// Takes a finished download of a link for its automatic install. `None` when it was taken already.
#[tauri::command]
#[specta::specta]
pub fn claim_download(downloads: State<'_, Downloads>, id: u32) -> Option<DownloadView> {
    downloads.claim(id)
}

/// Stops a running download, or deletes a downloaded file, and takes it off the queue.
#[tauri::command]
#[specta::specta]
pub async fn remove_download(app: AppHandle, id: u32) -> Result<Vec<DownloadView>, AppError> {
    blocking(move || {
        let downloads = app.state::<Downloads>();
        downloads.remove(id)?;
        Ok(publish(&app, &downloads))
    })
    .await
}

/// Downloads a failed link again. The key in a link expires after a while; then only a new click on the provider helps.
#[tauri::command]
#[specta::specta]
pub fn retry_download(
    app: AppHandle,
    downloads: State<'_, Downloads>,
    id: u32,
) -> Vec<DownloadView> {
    if let Some(link) = downloads.retry(id) {
        receive_links(&app, [link]);
    }

    publish(&app, &downloads)
}

/// Takes the installed and the failed downloads off the queue.
#[tauri::command]
#[specta::specta]
pub fn clear_downloads(app: AppHandle, downloads: State<'_, Downloads>) -> Vec<DownloadView> {
    downloads.clear_finished();
    publish(&app, &downloads)
}
