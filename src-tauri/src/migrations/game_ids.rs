//! The game data folders under the ids they used before the definitions switched to full-title slugs.

use super::{GAMES_DIR, REMOVED_SUFFIX, move_if_absent};
use crate::error::AppError;
use std::fs;
use std::path::Path;

/// The ids of the old definitions and the ids that replaced them. Once a game has shipped, its old id stays here.
const RENAMED: &[(&str, &str)] = &[
    ("cyberpunk2077", "cyberpunk-2077"),
    ("lethalcompany", "lethal-company"),
    ("taintedgrail", "tainted-grail-the-fall-of-avalon"),
    ("tcgcardshopsimulator", "tcg-card-shop-simulator"),
    ("witcher3", "the-witcher-3-wild-hunt"),
];

/// Deployment files under a profile's state folder that name absolute paths of the game data folder. The journal is
/// what undo reads; the mount file is what the hook reads on the next launch.
const PATH_FILES: &[&str] = &["journal.jsonl", "overlay-mounts.json"];

/// The current id for an id from an older definition, or the id itself.
pub(super) fn current_id(id: &str) -> &str {
    RENAMED
        .iter()
        .find(|(old, _)| old.eq_ignore_ascii_case(id))
        .map_or(id, |(_, new)| *new)
}

/// Whether the id was renamed at some point.
pub(super) fn is_renamed(id: &str) -> bool {
    RENAMED.iter().any(|(old, _)| old.eq_ignore_ascii_case(id))
}

pub(super) fn run(root: &Path, _known_game_ids: &[String]) -> Result<(), AppError> {
    let games = root.join(GAMES_DIR);

    for (old, new) in RENAMED {
        for suffix in ["", REMOVED_SUFFIX] {
            let from = games.join(format!("{old}{suffix}"));
            let to = games.join(format!("{new}{suffix}"));
            // A folder that already has the current id wins, so two runs can never overwrite one with the other.
            if from.exists() && !move_if_absent(&from, &to)? {
                crate::infra::log::warn(format!(
                    "{} already exists; {} is left as it is",
                    to.display(),
                    from.display()
                ));
            }
            // A folder moved by an earlier run can still hold deployment files with paths of the old id. Undo checks
            // those paths against the current folder and would refuse them as paths of another game.
            rewrite_paths(&to, &from)?;
        }
    }

    Ok(())
}

/// Rewrites old game folder paths in the deployment files of every profile of `to`. A missing state folder is fine.
fn rewrite_paths(to: &Path, from: &Path) -> Result<(), AppError> {
    let state = to.join("state");
    let Ok(profiles) = fs::read_dir(&state) else {
        return Ok(());
    };

    for profile in profiles.flatten() {
        for name in PATH_FILES {
            let path = profile.path().join(name);
            if path.is_file() {
                rewrite_file(&path, from, to)?;
            }
        }
    }

    Ok(())
}

/// Replaces the escaped old folder prefix in a JSON file. Paths are JSON strings, so escaping the prefix once and
/// replacing it keeps every field as it was, including fields a newer build added. A file without the prefix, or a
/// damaged line, is left alone; the journal reader has its own recovery for damaged lines.
fn rewrite_file(path: &Path, from: &Path, to: &Path) -> Result<(), AppError> {
    let old = escaped_prefix(from);
    let new = escaped_prefix(to);
    if old == new {
        return Ok(());
    }

    let text = fs::read_to_string(path).map_err(crate::error::io_at(path))?;

    if !text.contains(&old) {
        return Ok(());
    }

    crate::infra::store::write_atomic(path, text.replace(&old, &new).as_bytes())?;
    Ok(())
}

/// A directory prefix as it appears inside a JSON string: backslashes and quotes escaped, one trailing separator.
fn escaped_prefix(path: &Path) -> String {
    format!("{}{}", path.display(), std::path::MAIN_SEPARATOR)
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    /// A path as serde writes it into a JSON string.
    fn json(path: &Path) -> String {
        path.display().to_string().replace('\\', "\\\\")
    }

    #[test]
    fn an_old_active_folder_and_its_removed_twin_get_the_current_id() {
        let root = scratch!("migration-game-ids");
        let games = root.join(GAMES_DIR);
        fs::create_dir_all(games.join("witcher3")).unwrap();
        fs::create_dir_all(games.join("witcher3.removed")).unwrap();

        run(&root, &[]).unwrap();

        assert!(games.join("the-witcher-3-wild-hunt").is_dir());
        assert!(games.join("the-witcher-3-wild-hunt.removed").is_dir());
        assert!(!games.join("witcher3").exists());
        assert!(!games.join("witcher3.removed").exists());
    }

    #[test]
    fn an_existing_current_folder_is_not_replaced() {
        let root = scratch!("migration-game-ids-conflict");
        let games = root.join(GAMES_DIR);
        fs::create_dir_all(games.join("witcher3")).unwrap();
        fs::create_dir_all(games.join("the-witcher-3-wild-hunt")).unwrap();
        fs::write(games.join("the-witcher-3-wild-hunt/current.txt"), "current").unwrap();

        run(&root, &[]).unwrap();

        assert!(games.join("witcher3").exists());
        assert_eq!(
            fs::read_to_string(games.join("the-witcher-3-wild-hunt/current.txt")).unwrap(),
            "current"
        );
    }

    #[test]
    fn the_current_id_is_found_without_case_and_unknown_ids_stay_as_they_are() {
        assert_eq!(current_id("Witcher3"), "the-witcher-3-wild-hunt");
        assert_eq!(
            current_id("tcgcardshopsimulator"),
            "tcg-card-shop-simulator"
        );
        assert_eq!(current_id("valheim"), "valheim");
        assert!(is_renamed("WITCHER3"));
        assert!(!is_renamed("valheim"));
    }

    /// A journal of one profile with an overlay session, as the writer leaves it.
    fn journal_with(old_state: &Path) -> String {
        format!(
            "{{\"op\":\"layer_prepared\",\"mods\":\"{}\",\"overwrite\":\"{}\"}}\n",
            json(&old_state.join("mods")),
            json(&old_state.join("overwrite"))
        )
    }

    #[test]
    fn a_moved_journal_names_the_new_game_folder() {
        let root = scratch!("migration-journal-moved");
        let games = root.join(GAMES_DIR);
        let old = games.join("witcher3");
        let old_state = old.join("state").join("p1");
        fs::create_dir_all(&old_state).unwrap();
        fs::write(old_state.join(PATH_FILES[0]), journal_with(&old_state)).unwrap();

        run(&root, &[]).unwrap();

        let new_state = games
            .join("the-witcher-3-wild-hunt")
            .join("state")
            .join("p1");
        let text = fs::read_to_string(new_state.join(PATH_FILES[0])).unwrap();
        assert!(text.contains(&json(&new_state.join("mods"))), "{text}");
        assert!(text.contains(&json(&new_state.join("overwrite"))), "{text}");
        assert!(!text.contains("witcher3"), "{text}");

        // The step is idempotent: a second run leaves the journal as it is.
        run(&root, &[]).unwrap();
        assert_eq!(
            fs::read_to_string(new_state.join(PATH_FILES[0])).unwrap(),
            text
        );
    }

    #[test]
    fn a_journal_left_by_an_earlier_move_is_fixed_without_moving_a_folder() {
        let root = scratch!("migration-journal-already-moved");
        let games = root.join(GAMES_DIR);
        let new_state = games
            .join("the-witcher-3-wild-hunt")
            .join("state")
            .join("p1");
        let old_state = games.join("witcher3").join("state").join("p1");
        fs::create_dir_all(&new_state).unwrap();
        fs::write(new_state.join(PATH_FILES[0]), journal_with(&old_state)).unwrap();

        run(&root, &[]).unwrap();

        let text = fs::read_to_string(new_state.join(PATH_FILES[0])).unwrap();
        assert!(text.contains(&json(&new_state.join("mods"))), "{text}");
        assert!(!text.contains("witcher3"), "{text}");
    }

    #[test]
    fn the_mount_file_follows_the_folder_and_unrelated_paths_stay() {
        let root = scratch!("migration-mounts-moved");
        let games = root.join(GAMES_DIR);
        let new_state = games
            .join("the-witcher-3-wild-hunt")
            .join("state")
            .join("p2");
        let old_state = games.join("witcher3").join("state").join("p2");
        fs::create_dir_all(&new_state).unwrap();
        let text = format!(
            "{{\"mounts\":[{{\"guest\":\"{}\",\"upper\":\"{}\"}}]}}\n",
            json(Path::new("C:/Games/Witcher 3")),
            json(&old_state.join("overwrite"))
        );
        fs::write(new_state.join(PATH_FILES[1]), &text).unwrap();

        run(&root, &[]).unwrap();

        let updated = fs::read_to_string(new_state.join(PATH_FILES[1])).unwrap();
        assert!(
            updated.contains(&json(&new_state.join("overwrite"))),
            "{updated}"
        );
        assert!(updated.contains("C:/Games/Witcher 3"), "{updated}");
    }

    #[test]
    fn a_damaged_journal_line_and_a_current_journal_are_left_alone() {
        let root = scratch!("migration-journal-damaged");
        let games = root.join(GAMES_DIR);
        let new_state = games
            .join("the-witcher-3-wild-hunt")
            .join("state")
            .join("p1");
        fs::create_dir_all(&new_state).unwrap();
        let original = format!("{{ not json\n{}", journal_with(&new_state));
        fs::write(new_state.join(PATH_FILES[0]), &original).unwrap();

        run(&root, &[]).unwrap();

        assert_eq!(
            fs::read_to_string(new_state.join(PATH_FILES[0])).unwrap(),
            original
        );
    }
}
