//! Migrations of the files Gobbo keeps between versions. Each step is idempotent: it checks for the old layout and
//! does nothing when that layout is absent. The app can therefore run them on every start without a version marker.
//!
//! A step moves data; it never deletes a game folder. The only file a step removes is the old `games.json`, and only
//! after every folder it named was migrated.

mod game_ids;
mod games_list;
mod settings_method;
mod snapshots;

use crate::error::{AppError, io_at};
use std::fs;
use std::path::{Path, PathBuf};

/// The folder with one subfolder per game.
pub const GAMES_DIR: &str = "games";

/// The list of games used before the game folders became the source of truth.
pub const LEGACY_GAMES_FILE: &str = "games.json";

/// The suffix of a game folder whose game was removed from the list but whose data stays on disk.
pub const REMOVED_SUFFIX: &str = ".removed";

/// The folder of a game that was removed from the list.
pub fn removed_path(root: &Path, game_id: &str) -> PathBuf {
    root.join(GAMES_DIR)
        .join(format!("{game_id}{REMOVED_SUFFIX}"))
}

/// Moves `from` to `to` unless `to` already exists. Returns `false` and leaves both in place when the destination is
/// taken, so the caller can warn and carry on. A folder that already has the destination name never gets overwritten.
fn move_if_absent(from: &Path, to: &Path) -> Result<bool, AppError> {
    if to.exists() {
        return Ok(false);
    }

    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(io_at(parent))?;
    }

    fs::rename(from, to).map_err(io_at(from))?;
    Ok(true)
}

/// One migration step. It receives the data root and the ids of the current game definitions.
type Step = fn(&Path, &[String]) -> Result<(), AppError>;

/// Runs every migration in order. A failing step is reported and the next step still runs; the failed step is retried
/// on the next start because no version marker records a failure as done.
pub fn run(root: &Path, known_game_ids: &[String]) {
    let steps: &[(&str, Step)] = &[
        ("game ids", game_ids::run),
        ("settings deploy_method", settings_method::run),
        ("snapshots", snapshots::run),
        ("old games list", games_list::run),
    ];

    for (name, step) in steps {
        if let Err(error) = step(root, known_game_ids) {
            crate::infra::log::warn(format!("migration {name} failed: {error}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::fs;

    #[test]
    fn an_old_unlisted_game_becomes_a_removed_folder_with_its_current_id() {
        let root = scratch!("migration-old-removed");
        let games = root.join(GAMES_DIR);
        fs::create_dir_all(games.join("witcher3")).unwrap();
        fs::write(root.join(LEGACY_GAMES_FILE), r#"{"added":[]}"#).unwrap();

        run(&root, &["the-witcher-3-wild-hunt".to_string()]);

        assert!(games.join("the-witcher-3-wild-hunt.removed").is_dir());
        assert!(!games.join("witcher3").exists());
        assert!(!games.join("witcher3.removed").exists());
        assert!(!root.join(LEGACY_GAMES_FILE).exists());
    }

    #[test]
    fn an_old_id_and_an_old_list_become_the_current_folder_layout() {
        let root = scratch!("migration-layout");
        let games = root.join(GAMES_DIR);
        fs::create_dir_all(games.join("witcher3")).unwrap();
        fs::write(root.join(LEGACY_GAMES_FILE), r#"{"added":["witcher3"]}"#).unwrap();

        run(&root, &["the-witcher-3-wild-hunt".to_string()]);

        assert!(games.join("the-witcher-3-wild-hunt").is_dir());
        assert!(!games.join("witcher3").exists());
        assert!(!root.join(LEGACY_GAMES_FILE).exists());

        // Every step checks the layout, so a second run changes nothing.
        run(&root, &["the-witcher-3-wild-hunt".to_string()]);
        assert!(games.join("the-witcher-3-wild-hunt").is_dir());
    }
}
