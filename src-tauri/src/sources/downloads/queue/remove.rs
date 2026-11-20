//! Taking a download off the queue: stopping a running one, deleting a ready one and retrying a failed link.

use std::path::Path;
use std::sync::atomic::Ordering as FetchOrdering;

use crate::error::AppError;
use crate::infra::sidecar::delete_download;

use super::super::model::DownloadState;
use super::Downloads;

impl Downloads {
    /// Takes a download off the queue: a running one stops (its thread takes it off), a ready one is deleted.
    pub fn remove(&self, id: u32) -> Result<(), AppError> {
        self.remove_with(id, |path| delete_download(Path::new(path)))
    }

    /// The deletion runs after the queue guard is gone, so a slow disk never holds up the progress of another download.
    pub(super) fn remove_with(
        &self,
        id: u32,
        delete: impl FnOnce(&str) -> Result<(), AppError>,
    ) -> Result<(), AppError> {
        let removed = {
            let mut queue = self.lock();
            let Some(at) = queue.iter().position(|queued| queued.view.id == id) else {
                return Ok(());
            };
            let removed = match queue[at].view.state {
                DownloadState::Downloading => {
                    queue[at].stop.store(true, FetchOrdering::Relaxed);
                    None
                }
                DownloadState::Installing => None,
                state => {
                    let path = (state == DownloadState::Ready)
                        .then(|| queue[at].view.path.clone())
                        .flatten();
                    let game = queue[at].view.game.clone();
                    queue.remove(at);
                    path.map(|path| (game, path))
                }
            };
            drop(queue);
            removed
        };

        match removed {
            Some((game, path)) => {
                self.clear_pending(&game, &path);
                delete(&path)
            }
            None => Ok(()),
        }
    }

    /// Takes a failed download of a link off the queue and gives its link back, for another try.
    pub fn retry(&self, id: u32) -> Option<String> {
        let mut queue = self.lock();
        let at = queue.iter().position(|queued| {
            queued.view.id == id && queued.view.state == DownloadState::Failed
        })?;
        queue.remove(at).link
    }

    /// Takes the installed and the failed downloads off the queue.
    pub fn clear_finished(&self) {
        self.lock().retain(|queued| {
            !matches!(
                queued.view.state,
                DownloadState::Installed | DownloadState::Failed
            )
        });
    }
}
