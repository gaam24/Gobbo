//! The catalog tests use a local TCP server that answers a fixed sequence of responses, so a checksum mismatch is
//! reproducible without touching GitHub. All folder seams are scratch folders.

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::{
    CatalogInstallOutcome, CatalogInstallResult, Folders, check_from, drop_stale_from, install_from,
};
use crate::error::{AppError, ErrorKind as FailureKind};
use crate::games::GameOrigin;
use crate::test_support::games::{LOADED, LOADER};
use crate::test_support::scratch;

/// A valid, generic definition with the given id. It has one mirror so it passes the deploy rule.
fn game_text(id: &str) -> String {
    format!(
        "
schema = 1
id = \"{id}\"
name = \"{id}\"

[detect]
markers = [\"game.exe\"]

[[executables]]
name = \"Game\"
path = \"game.exe\"

[deploy]

[[deploy.mirror]]
dir = \"Mods\"
"
    )
}

/// The SHA-256 of a test string, in the same lowercase hex form the catalog uses.
fn sha(text: &str) -> String {
    use sha2::{Digest, Sha256};

    faster_hex::hex_string(&Sha256::digest(text.as_bytes()))
}

/// A file and its parent folders, for local state the check reads.
fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// The scratch user and bundled game folders the check reads, outside the install's folder set.
fn user_games(root: &Path) -> PathBuf {
    root.join("user/games")
}

/// The scratch bundled game folder the check reads.
fn bundled_games(root: &Path) -> PathBuf {
    root.join("bundled/games")
}

/// The scratch folders in the user / official / bundled precedence, all created up front.
fn folders(root: &Path) -> Folders {
    let folders = Folders {
        catalog_games: Some(root.join("catalog/games")),
        user_loaders: Some(root.join("user/loaders")),
        catalog_loaders: Some(root.join("catalog/loaders")),
        bundled_loaders: root.join("bundled/loaders"),
    };

    for dir in [
        Some(user_games(root).as_path()),
        folders.catalog_games.as_deref(),
        Some(bundled_games(root).as_path()),
        folders.user_loaders.as_deref(),
        folders.catalog_loaders.as_deref(),
        Some(folders.bundled_loaders.as_path()),
    ]
    .into_iter()
    .flatten()
    {
        fs::create_dir_all(dir).unwrap();
    }
    folders
}

/// A game entry, with any extra fields the test needs overlaid on the common shape.
fn game_entry(id: &str, file: &str, sha256: &str, extra: Value) -> Value {
    let mut entry = json!({
        "id": id,
        "name": id,
        "schema": 1,
        "loader": null,
        "verified": false,
        "file": file,
        "sha256": sha256,
        "icon": null,
        "iconSha256": null,
    });
    if let Some(map) = extra.as_object() {
        for (key, value) in map {
            entry[key.as_str()] = value.clone();
        }
    }
    entry
}

/// A loader entry in the manifest.
fn loader_entry(id: &str, file: &str, sha256: &str) -> Value {
    json!({
        "id": id,
        "name": id,
        "schema": 1,
        "file": file,
        "sha256": sha256,
    })
}

/// A whole manifest body with the given API version.
fn manifest(games: &[Value], loaders: &[Value], api_version: u32) -> String {
    json!({
        "apiVersion": api_version,
        "games": games,
        "loaders": loaders,
    })
    .to_string()
}

/// One HTTP answer with a content length.
fn ok(extra_headers: &str, body: &[u8]) -> Vec<u8> {
    let mut answer = format!(
        "HTTP/1.1 200 OK\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    answer.extend_from_slice(body);
    answer
}

/// A local server that answers one request per fixed response, in order. The deadline keeps a wrong request count
/// from hanging the test; the browser agent and the download agent connect in the order the install makes its calls.
fn serve(answers: Vec<Vec<u8>>) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        for answer in answers {
            loop {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                        let mut request = [0_u8; 8192];
                        let _ = stream.read(&mut request);
                        stream.write_all(&answer).unwrap();
                        let _ = stream.flush();
                        break;
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => {
                        if Instant::now() > deadline {
                            return;
                        }
                        thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => return,
                }
            }
        }
    });
    (format!("http://{address}"), handle)
}

/// Whether an outcome reports an installed id.
fn installed(outcomes: &[CatalogInstallOutcome], id: &str) -> bool {
    outcomes.iter().any(|outcome| {
        outcome.id == id && matches!(outcome.result, CatalogInstallResult::Installed)
    })
}

/// The error of a failed outcome, for asserting the stable code and its parameters.
fn failure(outcomes: &[CatalogInstallOutcome], id: &str) -> AppError {
    outcomes
        .iter()
        .find(|outcome| outcome.id == id)
        .and_then(|outcome| match &outcome.result {
            CatalogInstallResult::Failed { error } => Some(error.clone()),
            CatalogInstallResult::Installed => None,
        })
        .unwrap_or_else(|| panic!("no failed outcome for {id}"))
}

/// The ids in one classification list, sorted for a stable assertion.
fn ids(entries: &[super::CatalogEntryView]) -> Vec<&str> {
    let mut ids: Vec<&str> = entries.iter().map(|entry| entry.id.as_str()).collect();
    ids.sort_unstable();
    ids
}

#[test]
fn a_check_classifies_changed_available_and_blocked_entries() {
    let root = scratch!("catalog-check");
    let folders = folders(&root);
    let catalog = folders.catalog_games.as_deref().unwrap();
    let user = user_games(&root);
    let bundled = bundled_games(&root);

    let same = game_text("same-game");
    let changed = game_text("changed-game");
    let owned = game_text("user-game");
    write(&catalog.join("changed-game/game.toml"), &changed);
    write(&bundled.join("same-game/game.toml"), &same);
    write(&user.join("user-game/game.toml"), &owned);

    let games = vec![
        game_entry(
            "changed-game",
            "games/changed-game/game.toml",
            &sha("a different definition"),
            json!({}),
        ),
        game_entry(
            "same-game",
            "games/same-game/game.toml",
            &sha(&same),
            json!({}),
        ),
        game_entry(
            "available-game",
            "games/available-game/game.toml",
            &sha("anything"),
            json!({}),
        ),
        game_entry(
            "user-game",
            "games/user-game/game.toml",
            &sha("not what the user wrote"),
            json!({}),
        ),
        game_entry(
            "old-app-game",
            "games/old-app-game/game.toml",
            &sha("anything"),
            json!({"minGobbo": "999.0.0"}),
        ),
        game_entry(
            "old-loader-game",
            "games/old-loader-game/game.toml",
            &sha("anything"),
            json!({"loader": "future-loader"}),
        ),
        game_entry(
            "new-schema-game",
            "games/new-schema-game/game.toml",
            &sha("anything"),
            json!({"schema": 2}),
        ),
    ];
    let mut future_loader = loader_entry("future-loader", "loaders/future-loader.toml", &sha("x"));
    future_loader["minGobbo"] = json!("998.0.0");
    let body = manifest(&games, &[future_loader], 1);
    let (base, server) = serve(vec![ok(
        "Content-Type: application/json\r\n",
        body.as_bytes(),
    )]);
    let status = check_from(
        &format!("{base}/catalog.json"),
        Some(catalog),
        Some(&user),
        &bundled,
    )
    .unwrap();
    server.join().unwrap();

    assert_eq!(ids(&status.changed), ["changed-game"]);
    assert_eq!(ids(&status.available), ["available-game"]);
    assert_eq!(
        ids(&status.blocked),
        ["new-schema-game", "old-app-game", "old-loader-game"]
    );
    let floor = |id: &str| {
        status
            .blocked
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.min_gobbo.clone())
    };
    assert_eq!(floor("old-app-game").as_deref(), Some("999.0.0"));
    assert_eq!(floor("old-loader-game").as_deref(), Some("998.0.0"));
    assert!(!ids(&status.changed).contains(&"user-game"));
    assert!(!ids(&status.available).contains(&"user-game"));
}

#[test]
fn a_newer_api_version_is_unsupported_and_a_broken_fetch_is_retryable() {
    let body = manifest(&[], &[], 2);
    let (base, server) = serve(vec![ok(
        "Content-Type: application/json\r\n",
        body.as_bytes(),
    )]);
    let error = check_from(
        &format!("{base}/catalog.json"),
        None,
        None,
        Path::new("/unused"),
    )
    .unwrap_err();
    server.join().unwrap();
    assert_eq!(error.code, "catalog.unsupported");
    assert_eq!(error.kind, FailureKind::Permanent);

    let answer =
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .to_vec();
    let (base, server) = serve(vec![answer]);
    let error = check_from(
        &format!("{base}/catalog.json"),
        None,
        None,
        Path::new("/unused"),
    )
    .unwrap_err();
    server.join().unwrap();
    assert_eq!(error.code, "catalog.fetch");
    assert_eq!(error.kind, FailureKind::Retryable);
}

#[test]
fn a_checksum_mismatch_fails_only_that_id() {
    let root = scratch!("catalog-mismatch");
    let folders = folders(&root);
    let good = game_text("good-game");
    let games = vec![
        game_entry(
            "changed-game",
            "games/changed-game/game.toml",
            &sha("what the file should be"),
            json!({}),
        ),
        game_entry(
            "good-game",
            "games/good-game/game.toml",
            &sha(&good),
            json!({}),
        ),
    ];
    let body = manifest(&games, &[], 1);
    let (base, server) = serve(vec![
        ok("Content-Type: application/json\r\n", body.as_bytes()),
        ok("", b"a file that is newer than the manifest"),
        ok("", good.as_bytes()),
    ]);
    let ids = vec!["changed-game".to_string(), "good-game".to_string()];
    let outcomes = install_from(
        &format!("{base}/catalog.json"),
        &format!("{base}/"),
        &folders,
        &ids,
        &mut |_| {},
    )
    .unwrap();
    server.join().unwrap();

    let error = failure(&outcomes, "changed-game");
    assert_eq!(error.code, "catalog.changed");
    assert_eq!(error.kind, FailureKind::Retryable);
    assert!(installed(&outcomes, "good-game"));
    let catalog = folders.catalog_games.unwrap();
    assert!(!catalog.join("changed-game").exists());
    assert!(catalog.join("good-game/game.toml").is_file());
}

#[test]
fn an_invalid_definition_is_refused_and_the_other_ids_still_install() {
    let root = scratch!("catalog-invalid");
    let folders = folders(&root);
    let good = game_text("good-game");
    let broken = "not a definition";
    let games = vec![
        game_entry(
            "bad-game",
            "games/bad-game/game.toml",
            &sha(broken),
            json!({}),
        ),
        game_entry(
            "good-game",
            "games/good-game/game.toml",
            &sha(&good),
            json!({}),
        ),
    ];
    let body = manifest(&games, &[], 1);
    let (base, server) = serve(vec![
        ok("Content-Type: application/json\r\n", body.as_bytes()),
        ok("", broken.as_bytes()),
        ok("", good.as_bytes()),
    ]);
    let ids = vec!["bad-game".to_string(), "good-game".to_string()];
    let outcomes = install_from(
        &format!("{base}/catalog.json"),
        &format!("{base}/"),
        &folders,
        &ids,
        &mut |_| {},
    )
    .unwrap();
    server.join().unwrap();

    let error = failure(&outcomes, "bad-game");
    assert_eq!(error.code, "catalog.invalid");
    assert_eq!(error.kind, FailureKind::UserAction);
    assert!(error.params.contains_key("file"));
    assert!(error.params.contains_key("detail"));
    assert!(installed(&outcomes, "good-game"));

    let catalog = folders.catalog_games.unwrap();
    assert!(!catalog.join("bad-game").exists());
    assert_eq!(
        fs::read_to_string(catalog.join("good-game/game.toml")).unwrap(),
        good
    );
}

#[test]
fn an_installed_definition_loads_with_the_official_origin() {
    let root = scratch!("catalog-official");
    let folders = folders(&root);
    let text = game_text("sample-game");
    let body = manifest(
        &[game_entry(
            "sample-game",
            "games/sample-game/game.toml",
            &sha(&text),
            json!({}),
        )],
        &[],
        1,
    );
    let (base, server) = serve(vec![
        ok("Content-Type: application/json\r\n", body.as_bytes()),
        ok("", text.as_bytes()),
    ]);
    let ids = vec!["sample-game".to_string()];
    let outcomes = install_from(
        &format!("{base}/catalog.json"),
        &format!("{base}/"),
        &folders,
        &ids,
        &mut |_| {},
    )
    .unwrap();
    server.join().unwrap();
    assert!(installed(&outcomes, "sample-game"));

    let (definitions, problems) = crate::games::definitions_from(
        &bundled_games(&root),
        folders.catalog_games.as_deref(),
        Some(&user_games(&root)),
        &folders.bundled_loaders,
    )
    .unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    let (game, origin) = definitions
        .iter()
        .find(|(game, _)| game.id == "sample-game")
        .expect("the installed definition is listed");
    assert_eq!(game.name, "sample-game");
    assert_eq!(*origin, GameOrigin::Official);
}

#[test]
fn a_missing_loader_is_co_installed_with_the_game() {
    let root = scratch!("catalog-loader");
    let folders = folders(&root);
    let body = manifest(
        &[game_entry(
            "loaded",
            "games/loaded/game.toml",
            &sha(LOADED),
            json!({"loader": "fixture-loader"}),
        )],
        &[loader_entry(
            "fixture-loader",
            "loaders/fixture-loader.toml",
            &sha(LOADER),
        )],
        1,
    );
    let (base, server) = serve(vec![
        ok("Content-Type: application/json\r\n", body.as_bytes()),
        ok("", LOADED.as_bytes()),
        ok("", LOADER.as_bytes()),
    ]);
    let ids = vec!["loaded".to_string()];
    let outcomes = install_from(
        &format!("{base}/catalog.json"),
        &format!("{base}/"),
        &folders,
        &ids,
        &mut |_| {},
    )
    .unwrap();
    server.join().unwrap();

    assert!(installed(&outcomes, "loaded"));
    let loader = folders.catalog_loaders.unwrap().join("fixture-loader.toml");
    assert_eq!(
        fs::read_to_string(loader).unwrap(),
        crate::test_support::games::lf(LOADER)
    );
}

#[test]
fn a_changed_local_loader_is_replaced_by_the_catalog_copy() {
    let root = scratch!("catalog-loader-changed");
    let folders = folders(&root);
    write(
        &folders.bundled_loaders.join("fixture-loader.toml"),
        "a stale loader preset",
    );
    let body = manifest(
        &[game_entry(
            "loaded",
            "games/loaded/game.toml",
            &sha(LOADED),
            json!({"loader": "fixture-loader"}),
        )],
        &[loader_entry(
            "fixture-loader",
            "loaders/fixture-loader.toml",
            &sha(LOADER),
        )],
        1,
    );
    let (base, server) = serve(vec![
        ok("Content-Type: application/json\r\n", body.as_bytes()),
        ok("", LOADED.as_bytes()),
        ok("", LOADER.as_bytes()),
    ]);
    let ids = vec!["loaded".to_string()];
    let outcomes = install_from(
        &format!("{base}/catalog.json"),
        &format!("{base}/"),
        &folders,
        &ids,
        &mut |_| {},
    )
    .unwrap();
    server.join().unwrap();

    assert!(installed(&outcomes, "loaded"));
    let loader = folders.catalog_loaders.unwrap().join("fixture-loader.toml");
    assert_eq!(
        fs::read_to_string(loader).unwrap(),
        crate::test_support::games::lf(LOADER)
    );
}

#[test]
fn an_icon_is_installed_beside_the_game() {
    let root = scratch!("catalog-icon");
    let folders = folders(&root);
    let text = game_text("sample-game");
    let icon = "a tiny webp stand-in";
    let body = manifest(
        &[game_entry(
            "sample-game",
            "games/sample-game/game.toml",
            &sha(&text),
            json!({
                "icon": "games/sample-game/icon.webp",
                "iconSha256": sha(icon),
            }),
        )],
        &[],
        1,
    );
    let (base, server) = serve(vec![
        ok("Content-Type: application/json\r\n", body.as_bytes()),
        ok("", text.as_bytes()),
        ok("", icon.as_bytes()),
    ]);
    let ids = vec!["sample-game".to_string()];
    let outcomes = install_from(
        &format!("{base}/catalog.json"),
        &format!("{base}/"),
        &folders,
        &ids,
        &mut |_| {},
    )
    .unwrap();
    server.join().unwrap();

    assert!(installed(&outcomes, "sample-game"));
    let catalog = folders.catalog_games.unwrap();
    assert_eq!(
        fs::read_to_string(catalog.join("sample-game/icon.webp")).unwrap(),
        icon
    );
}

#[test]
fn an_unknown_id_is_refused_for_that_id_only() {
    let root = scratch!("catalog-unknown");
    let folders = folders(&root);
    let body = manifest(&[], &[], 1);
    let (base, server) = serve(vec![ok(
        "Content-Type: application/json\r\n",
        body.as_bytes(),
    )]);
    let ids = vec!["not-in-catalog".to_string()];
    let outcomes = install_from(
        &format!("{base}/catalog.json"),
        &format!("{base}/"),
        &folders,
        &ids,
        &mut |_| {},
    )
    .unwrap();
    server.join().unwrap();

    let error = failure(&outcomes, "not-in-catalog");
    assert_eq!(error.code, "catalog.unknown");
    assert_eq!(error.kind, FailureKind::UserAction);
}

#[test]
fn an_app_update_drops_official_copies_that_shadow_bundled_files() {
    let root = scratch!("catalog-stale");
    let folders = folders(&root);
    let catalog_games = folders.catalog_games.as_deref().unwrap();
    let catalog_loaders = folders.catalog_loaders.as_deref().unwrap();
    let bundled = bundled_games(&root);

    write(
        &bundled.join("shipped-game/game.toml"),
        &game_text("shipped-game"),
    );
    write(&folders.bundled_loaders.join("shipped-loader.toml"), LOADER);
    write(
        &catalog_games.join("shipped-game/game.toml"),
        "an older download",
    );
    write(
        &catalog_games.join("shipped-game/icon.webp"),
        "an older icon",
    );
    write(
        &catalog_games.join("new-game/game.toml"),
        &game_text("new-game"),
    );
    write(
        &catalog_loaders.join("shipped-loader.toml"),
        "an older loader",
    );
    write(&catalog_loaders.join("new-loader.toml"), LOADER);
    write(&root.join("catalog/app-version"), "1.0.0");

    let dropped = drop_stale_from(
        catalog_games,
        catalog_loaders,
        &bundled,
        &folders.bundled_loaders,
        "1.0.0",
    )
    .unwrap();
    assert_eq!(dropped, 0, "the same app version keeps every copy");
    assert!(catalog_games.join("shipped-game").exists());

    let dropped = drop_stale_from(
        catalog_games,
        catalog_loaders,
        &bundled,
        &folders.bundled_loaders,
        "1.1.0",
    )
    .unwrap();
    assert_eq!(dropped, 2);
    assert!(!catalog_games.join("shipped-game").exists());
    assert!(!catalog_loaders.join("shipped-loader.toml").exists());
    assert!(catalog_games.join("new-game/game.toml").is_file());
    assert!(catalog_loaders.join("new-loader.toml").is_file());
    assert_eq!(
        fs::read_to_string(root.join("catalog/app-version")).unwrap(),
        "1.1.0"
    );
}

#[test]
fn a_catalog_without_a_version_file_is_left_alone() {
    let root = scratch!("catalog-no-version");
    let folders = folders(&root);
    let catalog_games = folders.catalog_games.as_deref().unwrap();
    let bundled = bundled_games(&root);
    write(
        &bundled.join("shipped-game/game.toml"),
        &game_text("shipped-game"),
    );
    write(&catalog_games.join("shipped-game/game.toml"), "a download");

    let dropped = drop_stale_from(
        catalog_games,
        folders.catalog_loaders.as_deref().unwrap(),
        &bundled,
        &folders.bundled_loaders,
        "1.1.0",
    )
    .unwrap();
    assert_eq!(dropped, 0);
    assert!(catalog_games.join("shipped-game/game.toml").is_file());
}

#[test]
fn an_install_records_the_app_version() {
    let root = scratch!("catalog-version-mark");
    let folders = folders(&root);
    let text = game_text("sample-game");
    let body = manifest(
        &[game_entry(
            "sample-game",
            "games/sample-game/game.toml",
            &sha(&text),
            json!({}),
        )],
        &[],
        1,
    );
    let (base, server) = serve(vec![
        ok("Content-Type: application/json\r\n", body.as_bytes()),
        ok("", text.as_bytes()),
    ]);
    let ids = vec!["sample-game".to_string()];
    install_from(
        &format!("{base}/catalog.json"),
        &format!("{base}/"),
        &folders,
        &ids,
        &mut |_| {},
    )
    .unwrap();
    server.join().unwrap();

    assert_eq!(
        fs::read_to_string(root.join("catalog/app-version")).unwrap(),
        env!("CARGO_PKG_VERSION")
    );
}
