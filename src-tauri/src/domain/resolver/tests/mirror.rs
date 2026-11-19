//! Mirrored files: their game paths, one winner per path, and mods with no folder of their own.

use super::*;

#[test]
fn mirrored_files_keep_their_game_paths_and_join_the_mod() {
    let mods = [m(
        "modA",
        true,
        &[
            "content/a.ws",
            "DLC/dlcA/blob0.bundle",
            "bin/config/r4game/menu.xml",
        ],
    )];
    let plan = run(&slot_game(), &mods);
    assert_eq!(
        dests(&plan),
        [
            "DLC/dlcA/blob0.bundle",
            "Mods/modA/content/a.ws",
            "bin/config/r4game/menu.xml"
        ]
    );
    assert!(
        plan.files
            .iter()
            .all(|f| f.game_path != f.dest.starts_with("Mods/"))
    );
    assert!(plan.conflicts.is_empty());
    assert!(plan.problems.is_empty());
}

#[test]
fn a_mod_of_mirrored_files_only_has_no_folder_in_the_slot() {
    let mods = [ModInput {
        priority: Some(3),
        ..m("dlcOnly", true, &["DLC/dlcOnly/blob0.bundle"])
    }];
    let plan = run(&slot_game(), &mods);
    assert!(plan.settings.is_empty());
    assert_eq!(dests(&plan), ["DLC/dlcOnly/blob0.bundle"]);
    assert!(plan.files[0].game_path);
}

#[test]
fn a_mirrored_only_layout_and_the_file_scan_agree() {
    let deploy = mirror_game();
    let files: std::sync::Arc<[String]> = ["archive/pc/mod/x.archive", "r6/scripts/a.reds"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into();
    let profile = crate::domain::resolver::layout_profile(&deploy, &files);
    assert!(profile.mirrored_only);

    let mut with_layout = mirrored("modA", &["ignored"]);
    with_layout.files = std::sync::Arc::clone(&files);
    with_layout.layout = Some(&profile);
    let mut without_layout = with_layout.clone();
    without_layout.layout = None;

    assert_eq!(
        run(&deploy, &[with_layout]),
        run(&deploy, &[without_layout])
    );
}

#[test]
fn a_mirrored_file_of_two_mods_has_one_winner() {
    let mods = [
        m("modA", true, &["bin/config/r4game/x.xml"]),
        m("modB", true, &["bin/config/r4game/x.xml"]),
    ];
    let plan = run(&slot_game(), &mods);
    assert_eq!(file_set(&plan), ["modA/bin/config/r4game/x.xml"]);
    assert_eq!(plan.conflicts[0].winner, "modA");
    assert_eq!(plan.conflicts[0].losers, ["modB"]);

    let chosen = choosing(&slot_game(), &mods, "bin/config/r4game/x.xml", "modB");
    assert_eq!(file_set(&chosen), ["modB/bin/config/r4game/x.xml"]);
}

#[test]
fn mirror_plants_only_the_winner_on_disk() {
    let plan = run(
        &mirror_game(),
        &[
            mirrored("modTop", &["archive/pc/mod/x.archive"]),
            mirrored(
                "modBottom",
                &["archive/pc/mod/x.archive", "r6/scripts/b.reds"],
            ),
        ],
    );
    assert_eq!(
        file_set(&plan),
        [
            "modBottom/r6/scripts/b.reds",
            "modTop/archive/pc/mod/x.archive"
        ]
    );
    assert_eq!(plan.conflicts[0].winner, "modTop");
    assert_eq!(plan.conflicts[0].losers, ["modBottom"]);
}

/// The slot game with `winhttp.dll` as a root file: a loader's proxy that keeps its name in the game folder.
fn root_file_game() -> Deploy {
    let mut deploy = slot_game();
    deploy.root_files = vec!["winhttp.dll".to_string()];
    deploy
}

#[test]
fn a_root_file_keeps_its_name_in_the_game_folder() {
    let mods = [mirrored(
        "loader",
        &["winhttp.dll", "BepInEx/core/BepInEx.dll"],
    )];
    let plan = run(&root_file_game(), &mods);
    assert!(plan.problems.is_empty());
    assert!(plan.conflicts.is_empty());
    assert!(
        plan.files
            .iter()
            .any(|f| f.dest == "winhttp.dll" && f.game_path)
    );
    assert!(
        plan.files
            .iter()
            .any(|f| f.dest == "BepInEx/core/BepInEx.dll" && f.game_path)
    );
}

#[test]
fn a_root_file_is_planned_in_the_summary_too() {
    let mods = [mirrored("loader", &["winhttp.dll"])];
    let plan = plan_with_mode(
        &root_file_game(),
        &mods,
        &BTreeMap::new(),
        PlanMode::Summary,
    );
    assert_eq!(plan.file_count, 1);
    assert_eq!(dests(&plan), ["winhttp.dll"]);
}

#[test]
fn two_mods_with_the_same_root_file_have_one_winner() {
    let deploy = root_file_game();
    let mods = [
        mirrored("proxyA", &["winhttp.dll"]),
        mirrored("proxyB", &["winhttp.dll"]),
    ];
    let plan = run(&deploy, &mods);
    assert_eq!(file_set(&plan), ["proxyA/winhttp.dll"]);
    assert_eq!(plan.conflicts[0].winner, "proxyA");
    assert_eq!(plan.conflicts[0].losers, ["proxyB"]);

    let chosen = choosing(&deploy, &mods, "winhttp.dll", "proxyB");
    assert_eq!(file_set(&chosen), ["proxyB/winhttp.dll"]);
}
