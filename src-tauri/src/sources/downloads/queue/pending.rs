//! The durable part of the queue: the archives that still wait for the window's FOMOD wizard across a restart.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::{MutexGuard, PoisonError};

use crate::context::Data;
use crate::context::managed::managed_ids;
use crate::context::size_kb;
use crate::infra::http::PART_SUFFIX;
use crate::infra::sidecar::{SOURCE_SUFFIX, read_source};

use super::super::model::{DownloadState, DownloadView};
use super::{Downloads, PENDING_FILE, PendingState, archive_name};

impl Downloads {
    /// The queue with the downloads an earlier run left in the added games' folders, ready to install. What a run
    /// that ended in the middle left half done goes.
    pub fn load(root: &Data) -> Self {
        let downloads = Self {
            data_root: Some(root.dir.clone()),
            ..Self::default()
        };
        for game in managed_ids(root).unwrap_or_default() {
            let mut pending = downloads
                .pending_file(&game)
                .map(|path| crate::infra::json::read_json_or_default::<PendingState>(&path))
                .unwrap_or_default();
            let Ok(entries) = fs::read_dir(root.game(&game).downloads()) else {
                continue;
            };
            let mut files: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect();
            files.sort();
            let mut on_disk = BTreeSet::new();
            for path in files {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let orphan = name
                    .strip_suffix(SOURCE_SUFFIX)
                    .is_some_and(|archive| !path.with_file_name(archive).is_file());
                if name.ends_with(PART_SUFFIX) || orphan {
                    let _ = fs::remove_file(&path);
                    continue;
                }
                if !path.is_file() || !crate::infra::archive::is_archive(&name) {
                    continue;
                }
                on_disk.insert(name.clone());
                let size = size_kb(fs::metadata(&path).map_or(0, |meta| meta.len()));
                let source = read_source(&path);
                // The wizard the window was showing before the restart is still owed to this archive.
                let needs_choice = pending.files.contains(&name);
                downloads.push(
                    DownloadView {
                        game: game.clone(),
                        source,
                        received_kb: size,
                        total_kb: size,
                        path: Some(path.display().to_string()),
                        auto_install: needs_choice,
                        ..DownloadView::new(name, DownloadState::Ready)
                    },
                    None,
                );
            }
            // An archive that is gone must not leave a wizard behind after a restart.
            let before = std::mem::take(&mut pending.files);
            let before_count = before.len();
            pending.files = before
                .into_iter()
                .filter(|name| on_disk.contains(name))
                .collect();
            let changed = before_count != pending.files.len();
            {
                let mut all = downloads.pending_lock();
                if pending.files.is_empty() {
                    all.remove(&game);
                } else {
                    all.insert(game.clone(), pending);
                }
            }
            if changed {
                downloads.save_pending(&game);
            }
        }
        downloads
    }

    /// Locks the in-memory copy of the durable state. A poisoned mutex holds no partial state: each change replaces one
    /// game's whole set, so continuing with the inner value is safe.
    fn pending_lock(&self) -> MutexGuard<'_, BTreeMap<String, PendingState>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The state file of one game, beside its downloads folder.
    fn pending_file(&self, game: &str) -> Option<PathBuf> {
        self.data_root.as_ref().map(|root| {
            root.join(crate::migrations::GAMES_DIR)
                .join(game)
                .join(PENDING_FILE)
        })
    }

    /// Writes one game's pending archives through the shared atomic JSON helper. A failed write only loses the wait,
    /// never a download: the archive is still on disk and the window can offer the install again.
    fn save_pending(&self, game: &str) {
        let Some(path) = self.pending_file(game) else {
            return;
        };
        let state = self.pending_lock().get(game).cloned().unwrap_or_default();
        if let Err(error) = crate::infra::json::write_json(&path, &state) {
            crate::infra::log::warn(format!(
                "saving pending downloads for {game}: {}",
                error.code
            ));
        }
    }

    /// Keeps one archive's pending flag in step with its state file.
    pub(super) fn set_pending(&self, game: &str, file: &str, waiting: bool) {
        let mut all = self.pending_lock();
        let entry = all.entry(game.to_string()).or_default();
        if waiting {
            entry.files.insert(file.to_string());
        } else {
            entry.files.remove(file);
        }
        let drop_entry = entry.files.is_empty();
        drop(all);
        if drop_entry {
            self.pending_lock().remove(game);
        }
        self.save_pending(game);
    }

    /// Forgets the wait of the queued download whose archive is `path`, when there is one.
    pub(super) fn clear_pending_for_path(&self, path: &str) {
        let game = self
            .lock()
            .iter()
            .find(|queued| queued.view.path.as_deref() == Some(path))
            .map(|queued| queued.view.game.clone());
        if let Some(game) = game {
            self.clear_pending(&game, path);
        }
    }

    /// Forgets the wait of one archive of a known game.
    pub(super) fn clear_pending(&self, game: &str, path: &str) {
        if let Some(file) = archive_name(path) {
            self.set_pending(game, &file, false);
        }
    }
}
