//! The window's claim of a finished download and the import lifecycle the queue records for it.

use crate::error::AppError;

use super::super::model::{DownloadState, DownloadView};
use super::Downloads;

impl Downloads {
    /// Takes a finished download of a link for its automatic install, once: `None` when it is not ready, or a window
    /// (also one from before a reload) took it already. Taking it clears the durable wait as well, so a reloaded window
    /// cannot offer the same wizard twice.
    pub fn claim(&self, id: u32) -> Option<DownloadView> {
        let view = {
            let mut queue = self.lock();
            let queued = queue.iter_mut().find(|queued| {
                queued.view.id == id
                    && queued.view.state == DownloadState::Ready
                    && queued.view.auto_install
            })?;
            queued.view.auto_install = false;
            let view = queued.view.clone();
            drop(queue);
            view
        };
        if let Some(path) = view.path.as_deref() {
            self.clear_pending_for_path(path);
        }
        Some(view)
    }

    /// The import of `path` started. Answers whether it is a download of the queue.
    pub fn importing(&self, path: &str) -> bool {
        let marked = self.mark(path, |view| {
            view.state = DownloadState::Installing;
            view.error = None;
        });
        if marked {
            // The window is installing it now, whether it came from a claim or the manual Install action.
            self.clear_pending_for_path(path);
        }
        marked
    }

    /// The import of `path` ended. After a failed one the download stays ready, with the error, for another try.
    pub fn imported(&self, path: &str, result: Result<(), &AppError>) -> bool {
        self.mark(path, |view| match result {
            Ok(()) => {
                view.state = DownloadState::Installed;
                view.path = None;
            }
            Err(error) => {
                view.state = DownloadState::Ready;
                view.error = Some(error.clone());
            }
        })
    }

    fn mark(&self, path: &str, change: impl FnOnce(&mut DownloadView)) -> bool {
        let mut queue = self.lock();
        queue
            .iter_mut()
            .find(|queued| queued.view.path.as_deref() == Some(path))
            .map(|queued| change(&mut queued.view))
            .is_some()
    }
}
