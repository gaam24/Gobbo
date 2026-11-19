//! Importing a Mod Organizer 2 list: the profile's list becomes the file's, and undo brings the old one back.

use super::*;
use crate::context::profile::history::undo_list;
use crate::profiles::actions::save_profile;
use crate::profiles::mo2::import_mo2_list;

#[test]
fn an_mo2_list_becomes_the_profile_list_and_undo_brings_the_old_one_back() {
    let f = fixture("mo2-import");
    f.import_two_mods();
    // modA and modB are in the library; the list also names a mod that is not, and a separator.
    let text = "# comment\r\n+modB\r\n-modA\r\n+Missing Mod\r\n+Cool_separator\r\n";

    // The profiles of a game are kept in the game's own folder.
    let game_data = f.data.game(SLOT_GAME);
    // The list the user had before the import: modA, then modB. Saved first, so undo has that list to bring back.
    let previous = [row("modA", true), row("modB", true)];
    save_profile(&f.data, SLOT_GAME, &previous).unwrap();
    let report = import_mo2_list(&f.data, SLOT_GAME, text).unwrap();
    assert_eq!((report.mods, report.separators), (2, 1));
    assert_eq!(report.missing, vec!["Missing Mod".to_string()]);

    let profiles = load_profiles(&game_data).unwrap();
    let mods: Vec<(&str, bool)> = profiles
        .active()
        .mods
        .iter()
        .map(|entry| (entry.folder.as_str(), entry.enabled))
        .collect();
    assert_eq!(mods, vec![("modB", true), ("modA", false)]);
    let names: Vec<&str> = profiles
        .active()
        .separators
        .iter()
        .map(|separator| separator.name.as_str())
        .collect();
    assert_eq!(names, vec!["Cool"]);

    undo_list(&f.data, SLOT_GAME).unwrap();
    let before = load_profiles(&game_data).unwrap();
    let folders: Vec<&str> = before
        .active()
        .mods
        .iter()
        .map(|entry| entry.folder.as_str())
        .collect();
    assert_eq!(folders, vec!["modA", "modB"]);
    assert!(before.active().separators.is_empty());
}
