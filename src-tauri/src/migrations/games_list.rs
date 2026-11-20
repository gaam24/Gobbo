//! The old `games.json` list. The game folders become the source of truth: every known folder the list does not name
//! moves to its `.removed` twin, then the list is removed.

use super::game_ids;
use super::{GAMES_DIR, LEGACY_GAMES_FILE, REMOVED_SUFFIX, move_if_absent};
use crate::error::{AppError, io_at};
use crate::infra::json::{self, ReadError};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

/// The contents of the old `games.json`.
#[derive(Debug, Default, Deserialize)]
struct Legacy {
    #[serde(default)]
    added: Vec<String>,
}

pub(super) fn run(root: &Path, known_game_ids: &[String]) -> Result<(), AppError> {
    let file = root.join(LEGACY_GAMES_FILE);
    let legacy = match json::read_json::<Legacy>(&file) {
        Ok(Some(legacy)) => legacy,
        Ok(None) => return Ok(()),
        Err(ReadError::Parse(error)) => {
            json::keep_broken(&file, &error);
            return Ok(());
        }
        Err(ReadError::Io(error)) => return Err(io_at(&file)(error)),
    };
    let added: BTreeSet<&str> = legacy
        .added
        .iter()
        .map(|id| game_ids::current_id(id))
        .collect();
    let known: BTreeSet<&str> = known_game_ids
        .iter()
        .map(|id| game_ids::current_id(id))
        .collect();
    let games = root.join(GAMES_DIR);
    let entries = match fs::read_dir(&games) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::remove_file(&file).map_err(io_at(&file))?;
            return Ok(());
        }
        Err(error) => return Err(io_at(&games)(error)),
    };
    let mut moved_all = true;

    for entry in entries {
        let entry = entry.map_err(io_at(&games))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if name.starts_with('.') || name.ends_with(REMOVED_SUFFIX) || !entry.path().is_dir() {
            continue;
        }

        let current = game_ids::current_id(name.as_ref());

        if added.contains(current) {
            continue;
        }

        if !known.contains(current) && !game_ids::is_renamed(name.as_ref()) {
            continue;
        }

        let from = entry.path();
        let to = games.join(format!("{current}{REMOVED_SUFFIX}"));

        match move_if_absent(&from, &to) {
            Ok(true) => {}
            Ok(false) => {
                crate::infra::log::warn(format!(
                    "{} already exists; the old game list stays for the next start",
                    to.display()
                ));
                moved_all = false;
            }
            Err(error) => {
                crate::infra::log::warn(format!(
                    "could not move {} to {} ({error}); the old game list stays for the next start",
                    from.display(),
                    to.display()
                ));
                moved_all = false;
            }
        }
    }

    if moved_all {
        fs::remove_file(&file).map_err(io_at(&file))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::fs;

    fn folder(root: &Path, name: &str) {
        fs::create_dir_all(root.join(GAMES_DIR).join(name)).unwrap();
    }

    fn a_list(root: &Path, added: &str) {
        fs::write(root.join(LEGACY_GAMES_FILE), added).unwrap();
    }

    #[test]
    fn a_listed_folder_stays_and_an_unlisted_known_folder_moves_aside() {
        let root = scratch!("migration-games-list");
        folder(&root, "slot");
        folder(&root, "mirror");
        a_list(&root, r#"{"added":["slot"]}"#);

        run(&root, &["slot".to_string(), "mirror".to_string()]).unwrap();

        assert!(root.join(GAMES_DIR).join("slot").is_dir());
        assert!(root.join(GAMES_DIR).join("mirror.removed").is_dir());
        assert!(!root.join(GAMES_DIR).join("mirror").exists());
        assert!(!root.join(LEGACY_GAMES_FILE).exists());
    }

    #[test]
    fn an_old_id_in_the_list_matches_its_current_folder() {
        let root = scratch!("migration-games-list-old-id");
        folder(&root, "the-witcher-3-wild-hunt");
        a_list(&root, r#"{"added":["witcher3"]}"#);

        run(&root, &["the-witcher-3-wild-hunt".to_string()]).unwrap();

        assert!(
            root.join(GAMES_DIR)
                .join("the-witcher-3-wild-hunt")
                .is_dir()
        );
        assert!(
            !root
                .join(GAMES_DIR)
                .join("the-witcher-3-wild-hunt.removed")
                .exists()
        );
        assert!(!root.join(LEGACY_GAMES_FILE).exists());
    }

    #[test]
    fn an_unknown_folder_is_left_alone() {
        let root = scratch!("migration-games-list-unknown");
        folder(&root, "mystery");
        a_list(&root, r#"{"added":[]}"#);

        run(&root, &["slot".to_string()]).unwrap();

        assert!(root.join(GAMES_DIR).join("mystery").is_dir());
        assert!(!root.join(GAMES_DIR).join("mystery.removed").exists());
        assert!(!root.join(LEGACY_GAMES_FILE).exists());
    }

    #[test]
    fn a_broken_list_is_kept_aside_and_no_folder_moves() {
        let root = scratch!("migration-games-list-broken");
        folder(&root, "slot");
        a_list(&root, "{ not json");

        run(&root, &["slot".to_string()]).unwrap();

        assert!(root.join(GAMES_DIR).join("slot").is_dir());
        assert!(!root.join(LEGACY_GAMES_FILE).exists());
        assert!(root.join("games.json.broken").exists());
    }
}
