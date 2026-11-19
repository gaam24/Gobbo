//! Conflicts between mods: who wins a contested file, and how the losers stand in the list.

use super::*;

#[test]
fn a_chosen_winner_takes_a_mirrored_file() {
    let mods = [
        mirrored("modA", &["r6/scripts/a.reds"]),
        mirrored("modB", &["r6/scripts/a.reds"]),
    ];
    let chosen = choosing(&mirror_game(), &mods, "r6/scripts/a.reds", "modB");
    assert_eq!(file_set(&chosen), ["modB/r6/scripts/a.reds"]);
    assert_eq!(chosen.conflicts[0].winner, "modB");
    assert_eq!(chosen.conflicts[0].losers, ["modA"]);

    // Without a choice the first one on the list wins.
    assert_eq!(
        file_set(&run(&mirror_game(), &mods)),
        ["modA/r6/scripts/a.reds"]
    );
}

#[test]
fn the_summary_keeps_a_shared_script_conflict() {
    let mods = [
        m("modA", true, &["content/scripts/a.ws"]),
        m("modB", true, &["content/scripts/a.ws"]),
    ];
    let full = run(&slot_game(), &mods);
    let summary = plan_with_mode(&slot_game(), &mods, &BTreeMap::new(), PlanMode::Summary);
    assert_eq!(summary.file_count as usize, full.files.len());
    assert_eq!(summary.conflicts, full.conflicts);
}

#[test]
fn the_summary_counts_files_that_stay_in_the_mod_folder() {
    let mods = [m(
        "modA",
        true,
        &["content/mesh/a.bin", "content/mesh/b.bin"],
    )];
    let full = run(&plain(), &mods);
    let summary = plan_with_mode(&plain(), &mods, &BTreeMap::new(), PlanMode::Summary);
    assert_eq!(summary.file_count, 2);
    assert_eq!(summary.file_count as usize, full.files.len());
    assert!(summary.files.is_empty());
}

#[test]
fn the_summary_counts_the_files_and_keeps_the_conflicts() {
    let mods = [
        mirrored("modA", &["r6/scripts/a.reds", "r6/scripts/only_a.reds"]),
        mirrored("modB", &["r6/scripts/a.reds"]),
    ];
    let deploy = mirror_game();
    let full = run(&deploy, &mods);
    let summary = plan_with_mode(&deploy, &mods, &BTreeMap::new(), PlanMode::Summary);
    assert_eq!(summary.file_count as usize, full.files.len());
    assert_eq!(summary.conflicts, full.conflicts);
    assert_eq!(summary.problems, full.problems);
}

#[test]
fn a_chosen_winner_keeps_the_other_losers_in_list_order() {
    let mods = ["modA", "modB", "modC", "modD"].map(|id| mirrored(id, &["r6/scripts/a.reds"]));
    let chosen = choosing(&mirror_game(), &mods, "r6/scripts/a.reds", "modB");
    assert_eq!(chosen.conflicts[0].winner, "modB");
    assert_eq!(chosen.conflicts[0].losers, ["modA", "modC", "modD"]);
}

#[test]
fn a_shared_script_is_one_file_in_the_game() {
    let mods = [
        m("modA", true, &["content/scripts/a.ws"]),
        m("modB", true, &["content/scripts/a.ws"]),
    ];
    let plan = run(&slot_game(), &mods);
    assert_eq!(plan.conflicts[0].path, "content/scripts/a.ws");
    assert_eq!(plan.conflicts[0].winner, "modA");
    assert_eq!(file_set(&plan), ["modA/content/scripts/a.ws"]);
    // The winner's copy stays in its own folder.
    assert_eq!(dests(&plan), ["Mods/modA/content/scripts/a.ws"]);

    let chosen = choosing(&slot_game(), &mods, "content/scripts/a.ws", "modB");
    assert_eq!(chosen.conflicts[0].winner, "modB");
    assert_eq!(file_set(&chosen), ["modB/content/scripts/a.ws"]);
}

#[test]
fn files_outside_the_shared_kinds_stay_in_their_folders() {
    let files = ["content/blob0.bundle", "content/a.ws"];
    let plan = run(
        &slot_game(),
        &[m("modA", true, &files), m("modB", true, &files)],
    );
    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(plan.conflicts[0].path, "content/a.ws");
    assert_eq!(plan.files.len(), 3);
}

/// A flat slot and one path mirror, the shape of a game whose loose files are resolved by resource name.
fn resource_names() -> Deploy {
    let mut deploy = flat();
    deploy.resource_names = true;
    deploy.mirror = vec![crate::domain::deploy::Mirror {
        dir: "Data/Music".into(),
        extensions: Vec::new(),
        prefix: String::new(),
    }];
    deploy
}

#[test]
fn resource_names_conflict_across_a_flat_slot_and_a_mirror() {
    let mods = [
        m("modA", true, &["a/theme.ogg"]),
        mirrored("modB", &["Data/Music/theme.ogg"]),
    ];
    let plan = run(&resource_names(), &mods);
    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(plan.conflicts[0].path, "theme.ogg");
    assert_eq!(plan.conflicts[0].winner, "modA");
    assert_eq!(plan.files.len(), 1);
    assert_eq!(dests(&plan), ["Mods/theme.ogg"]);

    let chosen = choosing(&resource_names(), &mods, "theme.ogg", "modB");
    assert_eq!(dests(&chosen), ["Data/Music/theme.ogg"]);

    let summary = plan_with_mode(
        &resource_names(),
        &mods,
        &BTreeMap::new(),
        PlanMode::Summary,
    );
    assert_eq!(summary.file_count as usize, plan.files.len());
    assert_eq!(summary.conflicts, plan.conflicts);
}

#[test]
fn resource_names_leave_files_of_another_kind_apart() {
    let plan = run(
        &resource_names(),
        &[
            m("modA", true, &["a/theme.ogg"]),
            mirrored("modB", &["Data/Music/theme.bik"]),
        ],
    );
    assert!(plan.conflicts.is_empty());
    assert_eq!(plan.files.len(), 2);
}

#[test]
fn without_resource_names_the_two_paths_do_not_conflict() {
    let mut deploy = resource_names();
    deploy.resource_names = false;
    let plan = run(
        &deploy,
        &[
            m("modA", true, &["a/theme.ogg"]),
            mirrored("modB", &["Data/Music/theme.ogg"]),
        ],
    );
    assert!(plan.conflicts.is_empty());
    assert_eq!(plan.files.len(), 2);
}

#[test]
fn conflicts_ignore_case_and_separators() {
    let plan = run(
        &slot_game(),
        &[
            m("modA", true, &["Content\\Scripts/A.ws"]),
            m("modB", true, &["content/scripts/a.WS"]),
        ],
    );
    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(plan.conflicts[0].winner, "modA");
}

#[test]
fn reordering_changes_the_winners() {
    let a = m("modA", true, &["content/x.ws", "content/y.ws"]);
    let b = m("modB", true, &["content/x.ws"]);
    let forward = run(&slot_game(), &[a.clone(), b.clone()]);
    let reversed = run(&slot_game(), &[b, a]);
    assert_eq!(forward.conflicts[0].winner, "modA");
    assert_eq!(reversed.conflicts[0].winner, "modB");
    assert_eq!(
        file_set(&forward),
        ["modA/content/x.ws", "modA/content/y.ws"]
    );
    assert_eq!(
        file_set(&reversed),
        ["modA/content/y.ws", "modB/content/x.ws"]
    );
}
