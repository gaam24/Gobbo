//! The download queue: links from Nexus that wait for their download, downloads left by an earlier run, and removing
//! a finished one.

use super::*;
use crate::status::sync_profiles as status;
use crate::test_support::import_from;

#[test]
fn a_link_that_cannot_be_downloaded_stays_in_the_queue_with_its_error() {
    let f = fixture("download-link-errors");
    let downloads = Downloads::default();
    let seen = std::cell::RefCell::new(Vec::new());
    let changed = |event: DownloadEvent| seen.borrow_mut().push(event);
    downloads.download_link(
        &f.data,
        "nxm://othergame/mods/5/files/7?key=k&expires=1",
        &changed,
    );
    downloads.download_link(&f.data, "nxm://broken", &changed);
    let queue = downloads.list();
    let codes: Vec<_> = queue
        .iter()
        .map(|view| {
            (
                view.state,
                view.error.as_ref().map(|error| error.code.as_str()),
            )
        })
        .collect();
    assert_eq!(
        codes,
        [
            (DownloadState::Failed, Some("source.game_not_added")),
            (DownloadState::Failed, Some("nexus.bad_link")),
        ]
    );
    assert_eq!(queue[0].name, "othergame 5");
    assert!(
        queue
            .iter()
            .all(|view| !view.auto_install && view.game.is_empty())
    );
    // Each link was shown downloading first, then failed.
    let first = match &seen.borrow()[0] {
        DownloadEvent::Queue(queue) => queue[0].state,
        DownloadEvent::Progress(view) => view.state,
        DownloadEvent::Imported(_) => DownloadState::Installed,
    };
    assert_eq!(first, DownloadState::Downloading);
    assert_eq!(seen.borrow().len(), 4);
    // A failed link can go again, and the finished ones can be cleared.
    let id = queue[1].id;
    assert_eq!(downloads.retry(id).as_deref(), Some("nxm://broken"));
    assert_eq!(downloads.list().len(), 1);
    downloads.clear_finished();
    assert!(downloads.list().is_empty());
}

#[test]
fn a_collection_owned_download_follows_its_import_and_does_not_stay_ready() {
    // A collection's download is owned, so it never auto-installs: the collection imports the file itself and the
    // archive is deleted. Its queue entry must follow that import, or the row stays "Downloaded" with a dead path and
    // clicking Install reports "no such file".
    let f = mirror_fixture("download-collection-owned");
    let zip = f.zip("Example Mod.zip", &[("bin/x64/plugins/x.asi", "x")]);
    let dir = f.data.game(MIRROR_GAME).dir.join("downloads");
    fs::create_dir_all(&dir).unwrap();
    let archive = dir.join("Example Mod.zip");
    fs::copy(&zip, &archive).unwrap();
    fs::write(
        dir.join("Example Mod.zip.source.json"),
        r#"{"provider":"nexus","mod_id":"107","file_id":"9"}"#,
    )
    .unwrap();

    let downloads = Downloads::load(&f.data);
    let ready = &downloads.list()[0];
    assert_eq!(ready.state, DownloadState::Ready);
    assert!(!ready.auto_install);
    let path = ready.path.clone().unwrap();

    assert!(downloads.importing(&path));
    assert_eq!(downloads.list()[0].state, DownloadState::Installing);
    let result = import_from(
        &f.data,
        MIRROR_GAME,
        Path::new(&path),
        None,
        None,
        &mut |_, _| {},
    );
    assert!(result.is_ok());
    assert!(downloads.imported(&path, result.as_ref().map(|_| ())));
    let after = downloads.list();
    assert_eq!(after[0].state, DownloadState::Installed);
    assert_eq!(after[0].path, None);
    assert!(!archive.exists());
}

#[test]
fn downloads_left_by_an_earlier_run_wait_ready_and_install_once() {
    let f = mirror_fixture("download-ready");
    let zip = f.zip("CET 1.38.zip", &[("bin/x64/plugins/cet.asi", "x")]);
    let dir = f.data.game(MIRROR_GAME).dir.join("downloads");
    fs::create_dir_all(&dir).unwrap();
    let archive = dir.join("CET 1.38.zip");
    fs::copy(&zip, &archive).unwrap();
    fs::write(
        dir.join("CET 1.38.zip.source.json"),
        r#"{"provider":"nexus","mod_id":"107","file_id":"9"}"#,
    )
    .unwrap();
    fs::write(dir.join("half.zip.part"), "x").unwrap();
    fs::write(dir.join("gone.zip.source.json"), "{}").unwrap();
    fs::write(dir.join("notes.txt"), "x").unwrap();

    let downloads = Downloads::load(&f.data);
    let queue = downloads.list();
    assert_eq!(queue.len(), 1);
    let ready = &queue[0];
    assert_eq!(
        (ready.state, ready.game.as_str()),
        (DownloadState::Ready, MIRROR_GAME)
    );
    assert_eq!(
        ready
            .source
            .as_ref()
            .map(|source| (source.provider.as_str(), source.mod_id.as_str())),
        Some(("nexus", "107"))
    );
    assert!(!ready.auto_install);
    assert!(!dir.join("half.zip.part").exists());
    assert!(!dir.join("gone.zip.source.json").exists());
    // Only a link's own download installs itself.
    assert!(downloads.claim(ready.id).is_none());

    let path = ready.path.clone().unwrap();
    assert!(downloads.importing(&path));
    assert_eq!(downloads.list()[0].state, DownloadState::Installing);
    // Removing a download that is being installed does nothing.
    downloads.remove(ready.id).unwrap();
    assert_eq!(downloads.list().len(), 1);
    let imported = import_from(
        &f.data,
        MIRROR_GAME,
        Path::new(&path),
        None,
        None,
        &mut |_, _| {},
    );
    assert!(downloads.imported(&path, imported.as_ref().map(|_| ())));
    assert_eq!(downloads.list()[0].state, DownloadState::Installed);
    // The library keeps its own copy; the download and its source are gone, and the source reached the library.
    assert!(!archive.exists());
    assert!(!dir.join("CET 1.38.zip.source.json").exists());
    let status = status(&f.data, MIRROR_GAME).unwrap();
    assert_eq!(
        status.mods[0]
            .sources
            .iter()
            .find(|source| source.provider == "nexus")
            .map(|source| source.mod_id.as_str()),
        Some("107")
    );
}

#[test]
fn a_download_waiting_for_the_wizard_survives_a_restart() {
    let f = mirror_fixture("download-pending-restart");
    let zip = f.zip("Installer Mod.zip", &[("fomod/module.xml", "x")]);
    let dir = f.data.game(MIRROR_GAME).dir.join("downloads");
    fs::create_dir_all(&dir).unwrap();
    fs::copy(&zip, dir.join("Installer Mod.zip")).unwrap();
    fs::write(
        dir.join("Installer Mod.zip.source.json"),
        r#"{"provider":"nexus","mod_id":"42","file_id":"9"}"#,
    )
    .unwrap();

    let downloads = Downloads::load(&f.data);
    let queued = downloads.list();
    assert_eq!(queued.len(), 1);
    let id = queued[0].id;
    assert!(!queued[0].auto_install);
    // The backend records a FOMOD installer the window still owes the user.
    downloads.mark_pending(id);
    assert!(downloads.list()[0].auto_install);
    let state_file = f.data.game(MIRROR_GAME).dir.join("downloads.json");
    assert!(state_file.is_file());

    // A restart drops the in-memory queue and rebuilds it from disk, the way `watch_links` does at startup.
    drop(downloads);
    let reloaded = Downloads::load(&f.data);
    let waiting = reloaded.list();
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].state, DownloadState::Ready);
    assert!(waiting[0].auto_install);
    assert_eq!(waiting[0].path, queued[0].path);
    assert_eq!(waiting[0].game, MIRROR_GAME);

    // The wait is still actionable once, and a claim clears it on disk so a second window cannot install it twice.
    let claimed = reloaded
        .claim(waiting[0].id)
        .expect("the pending wizard is still actionable after a restart");
    assert_eq!(claimed.path, queued[0].path);
    assert!(reloaded.claim(waiting[0].id).is_none());
    assert!(!reloaded.list()[0].auto_install);
    drop(reloaded);
    let after = Downloads::load(&f.data);
    assert!(!after.list()[0].auto_install);
    assert!(after.claim(id).is_none());
}

#[test]
fn removing_a_ready_download_deletes_it() {
    let f = mirror_fixture("download-remove");
    let zip = f.zip("mod.zip", &[("bin/x64/plugins/x.asi", "x")]);
    let dir = f.data.game(MIRROR_GAME).dir.join("downloads");
    fs::create_dir_all(&dir).unwrap();
    fs::copy(&zip, dir.join("mod.zip")).unwrap();
    fs::write(
        dir.join("mod.zip.source.json"),
        r#"{"provider":"nexus","mod_id":"1"}"#,
    )
    .unwrap();
    let downloads = Downloads::load(&f.data);
    let id = downloads.list()[0].id;
    downloads.remove(id).unwrap();
    assert!(downloads.list().is_empty());
    assert!(!dir.join("mod.zip").exists());
    assert!(!dir.join("mod.zip.source.json").exists());
}

/// A full `nxm://` link goes through link parsing, the provider download and the auto-import without a network call.
/// `for_link` still finds the registered Nexus adapter to parse the link (that needs no network); the test seam replaces
/// the provider that `fetch` and the import ask for, so the file is written and imported locally.
#[test]
fn a_provider_link_downloads_and_imports_the_mod() {
    let f = mirror_fixture("download-link-import");
    let _provider =
        crate::providers::testing::install_fake(crate::providers::testing::FakeConfig {
            entries: &[("bin/x64/plugins/example.asi", "x")],
            ..Default::default()
        });

    let downloads = Downloads::default();
    let seen = std::cell::RefCell::new(Vec::new());
    let changed = |event: DownloadEvent| seen.borrow_mut().push(event);
    downloads.download_link(
        &f.data,
        "nxm://mirror/mods/1001/files/2002?key=short-lived&expires=1735689600",
        &changed,
    );

    let queue = downloads.list();
    assert_eq!(queue.len(), 1, "the link left one row in the queue");
    assert_eq!(queue[0].state, DownloadState::Installed);
    assert_eq!(queue[0].path, None, "an installed row keeps no archive");
    assert_eq!(queue[0].game, MIRROR_GAME);
    assert!(
        seen.borrow()
            .iter()
            .any(|event| matches!(event, DownloadEvent::Imported(_))),
        "the window hears the mod the link imported"
    );

    let mods = status(&f.data, MIRROR_GAME).unwrap().mods;
    assert_eq!(mods.len(), 1, "the downloaded archive became a library mod");
    assert_eq!(mods[0].info.name, "Example Provider File");
    assert_eq!(mods[0].info.version.as_deref(), Some("2.0.0"));
    assert_eq!(mods[0].folder, "Example Provider File-1001-1.0.0");
    assert_eq!(
        mods[0]
            .sources
            .iter()
            .find(|source| source.provider == "nexus")
            .map(|source| (source.mod_id.as_str(), source.file_id.as_deref())),
        Some(("1001", Some("2002"))),
        "the link's source reached the library"
    );
    let dir = f.data.game(MIRROR_GAME).downloads();
    assert!(!dir.join("Example Provider File-1001-1.0.0.zip").exists());
    assert!(
        !dir.join("Example Provider File-1001-1.0.0.zip.source.json")
            .exists()
    );
}

/// Stopping a download while the provider is still streaming is the case the seam unlocks: the fake calls the queue's
/// `remove` from inside its transfer, which sets the in-flight stop flag. The cancelled download leaves neither archive
/// nor sidecar, no library mod, and its row is taken off the queue.
#[test]
fn a_download_cancelled_mid_transfer_leaves_no_files_and_no_row() {
    use std::cell::RefCell;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    let f = mirror_fixture("download-link-cancel");
    let downloads = Arc::new(Downloads::default());
    let id = Arc::new(AtomicU32::new(0));
    let chunks = Arc::new(AtomicUsize::new(0));
    let seen = RefCell::new(Vec::new());

    let for_remove = Arc::clone(&downloads);
    let for_id = Arc::clone(&id);
    let for_chunks = Arc::clone(&chunks);
    let on_chunk = Box::new(move || {
        for_chunks.fetch_add(1, Ordering::Relaxed);
        let id = for_id.load(Ordering::Relaxed);

        if id != 0 {
            // On a downloading row this sets the queue's in-flight stop flag, which `fetch` answers with
            // `download.cancelled` and a sweep of the partial file.
            let _ = for_remove.remove(id);
        }
    });
    let _provider =
        crate::providers::testing::install_fake(crate::providers::testing::FakeConfig {
            entries: &[("bin/x64/plugins/example.asi", "x")],
            chunks: &[(1, 10), (2, 10), (3, 10)],
            on_chunk: Some(on_chunk),
            ..Default::default()
        });

    let changed = |event: DownloadEvent| {
        if let DownloadEvent::Queue(queue) = &event
            && queue.len() == 1
        {
            id.store(queue[0].id, Ordering::Relaxed);
        }

        seen.borrow_mut().push(event);
    };
    downloads.download_link(
        &f.data,
        "nxm://mirror/mods/1001/files/2002?key=short-lived&expires=1735689600",
        &changed,
    );

    assert!(
        chunks.load(Ordering::Relaxed) > 0,
        "the cancel callback ran inside the transfer"
    );
    assert!(
        downloads.list().is_empty(),
        "a cancelled download is reported by taking its row off the queue"
    );
    let dir = f.data.game(MIRROR_GAME).downloads();
    assert!(
        !dir.join("Example Provider File-1001-1.0.0.zip").exists(),
        "a cancelled transfer leaves no archive"
    );
    assert!(
        !dir.join("Example Provider File-1001-1.0.0.zip.source.json")
            .exists(),
        "a cancelled transfer leaves no sidecar"
    );
    assert!(
        status(&f.data, MIRROR_GAME).unwrap().mods.is_empty(),
        "a cancelled download is never imported"
    );
}
