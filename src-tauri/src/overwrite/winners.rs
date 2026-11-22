//! Choosing the winning mod for several file paths in one list edit.

use crate::context::Data;
use crate::context::Loaded;
use crate::context::library::not_in_library;
use crate::context::load_game;
use crate::context::load_profiles;
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::domain::pathkey::path_key;
use crate::error::AppError;

/// Chooses `family` as the winner for every path, keyed the way the conflict list names it. All paths are written in
/// one save, so they are one undo. Duplicate paths collapse onto the same key. An empty list still saves, which
/// leaves the list unchanged and writes no history entry.
pub fn set_file_winners(
    root: &Data,
    game_id: &str,
    paths: &[String],
    family: &str,
) -> Result<(), AppError> {
    let Loaded { data, library, .. } = load_game(root, game_id)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;

    if !library.families.contains_key(family) {
        return Err(not_in_library(family));
    }

    let winners = &mut profiles.active_mut().winners;

    for path in paths {
        winners.insert(path_key(path), family.to_string());
    }

    save_profiles(&data, &profiles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::profile::history;
    use crate::domain::library::{Family, Library};
    use crate::domain::profiles::{Entry, Profiles};
    use crate::test_support::{games as test_games, scratch, write};

    fn entry(id: &str) -> Entry {
        Entry {
            id: id.to_string(),
            folder: id.to_string(),
            enabled: true,
            priority: None,
        }
    }

    fn setup(name: &str) -> Data {
        test_games::install();
        let data = Data::new(scratch!(name));
        let game = data.game(test_games::SLOT_ID);
        let mut library = Library::default();

        for id in ["mod-a", "mod-b"] {
            library.families.insert(
                id.to_string(),
                Family {
                    id: id.to_string(),
                    name: id.to_string(),
                    ..Family::default()
                },
            );
        }

        let profiles = Profiles::with_default(vec![entry("mod-a"), entry("mod-b")]);
        write(&game.index(), &serde_json::to_string(&library).unwrap());
        write(
            &game.profiles_file(),
            &serde_json::to_string(&profiles).unwrap(),
        );
        data
    }

    #[test]
    fn all_paths_are_written_in_one_save() {
        let data = setup("winners-many");
        let game = data.game(test_games::SLOT_ID);
        set_file_winners(
            &data,
            test_games::SLOT_ID,
            &["Content/A.TXT".to_string(), "content/b.txt".to_string()],
            "mod-a",
        )
        .unwrap();

        let saved = crate::context::load_profiles(&game).unwrap();
        let winners = &saved.active().winners;
        assert_eq!(winners.len(), 2);
        assert_eq!(
            winners.get("content/a.txt").map(String::as_str),
            Some("mod-a")
        );
        assert_eq!(
            winners.get("content/b.txt").map(String::as_str),
            Some("mod-a")
        );
        assert_eq!(history::read_index(&game).undo.len(), 1);
    }

    #[test]
    fn duplicate_paths_collapse_and_an_unknown_family_is_refused() {
        let data = setup("winners-duplicate");
        set_file_winners(
            &data,
            test_games::SLOT_ID,
            &["content/a.txt".to_string(), "CONTENT/A.txt".to_string()],
            "mod-b",
        )
        .unwrap();
        let saved = crate::context::load_profiles(&data.game(test_games::SLOT_ID)).unwrap();
        let winners = &saved.active().winners;
        assert_eq!(winners.len(), 1);
        assert_eq!(
            winners.get("content/a.txt").map(String::as_str),
            Some("mod-b")
        );

        let error = set_file_winners(
            &data,
            test_games::SLOT_ID,
            &["content/c.txt".to_string()],
            "gone",
        )
        .unwrap_err();
        assert_eq!(error.code, "library.mod_missing");
    }
}
