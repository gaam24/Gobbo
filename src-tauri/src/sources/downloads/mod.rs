//! The download queue shared by every provider: links that arrive, the files that run, and the ones an earlier run
//! left ready. A provider owns how it fetches a file; the queue owns the state, the progress and the sidecar.

mod model;
mod progress;
mod queue;

use std::future::Future;

use crate::error::AppError;

#[cfg(test)]
pub use model::DownloadState;
pub use model::DownloadView;
pub use queue::{DownloadEvent, Downloads};

/// The event that carries the download queue to the window after each change.
pub const DOWNLOADS_EVENT: &str = "downloads";

/// Tells the window about the download queue, and gives it back. The import command calls it when a download starts
/// or finishes installing.
pub fn publish(app: &tauri::AppHandle, downloads: &Downloads) -> Vec<DownloadView> {
    let queue = downloads.list();
    let _ = tauri::Emitter::emit(app, DOWNLOADS_EVENT, &queue);
    queue
}

/// Runs an import while keeping the download queue in step with it. When `path` names a queued download, the entry is
/// marked installing and published before the work, then marked installed — or ready again with the error the caller
/// reports — and published after. The work receives whether the file came from the queue; `queue_outcome` maps its
/// result to the outcome the queue records, so each caller keeps its own error reporting.
pub async fn import_with_queue<T, F, Fut>(
    app: &tauri::AppHandle,
    downloads: &Downloads,
    path: &str,
    import: F,
    queue_outcome: impl FnOnce(&Result<T, AppError>) -> Result<(), &AppError>,
) -> Result<T, AppError>
where
    F: FnOnce(bool) -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    let from_queue = downloads.importing(path);
    if from_queue {
        publish(app, downloads);
    }
    let result = import(from_queue).await;
    if from_queue {
        downloads.imported(path, queue_outcome(&result));
        publish(app, downloads);
    }
    result
}

/// How often the window hears about a download in progress.
const PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_millis(250);
