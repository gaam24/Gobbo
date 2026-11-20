//! Searching for games in Steam libraries. Read-only: it reads `libraryfolders.vdf` and `appmanifest_*.acf`
//! and checks whether the folder has the files from the game definition. It writes nothing.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;

use super::GameDef;

/// The folders where Steam may be installed on this computer.
pub fn steam_installs(home: &Path) -> Vec<PathBuf> {
    crate::platform::steam_installs(home)
}

/// A game found in a Steam library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct Found {
    /// The game folder, for example `.../steamapps/common/The Witcher 3`.
    pub game_root: String,
    /// The store folder the game came from: the Steam library, or the folder a store names.
    pub library: String,
    /// The store that named the folder: `steam`, `gog`, `epic`, `heroic` or `lutris`.
    pub store: String,
}

/// The result of searching for one supported game. `found` is empty when the game is in no library.
#[derive(Debug, Serialize, Type)]
pub struct GameScan {
    pub id: String,
    pub found: Option<Found>,
}

/// All Steam libraries of this computer: each installation and the folders from its `libraryfolders.vdf`, without repeats.
pub fn libraries(home: &Path) -> Vec<PathBuf> {
    libraries_in(&steam_installs(home))
}

/// The Steam libraries of the given installations, with the folders from each `libraryfolders.vdf`, without repeats.
pub fn libraries_in(steams: &[PathBuf]) -> Vec<PathBuf> {
    let mut libraries: Vec<PathBuf> = Vec::new();
    let mut seen = HashSet::new();

    for steam in steams {
        if !steam.join("steamapps").is_dir() {
            continue;
        }

        let vdf =
            fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")).unwrap_or_default();
        let extra = super::steam::vdf_values(&vdf, "path")
            .into_iter()
            .map(PathBuf::from);
        for library in std::iter::once(steam.clone()).chain(extra) {
            if library.join("steamapps").is_dir() {
                push_unique(&mut libraries, &mut seen, library);
            }
        }
    }

    libraries
}

/// Adds a library if it is not there yet. Compares paths after resolving links, because `~/.steam/steam` is often a link.
fn push_unique(libraries: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, library: PathBuf) {
    let key = fs::canonicalize(&library).unwrap_or_else(|_| library.clone());

    if seen.insert(key) {
        libraries.push(library);
    }
}

/// Looks for the game in the libraries: first by `appmanifest_<id>.acf` (the `installdir` field), then by name
/// from `steam_install_dirs`. A folder counts when it has the files from `markers`.
pub fn find(game: &GameDef, libraries: &[PathBuf]) -> Option<Found> {
    libraries.iter().find_map(|library| {
        let steamapps = library.join("steamapps");
        let from_manifests = game.detect.steam_app_ids.iter().filter_map(|id| {
            let manifest =
                fs::read_to_string(steamapps.join(format!("appmanifest_{id}.acf"))).ok()?;
            super::steam::vdf_values(&manifest, "installdir")
                .into_iter()
                .next()
        });
        let by_name = game.detect.steam_install_dirs.iter().cloned();
        from_manifests
            .chain(by_name)
            .map(|dir| steamapps.join("common").join(dir))
            .find(|root| has_markers(root, game))
            .map(|root| Found {
                game_root: root.display().to_string(),
                library: library.display().to_string(),
                store: "steam".to_string(),
            })
    })
}

/// Whether `root` is the game's folder: it exists and has the files from `markers` (relative paths). With
/// `any_marker`, one marker is enough; with no markers, the folder itself is enough.
pub fn has_markers(root: &Path, game: &GameDef) -> bool {
    let markers = &game.detect.markers;
    let present = |marker: &String| root.join(marker).exists();
    root.is_dir()
        && (markers.is_empty()
            || if game.detect.any_marker {
                markers.iter().any(present)
            } else {
                markers.iter().all(present)
            })
}

/// The Steam library that holds the game folder, that is the directory above `steamapps/common/<game>`.
pub fn library_of(game_root: &Path) -> Option<&Path> {
    let common = game_root.parent()?;
    let steamapps = common.parent()?;
    let library = steamapps.parent()?;
    (common.file_name()? == "common" && steamapps.file_name()? == "steamapps").then_some(library)
}

/// Looks for every game in `games` in the Steam libraries and in the other stores of this computer.
pub fn scan(home: &Path, games: &[GameDef]) -> Vec<GameScan> {
    let program_data = std::env::var_os("PROGRAMDATA").map(PathBuf::from);
    scan_with(home, program_data.as_deref(), games)
}

/// `scan` with the Epic/GOG program-data folder given, for a test or a store outside the usual place.
pub fn scan_with(home: &Path, program_data: Option<&Path>, games: &[GameDef]) -> Vec<GameScan> {
    let mut scans = scan_libraries(&libraries(home), games);
    let candidates = super::stores::candidates(home, program_data);

    for (game, scan) in games.iter().zip(scans.iter_mut()) {
        if scan.found.is_some() {
            continue;
        }

        if let Some(candidate) = candidates
            .iter()
            .find(|candidate| super::stores::matches(game, candidate))
        {
            scan.found = Some(Found {
                game_root: candidate.root.display().to_string(),
                library: candidate
                    .root
                    .parent()
                    .map_or_else(String::new, |parent| parent.display().to_string()),
                store: candidate.store.name().to_string(),
            });
        }
    }

    scans
}

/// Looks for every game in `games` in the given libraries. The result follows the order of the games.
pub fn scan_libraries(libraries: &[PathBuf], games: &[GameDef]) -> Vec<GameScan> {
    games
        .iter()
        .map(|game| GameScan {
            id: game.id.clone(),
            found: find(game, libraries),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;
    use crate::test_support::games as test_games;
    use crate::test_support::scratch;

    /// A Steam installation in `home` with the libraries from `extra` added to `libraryfolders.vdf`.
    fn steam_home(home: &Path, extra: &[&Path]) -> PathBuf {
        let steam = home.join(".local/share/Steam");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        let mut vdf = String::from("\"libraryfolders\"\n{\n");

        for (index, library) in extra.iter().enumerate() {
            write!(
                vdf,
                "\t\"{index}\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n",
                library.display()
            )
            .unwrap();
        }

        vdf.push_str("}\n");
        fs::write(steam.join("steamapps/libraryfolders.vdf"), vdf).unwrap();
        steam
    }

    /// Files of the slot fixture from its definition, in `library/steamapps/common/<dir>`.
    fn install_slot(library: &Path, dir: &str) -> PathBuf {
        let root = library.join("steamapps").join("common").join(dir);
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("content/base")).unwrap();
        fs::write(root.join("bin/game.exe"), b"").unwrap();
        root
    }

    #[cfg(not(windows))]
    #[test]
    fn steam_names_the_tool_of_an_app_or_the_default_tool() {
        let config = "\"InstallConfigStore\"\n{\n\t\"Software\"\n\t{\n\t\t\"CompatToolMapping\"\n\t\t{\n\t\t\t\"0\"\n\t\t\t{\n\t\t\t\t\"name\"\t\t\"Proton-CachyOS Latest\"\n\t\t\t\t\"config\"\t\t\"\"\n\t\t\t}\n\t\t\t\"1091500\"\n\t\t\t{\n\t\t\t\t\"name\"\t\t\"proton_hotfix\"\n\t\t\t}\n\t\t}\n\t}\n}\n";
        assert_eq!(
            crate::games::steam::compat_tool(config, "1091500").as_deref(),
            Some("proton_hotfix")
        );
        assert_eq!(
            crate::games::steam::compat_tool(config, "42").as_deref(),
            Some("Proton-CachyOS Latest")
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn a_config_without_a_compat_tool_mapping_names_no_tool() {
        assert!(
            crate::games::steam::compat_tool("\"InstallConfigStore\"\n{\n}\n", "1091500").is_none()
        );
    }

    #[test]
    fn reads_the_libraries_listed_in_libraryfolders_vdf() {
        let home = scratch!("libraries");
        let second = home.join("games-disk");
        fs::create_dir_all(second.join("steamapps")).unwrap();
        let steam = steam_home(&home, &[&second]);
        assert_eq!(
            libraries_in(std::slice::from_ref(&steam)),
            vec![steam, second]
        );
    }

    #[test]
    fn finds_a_game_through_its_manifest_install_dir() {
        let home = scratch!("manifest");
        let library = home.join("games-disk");
        fs::create_dir_all(library.join("steamapps")).unwrap();
        let steam = steam_home(&home, &[&library]);
        let root = install_slot(&library, "Slot Remastered");
        fs::write(
            library.join("steamapps/appmanifest_1001.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1001\"\n\t\"installdir\"\t\t\"Slot Remastered\"\n}\n",
        )
        .unwrap();

        let found =
            find(&test_games::slot(), &libraries_in(&[steam])).expect("the game should be found");
        assert_eq!(found.game_root, root.display().to_string());
        assert_eq!(found.library, library.display().to_string());
    }

    #[test]
    fn falls_back_to_the_install_folder_name_without_a_manifest() {
        let home = scratch!("fallback");
        let steam = steam_home(&home, &[]);
        let root = install_slot(&steam, "Slot Game");

        let found =
            find(&test_games::slot(), &libraries_in(&[steam])).expect("the game should be found");
        assert_eq!(found.game_root, root.display().to_string());
    }

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

    #[test]
    fn the_order_file_folder_is_found_in_the_proton_prefix_once_it_exists() {
        let home = scratch!("proton");
        let steam = steam_home(&home, &[]);
        let root = install_slot(&steam, "Slot Game");
        let game = test_games::slot();
        assert_eq!(game.find_order_dir(&root, &home), None);

        let prefix =
            steam.join("steamapps/compatdata/1001/pfx/drive_c/users/steamuser/Documents/Slot Game");
        fs::create_dir_all(&prefix).unwrap();
        assert_eq!(game.find_order_dir(&root, &home), Some(prefix));
    }

    #[test]
    fn the_library_is_the_folder_above_steamapps_common() {
        assert_eq!(
            library_of(Path::new("/a/steamapps/common/Slot Game")),
            Some(Path::new("/a"))
        );
        assert_eq!(library_of(Path::new("/a/games/Slot Game")), None);
    }

    #[test]
    fn scan_answers_for_every_game_in_order() {
        let home = scratch!("scan");
        let steam = steam_home(&home, &[]);
        let root = install_slot(&steam, "Slot Game");
        let games = [test_games::slot(), test_games::mirror()];

        let scans = scan_libraries(&libraries_in(&[steam]), &games);
        assert_eq!(scans.len(), 2);
        assert_eq!(scans[0].id, test_games::SLOT_ID);
        assert_eq!(
            scans[0].found.as_ref().map(|f| f.game_root.clone()),
            Some(root.display().to_string())
        );
        assert_eq!(scans[1].id, test_games::MIRROR_ID);
        assert!(scans[1].found.is_none());
    }

    #[cfg(windows)]
    #[test]
    fn the_windows_order_file_folder_is_in_documents() {
        let home = scratch!("windows-settings");
        let settings = home.join("Documents/Slot Game");
        fs::create_dir_all(&settings).unwrap();

        assert_eq!(
            test_games::slot().find_order_dir(&home.join("game"), &home),
            Some(settings)
        );
    }
}
