//! The guard that keeps two starts of the same link from running two downloads.

use crate::domain::source::SourceRef;

use super::super::model::DownloadState;
use super::Downloads;

impl Downloads {
    /// Records the game and file of a starting download. Answers whether another active entry already holds the same
    /// file; then this entry leaves the queue. One guard makes two starts of the same link see each other, so exactly
    /// one download runs. A failed entry of the same file is dropped first, so the queue does not show its old error.
    pub(super) fn start_link(
        &self,
        id: u32,
        game: &str,
        source: &SourceRef,
        owner: Option<&str>,
    ) -> bool {
        let mut queue = self.lock();
        queue.retain(|queued| {
            queued.view.id == id
                || !(queued.view.state == DownloadState::Failed
                    && queued.view.game == game
                    && queued
                        .view
                        .source
                        .as_ref()
                        .is_some_and(|known| known.same_file(source)))
        });
        let duplicate = queue.iter().any(|queued| {
            queued.view.id != id
                && matches!(
                    queued.view.state,
                    DownloadState::Downloading | DownloadState::Ready | DownloadState::Installing
                )
                && queued.view.game == game
                && queued
                    .view
                    .source
                    .as_ref()
                    .is_some_and(|known| known.same_file(source))
        });

        if duplicate {
            let adopted_wait = if let Some(existing) = queue.iter_mut().find(|queued| {
                queued.view.id != id
                    && queued
                        .view
                        .source
                        .as_ref()
                        .is_some_and(|known| known.same_file(source))
            }) && let Some(owner) = owner
            {
                existing.view.owner = Some(owner.to_string());
                existing.view.auto_install = false;
                Some((existing.view.game.clone(), existing.view.path.clone()))
            } else {
                None
            };
            queue.retain(|queued| queued.view.id != id);
            drop(queue);

            if let Some((game, Some(path))) = adopted_wait {
                self.clear_pending(&game, &path);
            }

            return true;
        }

        if let Some(queued) = queue.iter_mut().find(|queued| queued.view.id == id) {
            queued.view.game = game.to_string();
            queued.view.source = Some(source.clone());

            if let Some(owner) = owner {
                queued.view.owner = Some(owner.to_string());
                queued.view.auto_install = false;
            }
        }

        drop(queue);
        false
    }
}
