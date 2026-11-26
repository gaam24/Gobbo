use std::path::{Path, PathBuf};

use super::model::{Found, GameScan, StoreInstall};
use crate::domain::pathkey::same_path;
use crate::games::GameDef;
use crate::games::stores::Store;
use crate::games::stores::steam::{find_all, libraries, library_of};

/// The store that claims an install folder. Missing or unreadable store files simply do not match.
pub fn classify_install(home: &Path, game_root: &Path) -> StoreInstall {
    classify_in(game_root, &crate::games::stores::store_folders(home))
}

/// `classify_install` with the stores' folders given, so a test can use scratch folders.
pub fn classify_in(game_root: &Path, folders: &crate::platform::StoreFolders) -> StoreInstall {
    let install = |store, prefix| StoreInstall { store, prefix };

    if library_of(game_root).is_some() {
        return install(Store::Steam, None);
    }

    // A launcher that manages the folder wins over a plain `goggame-*.info` file: Heroic's GOG installs keep such a
    // file too, but the launcher that starts the game owns the install. The candidates come in that order.
    let is_root = |root: &Path| same_path(game_root, root);

    if let Some(candidate) = crate::games::stores::candidates_in(folders)
        .into_iter()
        .find(|candidate| is_root(&candidate.root))
    {
        return install(candidate.store, candidate.prefix);
    }

    if !crate::games::stores::gog::product_ids(game_root).is_empty() {
        return install(Store::Gog, None);
    }

    install(Store::Manual, None)
}

/// Looks for every game in `games` in the Steam libraries and in the other stores of this computer.
pub fn scan(home: &Path, games: &[GameDef]) -> Vec<GameScan> {
    scan_in(
        &libraries(home),
        games,
        &crate::games::stores::store_folders(home),
    )
}

/// `scan` with the Steam libraries and the stores' folders given, so a test can use scratch folders on any platform.
pub fn scan_in(
    libraries: &[PathBuf],
    games: &[GameDef],
    folders: &crate::platform::StoreFolders,
) -> Vec<GameScan> {
    let candidates = crate::games::stores::candidates_in(folders);

    games
        .iter()
        .map(|game| {
            let mut installs = Vec::new();
            let mut claimed: Vec<PathBuf> = Vec::new();

            for hit in find_all(game, libraries) {
                push_install(&mut installs, &mut claimed, hit);
            }

            for candidate in &candidates {
                if !crate::games::stores::matches(game, candidate) {
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

    if claimed.iter().any(|other| same_path(other, &root)) {
        return;
    }

    claimed.push(root);
    installs.push(hit);
}
