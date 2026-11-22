//! The loader install against a local HTTP server: the real download, the hash check, the import into the library as one
//! game-path mod, and the game folder that stays untouched.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::AtomicBool;
use std::thread;

use sha2::Sha256;

use crate::context::{Data, Settings};
use crate::infra::io::digest_file;
use crate::test_support::{games as test_games, make_zip, scratch};

use super::{LoaderSetupState, install, view};

fn settings(root: &std::path::Path) -> Settings {
    Settings {
        game_root: root.display().to_string(),
        ..Settings::default()
    }
}

fn serve(bytes: Vec<u8>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            bytes.len()
        )
        .unwrap();
        stream.write_all(&bytes).unwrap();
    });
    (format!("http://{address}/loader.zip"), handle)
}

/// A game folder with the fixture's project folder, the data folder and the game definition.
fn fixture(
    name: &str,
) -> (
    std::path::PathBuf,
    std::path::PathBuf,
    Data,
    crate::games::GameDef,
) {
    let base = scratch!(name);
    let game_root = base.join("game");
    fs::create_dir_all(game_root.join("Project")).unwrap();
    let data = Data::new(base.join("data"));
    (base, game_root, data, test_games::unreal())
}

fn release(base: &std::path::Path) -> (Vec<u8>, String) {
    let zip = make_zip(
        base,
        "release.zip",
        &[
            ("dwmapi.dll", "proxy"),
            ("ue4ss/UE4SS.dll", "loader"),
            ("ue4ss/UE4SS-settings.ini", "settings"),
            ("ue4ss/Mods/mods.txt", "list"),
            ("ue4ss/Mods/BPModLoaderMod/Scripts/main.lua", "bundled"),
        ],
    );
    let hash = digest_file::<Sha256>(&zip, &mut |_| {}).unwrap();
    (fs::read(&zip).unwrap(), hash)
}

fn point_at(game: &mut crate::games::GameDef, url: String, sha256: String, size: Option<u64>) {
    let install = game.install.as_mut().unwrap();
    install.url = url;
    install.sha256 = sha256;
    install.size = size;
}

fn not_cancelled() -> AtomicBool {
    AtomicBool::new(false)
}

fn enabled_folders(data: &Data, game: &str) -> Vec<String> {
    crate::status::sync_profiles(data, game)
        .unwrap()
        .mods
        .into_iter()
        .filter(|item| item.enabled)
        .map(|item| item.folder)
        .collect()
}

#[test]
fn the_loader_becomes_one_enabled_mod_with_game_paths_and_the_game_folder_stays_clean() {
    let (base, game_root, data, mut game) = fixture("loader-install");
    let (bytes, hash) = release(&base);
    let size = u64::try_from(bytes.len()).unwrap();
    let (url, server) = serve(bytes);
    point_at(&mut game, url, hash, Some(size));
    let settings = settings(&game_root);

    assert_eq!(
        view(&data, &game, &settings).unwrap().unwrap().state,
        LoaderSetupState::Missing
    );
    let after = install(&data, &game, &settings, &not_cancelled(), &mut |_| {}).unwrap();
    server.join().unwrap();

    assert_eq!(after.state, LoaderSetupState::Installed);
    assert_eq!(after.installed_version.as_deref(), Some("4.0.0"));
    assert!(
        !game_root.join("Project/Binaries").exists(),
        "the game folder must stay untouched"
    );

    let library = crate::context::load_library(&data.game(&game.id), &game).unwrap();
    assert_eq!(
        library.versions.len(),
        1,
        "the release is one mod, bundled mods included"
    );
    let copy = library.versions.values().next().unwrap();
    assert!(copy.slot.is_empty());
    for path in [
        "Binaries/Win64/dwmapi.dll",
        "Binaries/Win64/ue4ss/UE4SS.dll",
        "Binaries/Win64/ue4ss/Mods/BPModLoaderMod/Scripts/main.lua",
    ] {
        assert!(
            copy.files.iter().any(|file| file == path),
            "{path}: {:?}",
            copy.files
        );
    }
    assert_eq!(
        enabled_folders(&data, &game.id),
        std::slice::from_ref(&copy.folder)
    );
}

#[test]
fn a_copy_in_the_library_is_enabled_without_a_download() {
    let (base, game_root, data, mut game) = fixture("loader-reuse");
    let (bytes, hash) = release(&base);
    let (url, server) = serve(bytes);
    point_at(&mut game, url, hash, None);
    let settings = settings(&game_root);
    install(&data, &game, &settings, &not_cancelled(), &mut |_| {}).unwrap();
    server.join().unwrap();
    let folder = enabled_folders(&data, &game.id).remove(0);
    let family = crate::context::load_library(&data.game(&game.id), &game)
        .unwrap()
        .versions[&folder]
        .family
        .clone();
    crate::profiles::edits::set_mod_enabled(&data, &game.id, &family, false).unwrap();
    assert!(enabled_folders(&data, &game.id).is_empty());

    // No server answers now: the copy in the library is used.
    point_at(
        &mut game,
        "https://127.0.0.1:9/unused.zip".into(),
        "0".repeat(64),
        None,
    );
    install(&data, &game, &settings, &not_cancelled(), &mut |_| {}).unwrap();
    assert_eq!(enabled_folders(&data, &game.id), [folder]);
}

#[test]
fn a_manual_install_in_the_game_folder_counts_as_installed() {
    let (_base, game_root, data, game) = fixture("loader-manual");
    let win64 = game_root.join("Project/Binaries/Win64");
    fs::create_dir_all(win64.join("ue4ss")).unwrap();
    fs::write(win64.join("dwmapi.dll"), b"proxy").unwrap();
    fs::write(win64.join("ue4ss/UE4SS.dll"), b"loader").unwrap();

    let view = view(&data, &game, &settings(&game_root)).unwrap().unwrap();
    assert_eq!(view.state, LoaderSetupState::Installed);
    assert!(view.installed_version.is_none());
    assert!(view.source.contains("UE4SS"));
    assert_eq!(view.target, "Binaries/Win64");
}

#[test]
fn a_wrong_hash_imports_nothing() {
    let (base, game_root, data, mut game) = fixture("loader-hash");
    let (bytes, _) = release(&base);
    let (url, server) = serve(bytes);
    point_at(&mut game, url, "0".repeat(64), None);

    let error = install(
        &data,
        &game,
        &settings(&game_root),
        &not_cancelled(),
        &mut |_| {},
    )
    .unwrap_err();
    server.join().unwrap();
    assert_eq!(error.code, "loader.hash");
    assert!(
        crate::context::load_library(&data.game(&game.id), &game)
            .unwrap()
            .versions
            .is_empty()
    );
}

#[test]
fn a_download_larger_than_its_named_size_stops() {
    let (base, game_root, data, mut game) = fixture("loader-too-large");
    let (bytes, hash) = release(&base);
    let (url, server) = serve(bytes);
    point_at(&mut game, url, hash, Some(1));

    let error = install(
        &data,
        &game,
        &settings(&game_root),
        &not_cancelled(),
        &mut |_| {},
    )
    .unwrap_err();
    server.join().unwrap();
    assert_eq!(error.code, "loader.too_large");
}

#[test]
fn a_cancelled_install_downloads_nothing() {
    let (_base, game_root, data, game) = fixture("loader-cancelled");
    let error = install(
        &data,
        &game,
        &settings(&game_root),
        &AtomicBool::new(true),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(error.is_cancelled(), "{}", error.code);
}
