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

use super::{LoaderChoice, LoaderSetupState, install, view};

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
    let after = install(
        &data,
        &game,
        &settings,
        &LoaderChoice::Auto,
        &not_cancelled(),
        &mut |_| {},
    )
    .unwrap();
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
    install(
        &data,
        &game,
        &settings,
        &LoaderChoice::Auto,
        &not_cancelled(),
        &mut |_| {},
    )
    .unwrap();
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
    install(
        &data,
        &game,
        &settings,
        &LoaderChoice::Auto,
        &not_cancelled(),
        &mut |_| {},
    )
    .unwrap();
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
        &LoaderChoice::Auto,
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
        &LoaderChoice::Auto,
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
        &LoaderChoice::Auto,
        &AtomicBool::new(true),
        &mut |_| {},
    )
    .unwrap_err();
    assert!(error.is_cancelled(), "{}", error.code);
}

/// A game folder and data folder for the Unity fixture, whose loader package installs into the game folder itself.
fn unity_fixture(
    name: &str,
) -> (
    std::path::PathBuf,
    std::path::PathBuf,
    Data,
    crate::games::GameDef,
) {
    let base = scratch!(name);
    let game_root = base.join("game");
    fs::create_dir_all(&game_root).unwrap();
    let data = Data::new(base.join("data"));
    (base, game_root, data, test_games::unity())
}

/// A loader build as a mod site publishes it: wrapped in a folder, with a bundled plugin and the site's own files.
fn site_build(base: &std::path::Path) -> std::path::PathBuf {
    make_zip(
        base,
        "Owner-LoaderPack-9.9.9.zip",
        &[
            ("LoaderPack/winhttp.dll", "site proxy"),
            ("LoaderPack/doorstop_config.ini", "settings"),
            ("LoaderPack/BepInEx/core/BepInEx.dll", "site core"),
            ("LoaderPack/BepInEx/plugins/Fix/Fix.dll", "bundled plugin"),
            ("manifest.json", "{}"),
        ],
    )
}

fn official_release(base: &std::path::Path) -> (Vec<u8>, String) {
    let zip = make_zip(
        base,
        "official.zip",
        &[
            ("winhttp.dll", "proxy"),
            ("doorstop_config.ini", "settings"),
            ("BepInEx/core/BepInEx.dll", "core"),
        ],
    );
    let hash = digest_file::<Sha256>(&zip, &mut |_| {}).unwrap();
    (fs::read(&zip).unwrap(), hash)
}

#[test]
fn a_loader_build_with_a_plugin_keeps_the_game_rules_and_counts_as_the_loader() {
    let (base, game_root, data, game) = unity_fixture("loader-site-build-paths");
    let zip = site_build(&base);
    let view = crate::test_support::import_from(
        &data,
        &game.id,
        &zip,
        None,
        Some(crate::domain::source::SourceRef::new(
            "thunderstore",
            "Owner-LoaderPack",
        )),
        &mut |_, _| {},
    )
    .unwrap();
    assert_eq!(view.folders.len(), 1);
    let library = crate::context::load_library(&data.game(&game.id), &game).unwrap();
    let copy = &library.versions[&view.folders[0]];
    // The wrapper folder is gone, the proxy and core keep their game paths and the plugin is in the plugin slot.
    assert_eq!(copy.slot, "BepInEx/plugins");
    for file in [
        "winhttp.dll",
        "doorstop_config.ini",
        "BepInEx/core/BepInEx.dll",
    ] {
        assert!(
            copy.files.iter().any(|path| path == file),
            "{file}: {:?}",
            copy.files
        );
    }
    assert!(
        !copy
            .files
            .iter()
            .any(|path| path.ends_with("manifest.json"))
    );
    let setup = view_of(&data, &game, &game_root);
    assert_eq!(setup.copies.len(), 1);
}

fn view_of(
    data: &Data,
    game: &crate::games::GameDef,
    game_root: &std::path::Path,
) -> super::LoaderSetupView {
    view(data, game, &settings(game_root)).unwrap().unwrap()
}

#[test]
fn the_user_picks_between_a_build_from_a_mod_site_and_the_official_release() {
    let (base, game_root, data, mut game) = unity_fixture("loader-site-or-official");
    let settings = settings(&game_root);
    let site = crate::test_support::import_from(
        &data,
        &game.id,
        &site_build(&base),
        None,
        Some(crate::domain::source::SourceRef::new(
            "thunderstore",
            "Owner-LoaderPack",
        )),
        &mut |_, _| {},
    )
    .unwrap()
    .folders
    .remove(0);

    let before = view(&data, &game, &settings).unwrap().unwrap();
    assert_eq!(before.state, LoaderSetupState::Missing);
    assert_eq!(before.target, "");
    assert_eq!(before.copies.len(), 1);
    assert!(!before.copies[0].official && !before.copies[0].enabled);

    // The quick action turns on the copy in the library: no server answers, so a download would fail.
    point_at(
        &mut game,
        "https://127.0.0.1:9/unused.zip".into(),
        "0".repeat(64),
        None,
    );
    let after = install(
        &data,
        &game,
        &settings,
        &LoaderChoice::Auto,
        &not_cancelled(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(
        after.state,
        LoaderSetupState::Installed,
        "a site build is never outdated"
    );
    assert_eq!(
        enabled_folders(&data, &game.id),
        std::slice::from_ref(&site)
    );

    // The official release is one click away, and it replaces the site build in the profile.
    let (bytes, hash) = official_release(&base);
    let (url, server) = serve(bytes);
    point_at(&mut game, url, hash, None);
    let official = install(
        &data,
        &game,
        &settings,
        &LoaderChoice::Official,
        &not_cancelled(),
        &mut |_| {},
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(official.copies.len(), 2);
    let official_copy = official.copies.iter().find(|copy| copy.official).unwrap();
    assert!(official_copy.enabled);
    assert_eq!(
        enabled_folders(&data, &game.id),
        std::slice::from_ref(&official_copy.folder)
    );

    // And back: the site build again, with the official copy turned off and kept in the library.
    let back = install(
        &data,
        &game,
        &settings,
        &LoaderChoice::Library {
            folder: site.clone(),
        },
        &not_cancelled(),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(back.copies.len(), 2);
    assert_eq!(enabled_folders(&data, &game.id), [site]);
    assert!(
        !game_root.join("winhttp.dll").exists(),
        "the game folder must stay untouched"
    );
}

#[test]
fn an_unknown_library_copy_is_refused() {
    let (_base, game_root, data, game) = unity_fixture("loader-unknown-copy");
    let error = install(
        &data,
        &game,
        &settings(&game_root),
        &LoaderChoice::Library {
            folder: "missing".into(),
        },
        &not_cancelled(),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(error.code, "loader.copy_unknown");
}
