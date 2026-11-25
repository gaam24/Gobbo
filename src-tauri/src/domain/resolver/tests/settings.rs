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

#[test]
fn a_mod_gets_an_order_row_exactly_when_the_game_reads_it_in_order() {
    let deploy = slot_game();
    let mods = [
        ModInput {
            priority: Some(1),
            ..m("modA", true, &["content/a.ws"])
        },
        // Its files all keep game paths, so it has no folder of its own in the slot.
        ModInput {
            priority: Some(2),
            ..m("modB", true, &["DLC/dlcB/blob0.bundle"])
        },
        // A mod without a slot has no folder either.
        ModInput {
            priority: Some(3),
            ..mirrored("modC", &["content/c.ws"])
        },
        ModInput {
            priority: Some(4),
            ..m("modD", false, &["content/d.ws"])
        },
    ];
    let expected: Vec<&str> = mods
        .iter()
        .filter(|input| is_orderable(&deploy, input))
        .map(|input| input.folder.as_ref())
        .collect();
    let plan = run(&deploy, &mods);
    let rows: Vec<&str> = plan
        .settings
        .iter()
        .map(|row| row.folder.as_str())
        .collect();

    assert_eq!(expected, ["modA", "modD"]);
    assert_eq!(rows, expected);

    // A flat slot has no folder per mod, so its mod is not orderable and gets no row.
    let flat_mod = ModInput {
        priority: Some(5),
        ..m("modE", true, &["content/e.ws"])
    };
    assert!(!is_orderable(&flat(), &flat_mod));
    assert!(run(&flat(), &[flat_mod]).settings.is_empty());
}
