//! The path guard for "open folder": only Gobbo's data and the game's configured folders may be opened.

use std::path::{Path, PathBuf};

use crate::context::target::deployment_root;
use crate::context::{Data, load_settings};
use crate::games;

/// Whether the interface may open this path in the system file manager. Only Gobbo's own data folder and the folders
/// of the game the user configured are allowed, never an arbitrary path from the webview. The check resolves links and
/// compares whole path components.
pub fn may_open(root: &Data, game_id: Option<&str>, path: &Path) -> bool {
    let mut roots = vec![root.dir.clone()];
    // A game id is not a path and must name a definition that loads, so a traversal id cannot add its folder.
    if let Some(id) = game_id
        && games::is_valid_id(id)
        && let Ok(game) = games::load(id)
    {
        let data = root.game(id);
        roots.push(data.dir.clone());
        let settings = load_settings(&data);

        if let Ok(deployed) = deployment_root(&data, &game, &settings) {
            roots.push(deployed);
        }

        for value in settings.paths() {
            let value = value.trim();

            if !value.is_empty() {
                roots.push(PathBuf::from(value));
            }
        }
    }

    let (Ok(path), roots) = (
        path.canonicalize(),
        roots
            .into_iter()
            .filter_map(|root| root.canonicalize().ok())
            .collect::<Vec<_>>(),
    ) else {
        return false;
    };
    roots
        .iter()
        .any(|root| crate::domain::pathkey::is_inside(root, &path))
}
