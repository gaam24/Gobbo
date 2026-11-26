//! Tests for the profile list and lifecycle actions.

use super::lifecycle::remove_profile_state;
use super::mods::{SeparatorIds, separator};
use crate::context::Data;
use crate::domain::profiles::Row;
use crate::domain::profiles::Separator;
use crate::test_support::scratch;
use crate::test_support::write;
use std::fs;

fn row(folder: &str) -> Row {
    crate::test_support::row(folder, false)
}

#[test]
fn removing_a_profile_state_removes_its_folder_and_a_missing_one_is_fine() {
    let base = scratch!("remove-state");
    let data = Data::new(base);
    let state = data.profile_state("p2");
    fs::create_dir_all(state.join("inner")).unwrap();

    remove_profile_state(&data, "p2").unwrap();
    assert!(!state.exists());
    remove_profile_state(&data, "p2").unwrap();
}

#[test]
fn a_profile_state_that_is_not_a_folder_is_an_error() {
    let base = scratch!("state-not-folder");
    let data = Data::new(base);
    write(&data.profile_state("p3"), "not a folder");

    assert!(remove_profile_state(&data, "p3").is_err());
}

fn separator_row(id: &str) -> Separator {
    Separator {
        id: id.to_string(),
        name: id.to_string(),
        before: None,
    }
}

#[test]
fn a_separator_name_is_trimmed_and_at_most_sixty_characters() {
    let mut ids = SeparatorIds::new(&[]);
    assert!(separator(&row(""), "   ", &mut ids).is_err());
    assert!(separator(&row(""), &"x".repeat(61), &mut ids).is_err());
    let kept = separator(&row(""), &format!("  {}  ", "x".repeat(60)), &mut ids).unwrap();
    assert_eq!(kept.name, "x".repeat(60));
}

#[test]
fn a_new_separator_above_the_others_does_not_take_their_ids() {
    let existing = [separator_row("sep1"), separator_row("sep2")];
    let mut ids = SeparatorIds::new(&existing);
    // The list comes top first, so the new separator is claimed before the two that already exist.
    assert_eq!(separator(&row(""), "New", &mut ids).unwrap().id, "sep3");
    assert_eq!(
        separator(&row("sep1"), "First", &mut ids).unwrap().id,
        "sep1"
    );
    assert_eq!(
        separator(&row("sep2"), "Second", &mut ids).unwrap().id,
        "sep2"
    );
    // A row that carries an id the profile did not have keeps it.
    assert_eq!(
        separator(&row("modB"), "Named", &mut ids).unwrap().id,
        "modB"
    );
}

#[test]
fn a_removed_separator_id_is_not_reused_while_a_higher_one_exists() {
    let existing = [separator_row("sep1"), separator_row("sep3")];
    let mut ids = SeparatorIds::new(&existing);
    assert_eq!(separator(&row(""), "New", &mut ids).unwrap().id, "sep4");
}

#[test]
fn two_rows_with_the_same_id_get_different_ones() {
    let mut ids = SeparatorIds::new(&[]);
    assert_eq!(separator(&row("sep1"), "One", &mut ids).unwrap().id, "sep1");
    assert_eq!(
        separator(&row("sep1"), "Again", &mut ids).unwrap().id,
        "sep2"
    );
}
