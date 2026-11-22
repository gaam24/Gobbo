//! Contracts of the commands the window calls: stable error codes, recovery steps and the shape of
//! optional answers. These were once asserted through the browser; each one drives the same work a
//! command does through the test data folder and the fixture games.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::*;
use crate::deploy::ops::{apply_deployment, undo_deployment};
use crate::deploy::preview::preview_deployment;
use crate::deploy::remove::{delete_mod, delete_version};
use crate::error::RecoveryAction;
use crate::infra::tasks::TaskFlags;
use crate::launch::tools::running::run_tool;
use crate::profiles::actions::{
    delete_profile, rename_profile, set_mod_priority, set_mod_version, set_profile_user_data,
    switch_profile,
};
use crate::profiles::history::preview::preview_restore;
use crate::settings::managed::{add_game, check_game_folder, remove_game};
use crate::snapshots::{
    create_manual, delete as delete_snapshot, list as list_snapshots, preview_public,
};

/// A game id no definition folder holds. The same answer is expected from every command that loads a definition.
const MISSING_GAME: &str = "missing-game";

#[test]
fn an_unknown_game_answers_a_stable_io_error() {
    let f = fixture("commands-unknown-game");

    let error = status(&f.data, MISSING_GAME).unwrap_err();
    assert_eq!(error.code, "io.failed");
    assert!(
        error.params["path"].contains(MISSING_GAME),
        "the path does not name the game: {:?}",
        error.params
    );

    let root = f.game_root.display().to_string();
    assert_eq!(
        add_game(&f.data, MISSING_GAME, &root).unwrap_err().code,
        "io.failed"
    );
    assert_eq!(
        check_game_folder(MISSING_GAME, &root).unwrap_err().code,
        "io.failed"
    );
    assert_eq!(
        remove_game(&f.data, MISSING_GAME).unwrap_err().code,
        "io.failed"
    );
    assert_eq!(
        preview_deployment(&f.data, MISSING_GAME).unwrap_err().code,
        "io.failed"
    );
    assert_eq!(
        undo_deployment(&f.data, MISSING_GAME).unwrap_err().code,
        "io.failed"
    );
    assert_eq!(
        apply_deployment(&f.data, MISSING_GAME).unwrap_err().code,
        "io.failed"
    );
    assert_eq!(
        crate::games::export(MISSING_GAME, &f.base.join("never.zip"))
            .unwrap_err()
            .code,
        "games.unknown"
    );
    assert!(list_snapshots(&f.data, MISSING_GAME).is_empty());
}

#[test]
fn a_stale_list_names_the_mod_and_profile_it_points_at() {
    // A game without a load order is the one where a priority is refused outright.
    let f = mirror_fixture("commands-stale-list");
    let active = f.active_id(MIRROR_GAME);

    assert_eq!(
        rename_profile(&f.data, MIRROR_GAME, "p99", "Nope")
            .unwrap_err()
            .code,
        "profiles.not_found"
    );
    assert_eq!(
        switch_profile(&f.data, MIRROR_GAME, "p99")
            .unwrap_err()
            .code,
        "profiles.not_found"
    );
    assert_eq!(
        delete_profile(&f.data, MIRROR_GAME, &active)
            .unwrap_err()
            .code,
        "profiles.last_profile"
    );
    assert_eq!(
        create_profile(&f.data, MIRROR_GAME, &"x".repeat(200), None, false)
            .unwrap_err()
            .code,
        "profiles.name_too_long"
    );
    assert_eq!(
        crate::library::set_mod_tags(&f.data, MIRROR_GAME, "local:nope", Vec::new())
            .unwrap_err()
            .code,
        "library.mod_missing"
    );
    assert_eq!(
        set_mod_priority(&f.data, MIRROR_GAME, "local:nope", Some(1))
            .unwrap_err()
            .code,
        "profile.priority_unsupported"
    );
    assert_eq!(
        set_mod_priority(&f.data, MIRROR_GAME, "local:nope", None)
            .unwrap_err()
            .code,
        "library.mod_missing"
    );
    assert_eq!(
        delete_mod(&f.data, MIRROR_GAME, "local:nope")
            .unwrap_err()
            .code,
        "library.mod_missing"
    );
    assert_eq!(
        set_mod_version(&f.data, MIRROR_GAME, "local:nope", "nope")
            .unwrap_err()
            .code,
        "library.version_unknown"
    );
    assert_eq!(
        delete_version(&f.data, MIRROR_GAME, "nope", None)
            .unwrap_err()
            .code,
        "library.mod_missing"
    );
    assert_eq!(
        undo_deployment(&f.data, MIRROR_GAME).unwrap_err().code,
        "deploy.nothing_to_undo"
    );
    assert_eq!(
        preview_restore(&f.data, MIRROR_GAME, 999).unwrap_err().code,
        "history.unknown"
    );
    assert_eq!(
        crate::context::profile::history::restore_list(&f.data, MIRROR_GAME, 999)
            .unwrap_err()
            .code,
        "history.unknown"
    );
}

#[test]
fn a_missing_folder_names_the_recovery_step_the_window_offers() {
    let f = fixture("commands-folder-recovery");

    f.save_settings(
        SLOT_GAME,
        Settings {
            game_root: String::new(),
            ..f.settings()
        },
    )
    .unwrap();
    let error = apply_deployment(&f.data, SLOT_GAME).unwrap_err();
    assert_eq!(error.code, "settings.game_folder_missing");
    assert_eq!(
        error.recovery.as_ref().map(|recovery| recovery.action),
        Some(RecoveryAction::SetGameFolder)
    );

    f.save_settings(
        SLOT_GAME,
        Settings {
            settings_dir: String::new(),
            ..f.settings()
        },
    )
    .unwrap();
    let error = apply_deployment(&f.data, SLOT_GAME).unwrap_err();
    assert_eq!(error.code, "settings.settings_folder_missing");
    assert_eq!(
        error.recovery.as_ref().map(|recovery| recovery.action),
        Some(RecoveryAction::SetSettingsFolder)
    );
}

#[test]
fn an_unknown_tool_is_refused_before_anything_starts() {
    let f = fixture("commands-unknown-tool");
    let tasks = TaskFlags::default();
    let error = run_tool(
        &f.data,
        SLOT_GAME,
        "nope",
        &f.base,
        Arc::new(AtomicBool::new(false)),
        tasks.handle("unknown-tool"),
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "tools.unknown");
}

/// The prerequisite command shares the tool lookup with `run_tool`; this checks its exact code on the
/// platforms where prerequisites can be installed. The shared lookup is covered on both platforms by
/// `launch::tools::running::tests::a_tool_is_found_by_its_id_and_an_unknown_one_is_an_error`.
#[cfg(not(windows))]
#[test]
fn an_unknown_tool_is_refused_before_its_prerequisites_are_checked() {
    let f = fixture("commands-unknown-prerequisites");
    let error = crate::launch::tools::prerequisites::install_prerequisites(
        &f.data, SLOT_GAME, "nope", &f.base,
    )
    .unwrap_err();
    assert_eq!(error.code, "tools.unknown");
}

#[test]
fn an_unknown_data_set_and_an_outside_path_are_refused() {
    let f = fixture("commands-outside-path");
    let active = f.active_id(SLOT_GAME);

    assert_eq!(
        set_profile_user_data(&f.data, SLOT_GAME, &active, "nope", true)
            .unwrap_err()
            .code,
        "user_data.unknown_set"
    );

    // `open_path` turns a refused path into `io.path_not_allowed` before it reaches the OS opener.
    let outside = f.base.join("outside");
    fs::create_dir_all(&outside).unwrap();
    assert!(!may_open(&f.data, Some(SLOT_GAME), &outside));
}

#[test]
fn an_empty_snapshot_is_listed_with_no_mod_or_file_counts() {
    let f = fixture("commands-empty-snapshot");
    let active = f.active_id(SLOT_GAME);

    let snapshot = create_manual(&f.data, SLOT_GAME, &active).unwrap();
    assert_eq!(snapshot.profile, active);
    assert_eq!(snapshot.mods, 0);
    assert_eq!(snapshot.files, 0);
    assert_eq!(snapshot.overwrite_files, 0);
    assert!(
        list_snapshots(&f.data, SLOT_GAME)
            .iter()
            .any(|listed| listed.id == snapshot.id),
        "the empty snapshot was not listed"
    );
    assert_eq!(
        preview_public(&f.data, SLOT_GAME, "nope").unwrap_err().code,
        "snapshot.not_found"
    );
    assert_eq!(
        delete_snapshot(&f.data, SLOT_GAME, "nope")
            .unwrap_err()
            .code,
        "snapshot.not_found"
    );
}

#[test]
fn an_optional_read_has_nothing_to_report_before_the_first_run() {
    let f = fixture("commands-optional-read");
    assert!(
        crate::launch::report::last_run(&f.data, SLOT_GAME).is_none(),
        "a game nobody launched has no report"
    );
}
