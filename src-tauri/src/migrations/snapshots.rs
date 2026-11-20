//! Snapshots written before the app folder and the game's data folder were separated. An automatic snapshot was stored
//! under the game's data folder (`<root>/games/<game>/snapshots/<game>/<id>`); every snapshot now lives in the app's
//! folder (`<root>/snapshots/<game>/<id>`). A folder whose manifest names another game or id is left where it is.

use super::{GAMES_DIR, move_if_absent};
use crate::domain::naming;
use crate::error::{AppError, io_at};
use crate::infra::json;
use serde::Deserialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SNAPSHOTS_DIR: &str = "snapshots";
const MANIFEST_FILE: &str = "manifest.json";

/// The part of the manifest the move needs. The other fields are not read, so a newer manifest still moves intact.
#[derive(Debug, Deserialize)]
struct Manifest {
    id: String,
    game: String,
}

pub(super) fn run(root: &Path, _known_game_ids: &[String]) -> Result<(), AppError> {
    let games = root.join(GAMES_DIR);
    let entries = match fs::read_dir(&games) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_at(&games)(error)),
    };

    for entry in entries {
        let entry = entry.map_err(io_at(&games))?;
        let game_id = entry.file_name().to_string_lossy().into_owned();

        if !entry.path().is_dir()
            || !naming::is_plain_folder(&game_id)
            || game_id.ends_with(super::REMOVED_SUFFIX)
        {
            continue;
        }

        move_game(root, &game_id)?;
    }

    Ok(())
}

/// Moves every valid snapshot of one game. A folder that already exists at the destination wins, so two runs can
/// never overwrite one with the other.
fn move_game(root: &Path, game_id: &str) -> Result<(), AppError> {
    let old = root
        .join(GAMES_DIR)
        .join(game_id)
        .join(SNAPSHOTS_DIR)
        .join(game_id);
    let entries = match fs::read_dir(&old) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_at(&old)(error)),
    };

    for entry in entries {
        let entry = entry.map_err(io_at(&old))?;
        let id = entry.file_name().to_string_lossy().into_owned();

        if !entry.path().is_dir() || !naming::is_plain_folder(&id) {
            continue;
        }

        if !matches(&entry.path(), game_id, &id) {
            continue;
        }

        let to = destination(root, game_id, &id);

        if !move_if_absent(&entry.path(), &to)? {
            crate::infra::log::warn(format!(
                "{} already exists; {} is left as it is",
                to.display(),
                entry.path().display()
            ));
        }
    }

    Ok(())
}

/// Whether the manifest names this game and this folder. An unreadable manifest is not a match.
fn matches(dir: &Path, game_id: &str, id: &str) -> bool {
    match json::read_json_or_none::<Manifest>(&dir.join(MANIFEST_FILE)) {
        Some(manifest) => manifest.game == game_id && manifest.id == id,
        None => false,
    }
}

fn destination(root: &Path, game_id: &str, id: &str) -> PathBuf {
    root.join(SNAPSHOTS_DIR).join(game_id).join(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    fn old_snapshot(root: &Path, game: &str, id: &str, manifest: &str) {
        let dir = root
            .join(GAMES_DIR)
            .join(game)
            .join(SNAPSHOTS_DIR)
            .join(game)
            .join(id);
        fs::create_dir_all(dir.join("sets")).unwrap();
        fs::write(dir.join("sets/data.txt"), "saved").unwrap();
        fs::write(dir.join(MANIFEST_FILE), manifest).unwrap();
    }

    fn old_dir(root: &Path, game: &str, id: &str) -> PathBuf {
        root.join(GAMES_DIR)
            .join(game)
            .join(SNAPSHOTS_DIR)
            .join(game)
            .join(id)
    }

    #[test]
    fn an_old_snapshot_moves_to_the_app_folder_and_a_second_run_does_nothing() {
        let root = scratch!("migration-snapshots");
        old_snapshot(&root, "slot", "1-a", r#"{"id":"1-a","game":"slot"}"#);

        run(&root, &[]).unwrap();

        let moved = destination(&root, "slot", "1-a");
        assert_eq!(
            fs::read_to_string(moved.join("sets/data.txt")).unwrap(),
            "saved"
        );
        assert!(!old_dir(&root, "slot", "1-a").exists());

        run(&root, &[]).unwrap();
        assert!(moved.join("sets/data.txt").is_file());
        assert!(!old_dir(&root, "slot", "1-a").exists());
    }

    #[test]
    fn a_tampered_or_unreadable_manifest_is_left_alone() {
        let root = scratch!("migration-snapshots-tampered");
        old_snapshot(&root, "slot", "1-a", r#"{"id":"1-a","game":"other"}"#);
        old_snapshot(&root, "slot", "2-b", r#"{"id":"not-2-b","game":"slot"}"#);
        old_snapshot(&root, "slot", "3-c", "{ not json");

        run(&root, &[]).unwrap();

        assert!(!root.join(SNAPSHOTS_DIR).join("slot").exists());

        for id in ["1-a", "2-b", "3-c"] {
            assert!(old_dir(&root, "slot", id).is_dir(), "{id} moved");
        }
    }
}
