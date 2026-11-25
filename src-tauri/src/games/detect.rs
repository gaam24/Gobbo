//! Searching for games: every store's installs of a game (Steam first, then the stores in `stores`), checked with the
//! marker rule, plus the store that claims a folder the user picks. Read-only: it writes nothing.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;

use super::GameDef;
use super::stores::steam::{find_all, libraries, library_of};

/// A game found in a Steam library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct Found {
    /// The game folder, for example `.../steamapps/common/The Witcher 3`.
    pub game_root: String,
    /// The store folder the game came from: the Steam library, or the folder a store names.
    pub library: String,
    /// The store that named the folder: `steam`, `gog`, `epic`, `heroic`, `legendary` or `lutris`.
    pub store: String,
}

/// The result of searching for one supported game. `found` is the preferred install; `others` are the other installs of
/// the same game the scan saw.
#[derive(Debug, Serialize, Type)]
pub struct GameScan {
    /// The game's id from its definition.
    pub id: String,
    /// The preferred install, or empty when the game is nowhere.
    pub found: Option<Found>,
    /// The other installs, Steam first then store order; a folder claimed twice appears once.
    pub others: Vec<Found>,
}

/// Whether `root` is the game's folder on this platform. Windows checks `markers`; Linux also accepts a folder with
/// the files from `linux_markers`, so a native-only install is found.
pub fn has_markers(root: &Path, game: &GameDef) -> bool {
    has_markers_for(!crate::platform::is_windows(), root, game)
}

/// The game folder, or the only immediate child that has the markers. A pre-installed game often keeps the real folder
/// one level down (`Game/Game`); two matching children are ambiguous and give `None` when the parent does not match.
pub fn game_folder(root: &Path, game: &GameDef, is_linux: bool) -> Option<PathBuf> {
    if has_markers_for(is_linux, root, game) {
        return Some(root.to_path_buf());
    }

    let mut children = fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|child| child.is_dir() && has_markers_for(is_linux, child, game));
    let first = children.next()?;
    children.next().is_none().then_some(first)
}

/// The marker rule with its platform given, so both sides can be tested on one machine. A folder qualifies when all
/// `markers` exist or, on Linux, when all `linux_markers` exist. `any_marker` relaxes each set the same way; with no
/// markers the folder itself is enough, and an empty `linux_markers` leaves the rule as it was.
pub fn has_markers_for(is_linux: bool, root: &Path, game: &GameDef) -> bool {
    root.is_dir() && satisfied_set(is_linux, root, game).is_some()
}

/// The marker set the folder satisfies under the rule of `has_markers_for`: the definition's `markers` first, then, on
/// Linux, its `linux_markers`. `None` when neither is satisfied. Every reader of the rule goes through here, so the
/// folder that is accepted and the marker that is shown cannot disagree.
fn satisfied_set<'a>(is_linux: bool, root: &Path, game: &'a GameDef) -> Option<&'a [String]> {
    /// Whether one set of markers is satisfied. An empty set needs no file, which keeps the folder itself enough.
    fn set_matches(markers: &[String], any: bool, root: &Path) -> bool {
        markers.is_empty()
            || if any {
                markers.iter().any(|marker| root.join(marker).exists())
            } else {
                markers.iter().all(|marker| root.join(marker).exists())
            }
    }

    let any = game.detect.any_marker;
    let markers = game.detect.markers.as_slice();
    let linux = game.detect.linux_markers.as_slice();

    if set_matches(markers, any, root) {
        Some(markers)
    } else if is_linux && !linux.is_empty() && set_matches(linux, any, root) {
        Some(linux)
    } else {
        None
    }
}

/// The marker file to show for a folder the rule accepts: the first file of the satisfied set that the folder has.
/// `None` when the folder is not accepted, or when the satisfied set names no file.
pub fn marker_file_for(is_linux: bool, root: &Path, game: &GameDef) -> Option<String> {
    if !root.is_dir() {
        return None;
    }

    satisfied_set(is_linux, root, game)?
        .iter()
        .find(|marker| root.join(marker).exists())
        .cloned()
}

/// The marker files a game folder of this game is asked to have on this platform, as a message shows them: the
/// definition's `markers`, and on Linux its `linux_markers` after ` /`, because either set is enough.
pub fn expected_markers(is_linux: bool, game: &GameDef) -> String {
    let mut sets = vec![game.detect.markers.join(", ")];

    if is_linux {
        sets.push(game.detect.linux_markers.join(", "));
    }

    sets.retain(|set| !set.is_empty());
    sets.join(" / ")
}

/// Whether two paths name the same folder. Links are resolved first, so a store's path and the game folder it points at
/// compare equal; paths that cannot be resolved are compared as they are.
pub fn same_folder(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

/// The store an install folder belongs to, and the Wine prefix that store runs it in when it names one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    /// `steam`, `gog`, `epic`, `heroic`, `legendary`, `lutris`, or `manual` when no store claims the folder.
    pub store: &'static str,
    /// The store's prefix for the game (Heroic and Lutris name one).
    pub prefix: Option<PathBuf>,
}

impl Install {
    /// The prefix as the settings keep it: the path, or empty for none.
    pub fn prefix_text(&self) -> String {
        self.prefix
            .as_ref()
            .map(|prefix| prefix.display().to_string())
            .unwrap_or_default()
    }
}

/// The store that claims an install folder. Missing or unreadable store files simply do not match.
pub fn classify_install(home: &Path, game_root: &Path) -> Install {
    classify_in(game_root, &super::stores::store_folders(home))
}

/// `classify_install` with the stores' folders given, so a test can use scratch folders.
pub fn classify_in(game_root: &Path, folders: &crate::platform::StoreFolders) -> Install {
    let install = |store, prefix| Install { store, prefix };

    if library_of(game_root).is_some() {
        return install("steam", None);
    }

    // A launcher that manages the folder wins over a plain `goggame-*.info` file: Heroic's GOG installs keep such a
    // file too, but the launcher that starts the game owns the install. The candidates come in that order.
    let resolved = fs::canonicalize(game_root).ok();
    let is_root = |root: &Path| match (&resolved, fs::canonicalize(root)) {
        (Some(resolved), Ok(root)) => *resolved == root,
        _ => root == game_root,
    };

    if let Some(candidate) = super::stores::candidates_in(folders)
        .into_iter()
        .find(|candidate| is_root(&candidate.root))
    {
        return install(candidate.store.name(), candidate.prefix);
    }

    if !super::stores::gog::product_ids(game_root).is_empty() {
        return install("gog", None);
    }

    install("manual", None)
}

/// Looks for every game in `games` in the Steam libraries and in the other stores of this computer.
pub fn scan(home: &Path, games: &[GameDef]) -> Vec<GameScan> {
    scan_in(&libraries(home), games, &super::stores::store_folders(home))
}

/// `scan` with the Steam libraries and the stores' folders given, so a test can use scratch folders on any platform.
pub fn scan_in(
    libraries: &[PathBuf],
    games: &[GameDef],
    folders: &crate::platform::StoreFolders,
) -> Vec<GameScan> {
    let candidates = super::stores::candidates_in(folders);

    games
        .iter()
        .map(|game| {
            let mut installs = Vec::new();
            let mut claimed: Vec<PathBuf> = Vec::new();

            for hit in find_all(game, libraries) {
                push_install(&mut installs, &mut claimed, hit);
            }

            for candidate in &candidates {
                if !super::stores::matches(game, candidate) {
                    continue;
                }

                push_install(
                    &mut installs,
                    &mut claimed,
                    Found {
                        game_root: candidate.root.display().to_string(),
                        library: candidate
                            .root
                            .parent()
                            .map_or_else(String::new, |parent| parent.display().to_string()),
                        store: candidate.store.name().to_string(),
                    },
                );
            }

            let mut installs = installs.into_iter();
            GameScan {
                id: game.id.clone(),
                found: installs.next(),
                others: installs.collect(),
            }
        })
        .collect()
}

/// Adds a hit unless its folder was already claimed, so one install appears once under the store that named it first.
fn push_install(installs: &mut Vec<Found>, claimed: &mut Vec<PathBuf>, hit: Found) {
    let root = PathBuf::from(&hit.game_root);

    if claimed.iter().any(|other| same_folder(other, &root)) {
        return;
    }

    claimed.push(root);
    installs.push(hit);
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::games::stores::steam::tests::{install_slot, steam_home};
    use crate::games::stores::steam::{find, libraries_in};
    use crate::test_support::games as test_games;
    use crate::test_support::{scratch, write};

    #[test]
    fn a_folder_without_the_game_files_is_not_a_match() {
        let home = scratch!("markers");
        let steam = steam_home(&home, &[]);
        fs::create_dir_all(steam.join("steamapps/common/Slot Game")).unwrap();

        assert_eq!(find(&test_games::slot(), &libraries_in(&[steam])), None);
    }

    #[test]
    fn one_marker_is_enough_when_the_definition_allows_any() {
        let base = scratch!("markers-any");
        let root = base.join("Game");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Game.x86_64"), b"").unwrap();
        let mut game = test_games::slot();
        game.detect.markers = vec!["Game.exe".to_string(), "Game.x86_64".to_string()];

        assert!(!has_markers(&root, &game));
        game.detect.any_marker = true;
        assert!(has_markers(&root, &game));
        game.detect.markers.clear();
        game.detect.any_marker = false;
        assert!(has_markers(&root, &game));
    }

    /// A fixture game whose two builds name their program differently.
    fn native_game() -> GameDef {
        let mut game = test_games::slot();
        game.detect.markers = vec!["Game.exe".to_string()];
        game.detect.linux_markers = vec!["Game.x86_64".to_string()];
        game.detect.steam_install_dirs = vec!["Native Game".to_string()];
        game.detect.steam_app_ids.clear();
        game
    }

    #[test]
    fn the_marker_rule_uses_each_platform_set_without_leaking_it_to_the_other() {
        let base = scratch!("linux-markers-rule");
        let windows = base.join("Windows Game");
        let linux = base.join("Native Game");
        let both = base.join("Both Builds");
        let empty = base.join("Empty Game");
        for root in [&windows, &linux, &both, &empty] {
            fs::create_dir_all(root).unwrap();
        }
        write(&windows.join("Game.exe"), "");
        write(&linux.join("Game.x86_64"), "");
        write(&both.join("Game.exe"), "");
        write(&both.join("Game.x86_64"), "");

        let game = native_game();
        assert!(has_markers_for(false, &windows, &game));
        assert!(!has_markers_for(false, &linux, &game));
        assert!(has_markers_for(true, &linux, &game));
        assert!(has_markers_for(true, &windows, &game));
        assert!(has_markers_for(true, &both, &game));
        assert!(!has_markers_for(true, &empty, &game));
        assert!(!has_markers_for(false, &empty, &game));
    }

    #[test]
    fn no_linux_markers_leaves_the_rule_as_it_was() {
        let base = scratch!("linux-markers-empty");
        let root = base.join("Game");
        fs::create_dir_all(&root).unwrap();
        let mut game = native_game();
        game.detect.linux_markers.clear();

        assert!(!has_markers_for(true, &root, &game));
        assert!(!has_markers_for(false, &root, &game));
        write(&root.join("Game.exe"), "");
        assert!(has_markers_for(true, &root, &game));
        assert!(has_markers_for(false, &root, &game));
        game.detect.markers.clear();
        assert!(has_markers_for(true, &root, &game));
        assert!(has_markers_for(false, &root, &game));
    }

    #[test]
    fn any_marker_relaxes_each_platform_set_on_its_own() {
        let base = scratch!("linux-markers-any");
        let root = base.join("Game");
        fs::create_dir_all(&root).unwrap();
        write(&root.join("Game.x86_64"), "");
        let mut game = native_game();
        game.detect.linux_markers = vec!["Game.x86_64".into(), "Game2.x86_64".into()];

        assert!(!has_markers_for(true, &root, &game));
        game.detect.any_marker = true;
        assert!(has_markers_for(true, &root, &game));
        assert!(!has_markers_for(false, &root, &game));

        write(&root.join("Game.exe"), "");
        game.detect.linux_markers.clear();
        game.detect.markers = vec!["Game.exe".into(), "Game2.exe".into()];
        game.detect.any_marker = false;
        assert!(!has_markers_for(false, &root, &game));
        game.detect.any_marker = true;
        assert!(has_markers_for(false, &root, &game));
    }

    #[test]
    fn the_scan_finds_a_native_install_through_its_linux_marker() {
        let home = scratch!("scan-native");
        let steam = steam_home(&home, &[]);
        let root = steam.join("steamapps/common/Native Game");
        fs::create_dir_all(&root).unwrap();
        write(&root.join("Game.x86_64"), "");

        let found = find(&native_game(), &libraries_in(&[steam]));
        assert_eq!(found.is_some(), cfg!(not(windows)));
        if let Some(found) = found {
            assert_eq!(found.game_root, root.display().to_string());
            assert_eq!(found.store, "steam");
        }
    }

    #[test]
    fn the_order_file_folder_is_found_in_the_proton_prefix_once_it_exists() {
        let home = scratch!("proton");
        let steam = steam_home(&home, &[]);
        let root = install_slot(&steam, "Slot Game");
        let game = test_games::slot();
        let find = || {
            game.find_order_dir(
                &root,
                &home,
                super::super::stores::steam::proton_prefix(&game, &root),
                true,
            )
        };
        assert_eq!(find(), None);

        let prefix =
            steam.join("steamapps/compatdata/1001/pfx/drive_c/users/steamuser/Documents/Slot Game");
        fs::create_dir_all(&prefix).unwrap();
        assert_eq!(find(), Some(prefix));
    }

    #[test]
    fn classify_install_names_the_store_that_claims_the_folder() {
        let base = scratch!("classify-install");
        let steam = base.join("library/steamapps/common/Slot");
        let gog = base.join("GOG Games/Slot Gog");
        let epic = base.join("Epic Games/Slot Epic");
        let heroic = base.join("Heroic/Slot Heroic");
        let heroic_gog = base.join("Heroic/Slot Heroic Gog");
        let legendary = base.join("Legendary/Slot");
        let manual = base.join("Games/Slot Manual");
        let folders = crate::platform::StoreFolders {
            epic_list: Some(base.join("UnrealEngineLauncher")),
            heroic: vec![base.join("config/heroic")],
            legendary: Some(base.join("config/legendary")),
            ..crate::platform::StoreFolders::default()
        };

        for root in [
            &steam,
            &gog,
            &epic,
            &heroic,
            &heroic_gog,
            &legendary,
            &manual,
        ] {
            fs::create_dir_all(root).unwrap();
        }
        write(&gog.join("goggame-456.info"), "{}");
        // Heroic's GOG installs keep the info file too; the launcher that manages the folder still wins.
        write(&heroic_gog.join("goggame-789.info"), "{}");
        write(
            &base.join("UnrealEngineLauncher/LauncherInstalled.dat"),
            &serde_json::json!({
                "InstallationList": [{
                    "AppName": "EpicApp",
                    "InstallLocation": epic.display().to_string(),
                }],
            })
            .to_string(),
        );
        write(
            &base.join("config/heroic/legendaryConfig/legendary/installed.json"),
            &serde_json::json!({
                "HeroicApp": { "install_path": heroic.display().to_string() },
            })
            .to_string(),
        );
        write(
            &base.join("config/heroic/gog_store/installed.json"),
            &serde_json::json!({ "installed": [
                { "appName": "789", "install_path": heroic_gog.display().to_string() },
            ] })
            .to_string(),
        );
        write(
            &base.join("config/legendary/installed.json"),
            &serde_json::json!({
                "LegendaryApp": { "install_path": legendary.display().to_string() },
            })
            .to_string(),
        );

        let classify = |root: &Path| classify_in(root, &folders).store;
        assert_eq!(classify(&steam), "steam");
        assert_eq!(classify(&gog), "gog");
        assert_eq!(classify(&epic), "epic");
        assert_eq!(classify(&heroic), "heroic");
        assert_eq!(classify(&heroic_gog), "heroic");
        assert_eq!(classify(&legendary), "legendary");
        assert_eq!(classify(&manual), "manual");
        assert_eq!(
            classify_in(&epic, &crate::platform::StoreFolders::default()).store,
            "manual"
        );
    }

    #[test]
    fn scan_answers_for_every_game_in_order() {
        let home = scratch!("scan");
        let steam = steam_home(&home, &[]);
        let root = install_slot(&steam, "Slot Game");
        let games = [test_games::slot(), test_games::mirror()];

        let scans = scan_in(
            &libraries_in(&[steam]),
            &games,
            &crate::platform::StoreFolders::default(),
        );
        assert_eq!(scans.len(), 2);
        assert_eq!(scans[0].id, test_games::SLOT_ID);
        assert_eq!(
            scans[0].found.as_ref().map(|f| f.game_root.clone()),
            Some(root.display().to_string())
        );
        assert_eq!(scans[1].id, test_games::MIRROR_ID);
        assert!(scans[1].found.is_none());
        assert!(scans[0].others.is_empty());
        assert!(scans[1].others.is_empty());
    }

    #[test]
    fn a_scan_keeps_every_install_with_the_steam_one_first() {
        let home = scratch!("scan-many");
        let steam = steam_home(&home, &[]);
        let steam_root = install_slot(&steam, "Slot Game");
        let gog_root = home.join("GOG Games").join("Slot Gog");
        // The GOG folder has the game files and one `goggame-*.info` file naming the product id.
        write(&gog_root.join("bin/game.exe"), "");
        write(&gog_root.join("content/base/x.txt"), "");
        write(&gog_root.join("goggame-456.info"), "{}");

        let mut game = test_games::slot();
        game.detect.gog_ids = vec!["456".into()];
        let folders = crate::platform::StoreFolders {
            gog_libraries: vec![home.join("GOG Games")],
            ..crate::platform::StoreFolders::default()
        };

        let scans = scan_in(&libraries_in(&[steam]), &[game], &folders);
        let scan = &scans[0];
        let preferred = scan.found.as_ref().expect("the Steam install is found");
        assert_eq!(preferred.store, "steam");
        assert_eq!(preferred.game_root, steam_root.display().to_string());

        assert_eq!(scan.others.len(), 1);
        assert_eq!(scan.others[0].store, "gog");
        assert_eq!(scan.others[0].game_root, gog_root.display().to_string());
    }

    #[test]
    fn two_claims_of_one_folder_appear_once_under_the_first_store() {
        let base = scratch!("scan-duplicates");
        let root = base.join("Games/Slot");
        write(&root.join("bin/game.exe"), "");
        write(&root.join("content/base/x.txt"), "");
        // Heroic's GOG list and the folder's own info file both name the install.
        write(&root.join("goggame-456.info"), "{}");
        write(
            &base.join("heroic/gog_store/installed.json"),
            &serde_json::json!({ "installed": [
                { "appName": "456", "install_path": root.display().to_string() },
            ] })
            .to_string(),
        );
        let folders = crate::platform::StoreFolders {
            gog_libraries: vec![base.join("Games")],
            heroic: vec![base.join("heroic")],
            ..crate::platform::StoreFolders::default()
        };

        let mut game = test_games::slot();
        game.detect.gog_ids = vec!["456".into()];
        let scans = scan_in(&[], &[game], &folders);
        assert_eq!(
            scans[0].found.as_ref().map(|found| found.store.as_str()),
            Some("heroic")
        );
        assert!(scans[0].others.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn the_windows_order_file_folder_is_in_documents() {
        let home = scratch!("windows-settings");
        let settings = home.join("Documents/Slot Game");
        fs::create_dir_all(&settings).unwrap();

        assert_eq!(
            test_games::slot().find_order_dir(&home.join("game"), &home, None, false),
            Some(settings)
        );
    }
}
