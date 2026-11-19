//! The queue of provider downloads: links that arrive, the downloads that run, and the ones an earlier run left
//! ready.

mod claim;
mod dedup;
mod expectations;
mod game;
mod install;
mod pending;
mod remove;
mod start;

#[cfg(test)]
use super::model::DownloadState;
use super::model::DownloadView;
use crate::domain::source::SourceRef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering as FetchOrdering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// The queue's state file, beside a game's downloads folder. It keeps the archives that still wait for the window's
/// FOMOD wizard across a restart.
const PENDING_FILE: &str = "downloads.json";

/// The durable part of the queue: which downloaded archives still wait for the window's wizard.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
struct PendingState {
    /// Archive file names in the game's downloads folder that need the wizard.
    files: BTreeSet<String>,
}

/// The file name of an archive in the downloads folder, the key used by [`PENDING_FILE`].
fn archive_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// The downloads of this run, and the ones an earlier run left ready in the games' folders.
#[derive(Default)]
pub struct Downloads {
    queue: Mutex<Vec<Queued>>,
    last_id: AtomicU32,
    /// A collection session says which file it is about to ask the user for. The matching link is then owned by that
    /// session instead of being auto-installed by the window.
    expectations: Mutex<Vec<Expectation>>,
    /// The app data folder, when this queue was loaded from `Data`. Without one, a hand-built queue keeps its state
    /// in memory only.
    data_root: Option<PathBuf>,
    /// The pending archives of each game, in step with their state files on disk.
    pending: Mutex<BTreeMap<String, PendingState>>,
}

struct Expectation {
    game: String,
    source: SourceRef,
    owner: String,
}

struct Queued {
    view: DownloadView,
    /// The link it came from, for another try.
    link: Option<String>,
    /// Set to stop the download.
    stop: Arc<AtomicBool>,
}

/// What the window hears about the queue. A download in progress changes a few times a second; sending the whole
/// queue each time cloned every other entry for nothing, so the progress goes out as the one changed view.
pub enum DownloadEvent {
    /// The whole queue, after an entry appears or leaves.
    Queue(Vec<DownloadView>),
    /// One entry whose progress or state changed.
    Progress(Box<DownloadView>),
    /// A link's download the backend imported on its own.
    Imported(Box<super::model::DownloadImported>),
}

impl Downloads {
    pub fn list(&self) -> Vec<DownloadView> {
        self.lock()
            .iter()
            .map(|queued| queued.view.clone())
            .collect()
    }

    /// One download of the queue, for the progress event.
    fn view(&self, id: u32) -> Option<DownloadView> {
        self.lock()
            .iter()
            .find(|queued| queued.view.id == id)
            .map(|queued| queued.view.clone())
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Queued>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn push(&self, view: DownloadView, link: Option<String>) -> (u32, Arc<AtomicBool>) {
        let id = self.last_id.fetch_add(1, FetchOrdering::Relaxed) + 1;
        let stop = Arc::new(AtomicBool::new(false));
        self.lock().push(Queued {
            view: DownloadView { id, ..view },
            link,
            stop: Arc::clone(&stop),
        });
        (id, stop)
    }

    fn update(&self, id: u32, change: impl FnOnce(&mut DownloadView)) {
        if let Some(queued) = self.lock().iter_mut().find(|queued| queued.view.id == id) {
            change(&mut queued.view);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::game::first_game;
    use super::*;

    #[test]
    fn two_starts_of_the_same_link_leave_one_download() {
        let downloads = Downloads::default();
        let source = SourceRef::new("nexus", "7").with_file("9");
        let (first, _) = downloads.push(
            DownloadView::new("first".into(), DownloadState::Downloading),
            None,
        );
        assert!(!downloads.start_link(first, "game", &source, None));
        let (second, _) = downloads.push(
            DownloadView::new("second".into(), DownloadState::Downloading),
            None,
        );
        assert!(downloads.start_link(second, "game", &source, None));
        let queue = downloads.list();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].id, first);
        assert_eq!(queue[0].game, "game");
        assert_eq!(queue[0].source.as_ref(), Some(&source));
    }

    #[test]
    fn removing_a_ready_download_leaves_the_queue_free_for_progress() {
        let downloads = Downloads::default();
        let (other, _) = downloads.push(
            DownloadView::new("other".into(), DownloadState::Downloading),
            None,
        );
        let mut ready = DownloadView::new("ready".into(), DownloadState::Ready);
        ready.path = Some("ready.zip".into());
        let (ready, _) = downloads.push(ready, None);
        downloads
            .remove_with(ready, |path| {
                assert_eq!(path, "ready.zip");
                assert!(
                    downloads.queue.try_lock().is_ok(),
                    "the queue guard must be gone before the file is deleted"
                );
                downloads.update(other, |view| view.received_kb = 5);
                Ok(())
            })
            .unwrap();
        let queue = downloads.list();
        assert_eq!(queue.len(), 1);
        assert_eq!((queue[0].id, queue[0].received_kb), (other, 5));
    }

    #[test]
    fn a_link_without_a_game_takes_the_first_listed_game_the_user_has() {
        let keys = [
            "modio".to_string(),
            "examplegame".to_string(),
            "othergame".to_string(),
        ];
        let added = |key: &str| (key != "modio").then(|| format!("game-{key}"));
        assert_eq!(
            first_game(&keys, added),
            Some(("game-examplegame".to_string(), "examplegame".to_string()))
        );
        assert_eq!(first_game(&keys[..1], added), None);
        assert_eq!(first_game(&[], added), None);
    }
}
