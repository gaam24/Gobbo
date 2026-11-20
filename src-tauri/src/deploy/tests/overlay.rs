//! Overlay tests (Linux only): the layer the game sees, the writes of a session, and the refusals.

use super::*;
use crate::deploy::overlay::{
    Session, command, execute, in_use, matches_plan, run, run_step, session_pids,
};
use crate::games::{Phase, Step};
use crate::test_support::require_overlay_namespaces;

/// The overlay session of the fixture: its layers in the state folder, over the game folder.
fn overlay_session(f: &Fixture) -> Session {
    Session::at(&f.target.state, &f.root)
}

/// Plans the folders as enabled mods of `slot`.
fn overlay_plan(f: &Fixture, slot: &str, folders: &[&str]) -> DeployPlan {
    f.plan(&profile(folders, slot)).unwrap()
}

/// Applies a plan to the overlay layer.
fn overlay_apply(f: &Fixture, plan: &DeployPlan) -> ApplyReport {
    apply(plan, &f.library, &f.target).unwrap()
}

/// Runs `script` in the session with the game folder as `$1` and returns whether it succeeded.
fn in_session(f: &Fixture, script: &str) -> bool {
    let args: Vec<String> = vec![
        "-c".into(),
        script.into(),
        "probe".into(),
        f.root.display().to_string(),
    ];
    command(&overlay_session(f), "sh", &args)
        .status()
        .unwrap()
        .success()
}

/// A slot game with a foreign merger output and a file in `bin/`, next to the library with mod A.
fn slots_fixture(name: &str) -> Fixture {
    let base = scratch!(name);
    let f = fixture_in(
        &base,
        crate::test_support::games::slot().deploy,
        Some(base.join("settings/mods.settings")),
    );
    write(
        &f.root.join("Mods/mod0000_MergedFiles/content/merged.ws"),
        "merged",
    );
    write(&f.root.join("bin/game.txt"), "game");
    write(
        f.target.settings.as_deref().unwrap(),
        "[mod0000_MergedFiles]\r\nEnabled=1\r\nPriority=1\r\n",
    );
    write(&f.library.join("modA/content/x.ws"), "from A");
    f
}

#[test]
fn apply_with_overlay_stages_mods_and_leaves_the_game_folder_alone() {
    let f = slots_fixture("stage");
    let (files_before, settings_path, settings_before) = snapshot(&f);

    let report = overlay_apply(&f, &overlay_plan(&f, "Mods", &["modA"]));
    assert_eq!(report.files, 1);
    let session = overlay_session(&f);
    assert_eq!(
        fs::read_to_string(session.mods.join("Mods/modA/content/x.ws")).unwrap(),
        "from A"
    );
    assert_eq!(scan::list_files(&f.root).unwrap(), files_before);
    // Without a priority the user set, the order file stays as it was.
    assert_eq!(settings::read(&settings_path).unwrap(), settings_before);

    let warnings = undo(&f.target.journal_roots()).unwrap();
    assert!(warnings.is_empty());
    assert!(session.mods.exists());
    assert_eq!(settings::read(&settings_path).unwrap(), settings_before);
}

#[test]
fn mirror_stages_files_at_their_game_paths() {
    let f = mirror_fixture("stage-mirror");
    overlay_apply(&f, &overlay_plan(&f, "", &["modA"]));
    assert!(
        overlay_session(&f)
            .mods
            .join("archive/pc/mod/a.archive")
            .is_file()
    );
    assert!(!f.root.join("archive").exists());
    undo(&f.target.journal_roots()).unwrap();
}

#[test]
fn writes_made_by_the_game_in_a_session_land_in_the_upper_layer() {
    require_overlay_namespaces!();
    let f = slots_fixture("session");
    overlay_apply(&f, &overlay_plan(&f, "Mods", &["modA"]));
    let session = overlay_session(&f);

    // "Game" in the session sees the mod from the layer and writes the file to its own folder.
    assert!(in_session(
        &f,
        r#"test -f "$1/Mods/modA/content/x.ws" && echo written-in-session > "$1/bin/probe.txt""#
    ));
    assert!(!f.root.join("bin/probe.txt").exists());
    assert!(session.overwrite.join("bin/probe.txt").exists());

    // The game's writes stay, so undo warns about them. The mod layer stays for the next deployment.
    let warnings = undo(&f.target.journal_roots()).unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(session.mods.exists());
}

#[test]
fn mirror_session_shows_mod_files_at_game_paths() {
    require_overlay_namespaces!();
    let f = mirror_fixture("session-mirror");
    overlay_apply(&f, &overlay_plan(&f, "", &["modA"]));
    assert!(in_session(
        &f,
        r#"test -f "$1/archive/pc/mod/a.archive" && test -f "$1/bin/x64/game.exe""#
    ));
    assert!(!f.root.join("archive").exists());
    undo(&f.target.journal_roots()).unwrap();
}

#[test]
fn a_mod_file_covers_the_game_file_in_the_session_only() {
    require_overlay_namespaces!();
    let f = mirror_fixture("cover");
    write(&f.root.join("archive/pc/mod/a.archive"), "game's own");
    let plan = overlay_plan(&f, "", &["modA"]);
    assert!(plan.problems.is_empty(), "{:?}", plan.problems);
    overlay_apply(&f, &plan);

    assert!(in_session(
        &f,
        r#"grep -q "from A" "$1/archive/pc/mod/a.archive""#
    ));
    assert_eq!(
        fs::read_to_string(f.root.join("archive/pc/mod/a.archive")).unwrap(),
        "game's own"
    );
    undo(&f.target.journal_roots()).unwrap();
}

#[test]
fn a_root_inside_the_game_folder_is_a_folder_inside_the_layer() {
    require_overlay_namespaces!();
    let mut f = mirror_fixture("inner-root");
    f.target.root = f.root.join("Data");
    let plan = overlay_plan(&f, "", &["modA"]);
    overlay_apply(&f, &plan);

    let session = overlay_session(&f);
    assert!(session.mods.join("Data/archive/pc/mod/a.archive").is_file());
    assert!(in_session(
        &f,
        r#"test -f "$1/Data/archive/pc/mod/a.archive""#
    ));
    assert!(!f.root.join("Data").exists());
    undo(&f.target.journal_roots()).unwrap();

    // The same plan reuses the layer.
    assert!(matches_plan(&plan, &f.library, &session.mods, Path::new("Data")).unwrap());
}

#[test]
fn a_staging_left_by_undo_is_reused_when_it_matches_the_plan() {
    let f = slots_fixture("reuse");
    let plan = overlay_plan(&f, "Mods", &["modA"]);
    overlay_apply(&f, &plan);
    undo(&f.target.journal_roots()).unwrap();

    let session = overlay_session(&f);
    assert!(matches_plan(&plan, &f.library, &session.mods, Path::new("")).unwrap());

    let again = overlay_apply(&f, &plan);
    assert_eq!(again.files, 1);
    assert_eq!(
        fs::read_to_string(session.mods.join("Mods/modA/content/x.ws")).unwrap(),
        "from A"
    );
    undo(&f.target.journal_roots()).unwrap();
}

#[test]
fn a_staging_with_other_files_is_not_reused() {
    let f = slots_fixture("other-plan");
    overlay_apply(&f, &overlay_plan(&f, "Mods", &["modA"]));
    undo(&f.target.journal_roots()).unwrap();
    let other = overlay_plan(&f, "Mods", &[]);
    assert!(!matches_plan(&other, &f.library, &overlay_session(&f).mods, Path::new("")).unwrap());
}

#[test]
fn a_rebuilt_layer_starts_without_the_whiteouts_of_the_old_one() {
    let f = slots_fixture("new-layer");
    let plan = overlay_plan(&f, "Mods", &["modA"]);
    overlay_apply(&f, &plan);
    let whiteouts = overlay_session(&f).layers().whiteouts_file();
    let journal = overlay_session(&f).layers().whiteouts_journal_file();
    fs::write(&whiteouts, "mods\\moda\\content\\x.ws\n").unwrap();
    fs::write(&journal, "mods\\moda\\content\\y.ws\n").unwrap();
    undo(&f.target.journal_roots()).unwrap();

    // The same mods reuse the layer, and its whiteouts stay with it.
    overlay_apply(&f, &plan);
    assert!(whiteouts.is_file());
    assert!(journal.is_file());
    undo(&f.target.journal_roots()).unwrap();

    // Other mods build the layer again, so the old whiteouts go.
    overlay_apply(&f, &overlay_plan(&f, "Mods", &[]));
    assert!(!whiteouts.exists());
    assert!(!journal.exists());
    undo(&f.target.journal_roots()).unwrap();
}

#[test]
fn running_without_an_overlay_session_is_refused() {
    let f = slots_fixture("no-session");
    let err = run(&overlay_session(&f), "true", &[]).unwrap_err();
    assert!(matches!(err, DeployError::NoOverlaySession));
}

/// Starts `sleep` in the session and returns once the overlay is in use. The namespace takes a moment to mount, so the
/// check waits for it.
fn sleeping_in_session(f: &Fixture) -> std::process::Child {
    let child = command(&overlay_session(f), "sleep", &["30".to_string()])
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);

    while !in_use(&f.root) {
        assert!(
            std::time::Instant::now() < deadline,
            "the overlay was never in use"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    child
}

#[test]
fn the_overlay_is_in_use_while_a_process_runs_in_it() {
    require_overlay_namespaces!();
    let f = slots_fixture("in-use");
    overlay_apply(&f, &overlay_plan(&f, "Mods", &["modA"]));
    assert!(!in_use(&f.root));

    let mut child = sleeping_in_session(&f);
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(!in_use(&f.root));
    undo(&f.target.journal_roots()).unwrap();
}

#[test]
fn a_layer_is_not_rebuilt_while_the_game_runs_from_it() {
    require_overlay_namespaces!();
    let f = slots_fixture("busy");
    overlay_apply(&f, &overlay_plan(&f, "Mods", &["modA"]));
    undo(&f.target.journal_roots()).unwrap();

    let mut child = sleeping_in_session(&f);
    let other = overlay_plan(&f, "Mods", &[]);
    let refused = apply(&other, &f.library, &f.target);
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(matches!(refused, Err(DeployError::OverlayInUse { .. })));
}

#[test]
fn a_skipped_step_clears_its_outputs_from_the_overwrite_layer() {
    let f = slots_fixture("skipped-step");
    overlay_apply(&f, &overlay_plan(&f, "Mods", &["modA"]));
    let session = overlay_session(&f);
    let cache = session.overwrite.join("cache/final.bin");
    write(&cache, "stale");
    let step = Step {
        id: "compile".into(),
        phase: Phase::BeforeStart,
        program: "true".into(),
        args: Vec::new(),
        when_dir: Some("scripts".into()),
        clean: vec!["cache/final.bin".into()],
        outputs: Vec::new(),
        log: None,
    };

    // The mods have no `scripts`, so the step does not run, but the cache it would rebuild is removed.
    assert!(!run_step(&session.state, &session, &step, None).unwrap());
    assert!(!cache.exists());
    undo(&f.target.journal_roots()).unwrap();
}

#[test]
fn a_stop_ends_the_session_processes_that_name_no_game_file() {
    require_overlay_namespaces!();
    let f = slots_fixture("stop-session");
    overlay_apply(&f, &overlay_plan(&f, "Mods", &["modA"]));
    let mut child = sleeping_in_session(&f);
    // The `sleep` names no file of the game, so only the session finds it, as it finds Wine's services.
    assert!(session_pids(&f.root).contains(&child.id()));

    assert!(crate::platform::stop_game(&f.root));
    assert!(!child.wait().unwrap().success());
    assert!(!in_use(&f.root));
    undo(&f.target.journal_roots()).unwrap();
}

#[test]
fn a_session_starts_when_its_work_folder_is_missing() {
    require_overlay_namespaces!();
    let f = slots_fixture("no-work");
    overlay_apply(&f, &overlay_plan(&f, "Mods", &["modA"]));
    let session = overlay_session(&f);
    std::fs::remove_dir_all(&session.work).unwrap();

    // The state folder lost its work folder: the start creates it before the mount names it.
    assert_eq!(execute(&session, "true", &[], &[]).unwrap(), 0);
    assert!(session.work.is_dir());
    undo(&f.target.journal_roots()).unwrap();
}

#[test]
fn a_path_with_a_mount_separator_is_refused_before_anything_is_staged() {
    let mut f = slots_fixture("unsafe");
    f.target.root = f.root.with_file_name("game,with-comma");
    f.target.game_root = f.target.root.clone();
    let plan = overlay_plan(&f, "Mods", &["modA"]);

    let err = apply(&plan, &f.library, &f.target).unwrap_err();
    assert!(matches!(err, DeployError::UnsafePath { .. }));
    assert!(!f.target.state.join("mods").exists());
}
