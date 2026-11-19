//! The game's order file: which mods get a row, and whether the list order still decides conflicts.

use super::*;

#[test]
fn disabled_mods_are_not_planned_and_get_a_row_only_with_a_priority() {
    let without = run(&slot_game(), &[m("modA", false, &["content/a.ws"])]);
    assert!(without.files.is_empty());
    assert!(without.settings.is_empty());

    let with_priority = run(
        &slot_game(),
        &[ModInput {
            priority: Some(7),
            ..m("modA", false, &[])
        }],
    );
    assert!(with_priority.files.is_empty());
    assert_eq!(
        with_priority.settings,
        [SettingsRow {
            folder: "modA".into(),
            enabled: false,
            priority: 7
        }]
    );
}

#[test]
fn only_mods_with_a_set_priority_get_a_settings_row() {
    let plan = run(
        &slot_game(),
        &[
            m("modA", true, &["content/x.ws"]),
            ModInput {
                priority: Some(50),
                ..m("modB", true, &["content/y.ws"])
            },
            m("modC", true, &["content/z.ws"]),
        ],
    );
    assert_eq!(
        plan.settings,
        [SettingsRow {
            folder: "modB".into(),
            enabled: true,
            priority: 50
        }]
    );
}

#[test]
fn list_order_still_decides_conflicts_and_not_the_settings_file() {
    let plan = run(
        &slot_game(),
        &[
            m("modA", true, &["content/x.ws"]),
            ModInput {
                priority: Some(9999),
                ..m("modB", true, &["content/x.ws"])
            },
        ],
    );
    assert_eq!(
        plan.conflicts,
        [Conflict {
            path: "content/x.ws".into(),
            winner: "modA".into(),
            losers: vec!["modB".into()]
        }]
    );
    assert_eq!(plan.settings[0].priority, 9999);
}
