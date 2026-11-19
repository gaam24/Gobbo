//! The one download that is not a mod: a game tool's archive is unpacked into the game folder and then removed.

use std::path::{Path, PathBuf};

use super::{DownloadEvent, Downloads};
use crate::acquire;
use crate::context::{Data, load_settings};
use crate::games;
use crate::import::tool::unpack_tool;
use crate::infra::sidecar::delete_download;
use crate::sources::downloads::model::DownloadImported;
use crate::sources::downloads::model::DownloadState;

impl Downloads {
    /// The archive of a game tool goes into the tool's folder in the game folder, and the archive goes once it is there.
    /// Answers whether the download was a tool's; other downloads are left ready for their import.
    pub(super) fn unpack_tool_download(&self, root: &Data, id: u32) -> bool {
        let Some(view) = self.view(id) else {
            return false;
        };
        let (Some(path), Some(source)) = (view.path.clone(), view.source.clone()) else {
            return false;
        };
        let Ok(game) = games::load(&view.game) else {
            return false;
        };
        let Some(tool) = game.tool_for_source(source.provider.as_str(), &source.mod_id) else {
            return false;
        };
        self.update(id, |view| view.state = DownloadState::Installing);
        let game_root = load_settings(&root.game(&view.game)).game_root;
        let archive = PathBuf::from(&path);
        let result = unpack_tool(Path::new(game_root.trim()), tool, &archive)
            .and_then(|()| delete_download(&archive));
        self.update(id, |view| match result {
            Ok(()) => {
                view.state = DownloadState::Installed;
                view.path = None;
            }
            Err(error) => {
                view.state = DownloadState::Failed;
                view.error = Some(error);
            }
        });
        true
    }

    /// Imports a link's finished download through the shared `acquire` path, the same one a collection member takes, so
    /// the same file ends in the same library state whichever way it came. An installer archive waits for the window's
    /// wizard, recorded with the queue so a restart still offers it; a duplicate of a library file leaves the downloads
    /// folder.
    pub(super) fn import_download(&self, root: &Data, id: u32, changed: &dyn Fn(DownloadEvent)) {
        let Some(view) = self.view(id) else {
            return;
        };
        let Some(path) = view
            .path
            .clone()
            .filter(|_| view.state == DownloadState::Ready)
        else {
            return;
        };
        self.update(id, |view| view.state = DownloadState::Installing);
        changed(DownloadEvent::Queue(self.list()));
        let request = acquire::Request {
            origin: acquire::Origin::Download(Path::new(&path)),
            source: view.source.clone(),
            choices: acquire::Choices::Ask,
        };
        let result = acquire::acquire(root, &view.game, request, None, &mut |_, _| {});
        let (imported, existing) = match result {
            Ok(acquire::Acquired::Imported(imported)) => (Some(imported), None),
            Ok(acquire::Acquired::Existing { folder }) => (None, Some(folder)),
            Ok(acquire::Acquired::NeedsChoice { .. }) => {
                self.update(id, |view| view.state = DownloadState::Ready);
                // Record the wizard the window owes for this archive on disk, so a restart still offers it.
                self.mark_pending(id);
                return;
            }
            Err(error) => {
                self.update(id, |view| {
                    view.state = DownloadState::Ready;
                    view.error = Some(error);
                });
                return;
            }
        };
        self.update(id, |view| {
            view.state = DownloadState::Installed;
            view.path = None;
        });
        // The profiles take the new mods (disabled) now, as they would at the window's next status read.
        match crate::status::sync_profiles(root, &view.game) {
            Ok(status) => changed(DownloadEvent::Imported(Box::new(DownloadImported {
                game: view.game.clone(),
                name: view.name.clone(),
                view: imported,
                existing,
                status: Box::new(status),
            }))),
            Err(error) => {
                crate::infra::log::warn(format!("sync after a download import: {}", error.code));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;

    use super::*;
    use crate::context::{Settings, save_settings};
    use crate::test_support::{self, scratch};

    /// The queue auto-installs a ready download: the archive becomes a library mod, the row is marked installed with
    /// no path, and the window hears the imported mod. This drives `import_download` directly with a staged archive
    /// and sidecar, the state a download left ready on disk starts in.
    #[test]
    fn a_ready_download_imports_and_leaves_an_installed_row() {
        test_support::games::install();
        let base = scratch!("queue-import-download");
        let game_root = base.join("game");
        fs::create_dir_all(game_root.join("bin/x64")).unwrap();
        let data = Data::new(base.join("data"));
        let game = test_support::games::MIRROR_ID;
        save_settings(
            &data,
            game,
            Settings {
                game_root: game_root.display().to_string(),
                ..Settings::default()
            },
        )
        .unwrap();

        let dir = data.game(game).downloads();
        fs::create_dir_all(&dir).unwrap();
        test_support::make_zip(&dir, "Ready Mod.zip", &[("bin/x64/plugins/ready.asi", "x")]);
        fs::write(
            dir.join("Ready Mod.zip.source.json"),
            r#"{"provider":"nexus","mod_id":"107","file_id":"9"}"#,
        )
        .unwrap();

        let downloads = Downloads::load(&data);
        let ready = &downloads.list()[0];
        assert_eq!(ready.state, DownloadState::Ready);
        let id = ready.id;

        let seen = RefCell::new(Vec::new());
        let changed = |event: DownloadEvent| seen.borrow_mut().push(event);
        downloads.import_download(&data, id, &changed);

        let after = downloads.list();
        assert_eq!(after[0].state, DownloadState::Installed);
        assert_eq!(
            after[0].path, None,
            "an installed row keeps no archive path"
        );
        assert!(
            seen.borrow()
                .iter()
                .any(|event| matches!(event, DownloadEvent::Imported(_))),
            "the window hears the mod the download imported"
        );
        let mods = crate::status::sync_profiles(&data, game).unwrap().mods;
        assert_eq!(mods.len(), 1, "the ready archive became a library mod");
        assert_eq!(
            mods[0]
                .sources
                .iter()
                .find(|source| source.provider == "nexus")
                .map(|source| source.mod_id.as_str()),
            Some("107"),
            "the download's source reached the library"
        );
    }
}
