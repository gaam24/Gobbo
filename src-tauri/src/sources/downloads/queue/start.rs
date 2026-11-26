//! The link download flow: starting a provider link, fetching its file and recording the wizard the window owes.

use std::fs;
use std::sync::atomic::AtomicBool;

use crate::context::Data;
use crate::context::size_kb;
use crate::error::{AppError, DOWNLOAD_CANCELLED};
use crate::providers;
use crate::providers::fetch::{Fetch as FetchRequest, fetch_with_progress};

use super::super::model::{DownloadState, DownloadView, download_name};
use super::super::progress::Meter;
use super::game::game_for_link;
use super::{DownloadEvent, Downloads, archive_name};

impl Downloads {
    /// Downloads the file of a provider deep link on this thread. `changed` hears the queue after each step, and the
    /// one changed download a few times a second while the file comes in.
    pub fn download_link(&self, root: &Data, link: &str, changed: &dyn Fn(DownloadEvent)) {
        let name = providers::for_link(link)
            .and_then(|provider| provider.parse_link(link).ok())
            .map_or_else(
                || link.to_string(),
                |link| download_name(link.game_key.as_deref(), &link.source.mod_id),
            );
        let view = DownloadView::new(name, DownloadState::Downloading);
        let (id, stop) = self.push(view, Some(link.to_string()));
        changed(DownloadEvent::Queue(self.list()));

        match self.fetch_link(root, id, link, &stop, changed) {
            Ok(()) => {
                let owned = self.view(id).is_some_and(|view| view.owner.is_some());

                if !self.unpack_tool_download(root, id) && !owned {
                    self.import_download(root, id, changed);
                }
            }
            Err(error) if error.code == DOWNLOAD_CANCELLED => {
                self.lock().retain(|queued| queued.view.id != id);
            }
            Err(error) => {
                self.update(id, |view| {
                    view.state = DownloadState::Failed;
                    view.speed_kb = 0;
                    view.error = Some(error);
                });
            }
        }

        changed(DownloadEvent::Queue(self.list()));
    }

    fn fetch_link(
        &self,
        root: &Data,
        id: u32,
        link: &str,
        stop: &AtomicBool,
        changed: &dyn Fn(DownloadEvent),
    ) -> Result<(), AppError> {
        let provider =
            providers::for_link(link).ok_or_else(|| providers::bad_link().with("url", link))?;
        let parsed = provider.parse_link(link)?;
        let (game, game_key) = game_for_link(root, provider, &parsed)?;
        let source = parsed.source;
        let owner = self.take_expectation(&game, &source);
        // The same link may arrive twice (a double click, a second start). One download is enough.
        if self.start_link(id, &game, &source, owner.as_deref()) {
            return Ok(());
        }

        let download_key = parsed.download_key.as_deref().zip(parsed.expires);
        let mut meter = Meter::new();
        let downloaded = fetch_with_progress(
            FetchRequest {
                source,
                game_key: Some(&game_key),
                directory: &root.game(&game).downloads(),
                archive_name: None,
                download_key,
            },
            Some(stop),
            &mut |received, total| {
                if let Some(speed) = meter.tick(received) {
                    self.update(id, |view| {
                        view.received_kb = size_kb(received);
                        view.total_kb = size_kb(total);
                        view.speed_kb = speed;
                    });

                    if let Some(view) = self.view(id) {
                        changed(DownloadEvent::Progress(Box::new(view)));
                    }
                }
            },
        )?;
        let path = downloaded.path;
        let resolved = downloaded.source;
        let size = size_kb(fs::metadata(&path).map_or(0, |meta| meta.len()));
        let name = path.file_name().map_or_else(
            || download_name(Some(&game_key), &resolved.mod_id),
            |name| name.to_string_lossy().into_owned(),
        );
        let path = path.display().to_string();
        let mut queue = self.lock();
        // The same file downloaded again replaces the earlier download, which is no longer on the disk as it was.
        queue.retain(|queued| {
            queued.view.id == id
                || queued.view.path.as_deref() != Some(path.as_str())
                || queued.view.state == DownloadState::Installing
        });

        if let Some(queued) = queue.iter_mut().find(|queued| queued.view.id == id) {
            let view = &mut queued.view;
            view.state = DownloadState::Ready;
            // The backend imports the download next (`import_download`); a game tool's archive is unpacked instead, and
            // an owned one is imported by its collection session. Only an installer archive asks the window.
            view.auto_install = false;
            view.name.clone_from(&name);
            view.received_kb = size;
            view.total_kb = size;
            view.speed_kb = 0;
            view.path = Some(path.clone());
            view.source = Some(resolved);
        }

        drop(queue);
        // A fresh download of a previously pending archive has no wait: the new import fetches its own choices.
        self.clear_pending_for_path(&path);
        Ok(())
    }

    /// Marks the ready download `id` as waiting for the window's FOMOD wizard, so the wait survives a restart.
    pub fn mark_pending(&self, id: u32) {
        let Some(view) = self.view(id) else {
            return;
        };
        let Some(file) = view.path.as_deref().and_then(archive_name) else {
            return;
        };
        self.update(id, |view| view.auto_install = true);
        self.set_pending(&view.game, &file, true);
    }
}
