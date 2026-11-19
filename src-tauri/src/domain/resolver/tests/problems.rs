//! Problems that stop a deployment.

use super::*;

#[test]
fn duplicate_family_ids_are_reported_and_planned_once() {
    let plan = run(
        &slot_game(),
        &[
            m("modA", true, &["content/a.ws"]),
            m("modA", true, &["content/b.ws"]),
        ],
    );
    assert_eq!(plan.problems, [Problem::DuplicateMod { id: "modA".into() }]);
    assert_eq!(file_set(&plan), ["modA/content/a.ws"]);
}
