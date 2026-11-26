use std::path::PathBuf;

use serde::Serialize;
use specta::Type;

use crate::games::stores::Store;

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

/// The store an install folder belongs to, and the Wine prefix that store runs it in when it names one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreInstall {
    /// The store that claims the folder, or `Manual` when no store claims it.
    pub store: Store,
    /// The store's prefix for the game (Heroic and Lutris name one).
    pub prefix: Option<PathBuf>,
}

impl StoreInstall {
    /// The prefix as the settings keep it: the path, or empty for none.
    pub fn prefix_text(&self) -> String {
        self.prefix
            .as_ref()
            .map(|prefix| prefix.display().to_string())
            .unwrap_or_default()
    }
}
