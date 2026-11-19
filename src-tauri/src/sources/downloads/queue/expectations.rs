//! Collection-session expectations: which file a session is about to ask the user for, and who owns it once found.

use std::sync::PoisonError;

use crate::domain::source::SourceRef;

use super::super::model::DownloadState;
use super::{Downloads, Expectation};

impl Downloads {
    /// Says that `owner` is about to ask the user for `source`. The next matching link is owned by that collection
    /// session and is not auto-installed by the window.
    pub fn expect_file(&self, game: &str, source: SourceRef, owner: String) {
        let mut expectations = self
            .expectations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        expectations.retain(|known| known.owner != owner);
        expectations.push(Expectation {
            game: game.to_string(),
            source,
            owner,
        });
    }

    /// Drops an expectation when the user skips or cancels the member.
    pub fn forget(&self, owner: &str) {
        self.expectations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|known| known.owner != owner);
    }

    /// Gives an already ready download to `owner`, after a restart where the expectation was lost.
    pub fn adopt(&self, game: &str, source: &SourceRef, owner: &str) -> Option<u32> {
        let mut queue = self.lock();
        let queued = queue.iter_mut().find(|queued| {
            queued.view.state == DownloadState::Ready
                && queued.view.owner.is_none()
                && queued.view.game == game
                && queued
                    .view
                    .source
                    .as_ref()
                    .is_some_and(|known| known.same_file(source))
        })?;
        queued.view.owner = Some(owner.to_string());
        queued.view.auto_install = false;
        let id = queued.view.id;
        let path = queued.view.path.clone();
        drop(queue);
        // The collection brings its own FOMOD choices, so the generic wizard is no longer owed.
        if let Some(path) = path {
            self.clear_pending(game, &path);
        }
        Some(id)
    }

    pub(super) fn take_expectation(&self, game: &str, source: &SourceRef) -> Option<String> {
        let mut expectations = self
            .expectations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let at = expectations
            .iter()
            .position(|known| known.game == game && known.source.same_file(source))?;
        Some(expectations.remove(at).owner)
    }
}
