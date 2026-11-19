//! The health report: one list that agrees with the status the window already shows.

use super::*;
use crate::status::health::{HealthActionKind, HealthSeverity, health};

#[test]
fn an_empty_library_is_a_warning_with_the_list_action() {
    let f = fixture("health-empty");
    let report = health(&f.data, SLOT_GAME).unwrap();

    let item = report
        .items
        .iter()
        .find(|item| item.message.code == "health.emptyLibrary")
        .expect("the empty library is reported");
    assert_eq!(item.severity, HealthSeverity::Warning);
    assert_eq!(
        item.action.as_ref().map(|action| action.kind),
        Some(HealthActionKind::ShowMods)
    );
}

#[test]
fn a_missing_game_folder_is_a_problem_that_opens_settings() {
    let f = fixture("health-setup");
    save_settings(
        &f.data,
        SLOT_GAME,
        Settings {
            game_root: String::new(),
            ..f.settings()
        },
    )
    .unwrap();

    let report = health(&f.data, SLOT_GAME).unwrap();

    let item = report
        .items
        .iter()
        .find(|item| item.message.code == "health.setup")
        .expect("the missing game folder is reported");
    assert_eq!(item.severity, HealthSeverity::Problem);
    assert_eq!(
        item.action.as_ref().map(|action| action.kind),
        Some(HealthActionKind::OpenSettings)
    );
}
