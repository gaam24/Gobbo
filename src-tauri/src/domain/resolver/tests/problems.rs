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

#[test]
fn a_file_of_one_mod_and_a_folder_of_another_name_both_mods_by_their_names() {
    let file_mod = ModInput {
        display_name: "Example Mod A".into(),
        ..m("local:example-mod-a", true, &["DLC/dlcA"])
    };
    let folder_mod = ModInput {
        display_name: "Example Mod B".into(),
        ..m("local:example-mod-b", true, &["DLC/dlcA/blob0.bundle"])
    };
    let plan = run(&slot_game(), &[file_mod, folder_mod]);
    assert_eq!(
        plan.problems,
        [Problem::FileFolderCollision {
            path: "DLC/dlcA".into(),
            file: "Example Mod A".into(),
            folder: "Example Mod B".into(),
            same_mod: false,
        }]
    );
    let message = plan.problems[0].message();
    assert_eq!(message.code, "plan.file_folder_collision_between");
    assert_eq!(
        message.params.get("file").map(String::as_str),
        Some("Example Mod A")
    );
    assert_eq!(
        message.params.get("folder").map(String::as_str),
        Some("Example Mod B")
    );
}

#[test]
fn a_folder_with_many_files_names_its_clash_once() {
    let plan = run(
        &slot_game(),
        &[
            m("modA", true, &["DLC/dlcA"]),
            m(
                "modB",
                true,
                &["DLC/dlcA/one.bundle", "DLC/dlcA/two.bundle"],
            ),
        ],
    );
    assert_eq!(plan.problems.len(), 1, "{:?}", plan.problems);
}

#[test]
fn a_clash_inside_one_mod_keeps_the_message_about_the_archive() {
    let plan = run(
        &slot_game(),
        &[m("modA", true, &["DLC/dlcA", "DLC/dlcA/blob0.bundle"])],
    );
    assert!(
        plan.problems
            .iter()
            .any(|problem| problem.message().code == "plan.file_folder_collision")
    );
}
