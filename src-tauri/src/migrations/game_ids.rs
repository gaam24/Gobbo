//! The game data folders under the ids they used before the definitions switched to full-title slugs.

use super::{GAMES_DIR, REMOVED_SUFFIX, move_if_absent};
use crate::error::AppError;
use std::path::Path;

/// The ids of the old definitions and the ids that replaced them. Once a game has shipped, its old id stays here.
const RENAMED: &[(&str, &str)] = &[
    ("cyberpunk2077", "cyberpunk-2077"),
    ("lethalcompany", "lethal-company"),
    ("taintedgrail", "tainted-grail-the-fall-of-avalon"),
    ("tcgcardshopsimulator", "tcg-card-shop-simulator"),
    ("witcher3", "the-witcher-3-wild-hunt"),
];

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
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::fs;

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
}
