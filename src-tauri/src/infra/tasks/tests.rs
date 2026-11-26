use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::AppError;

use super::journal::INTERRUPTED;
use super::*;

/// The exported list pins the codes and their order, so a reindex cannot silently rename a named constant.
#[test]
fn task_kinds_are_the_codes_the_frontend_expects() {
    assert_eq!(
        TASK_KINDS,
        [
            "import",
            "update",
            "update-all",
            "requirement",
            "apply",
            "pack",
            "pack-link",
            "collection",
            "launch",
            "install",
            "loader",
        ]
    );
    let distinct: std::collections::HashSet<&str> = TASK_KINDS.iter().copied().collect();
    assert_eq!(distinct.len(), TASK_KINDS.len());
}

/// The builders keep the ids the running workflows already used, so no cancel command or journal changes.
#[test]
fn task_key_builders_keep_the_existing_ids() {
    assert_eq!(TaskKey::apply("game-a").as_str(), "apply:game-a");
    assert_eq!(
        TaskKey::import("game-a", "/mods/a.zip").as_str(),
        "import:game-a\n/mods/a.zip"
    );
    assert_eq!(
        TaskKey::update_mod("game-a", "CET").as_str(),
        "update-mod:game-a\nCET"
    );
    assert_eq!(TaskKey::update_all("game-a").as_str(), "update:game-a");
    assert_eq!(
        TaskKey::requirement("game-a").as_str(),
        "requirement:game-a"
    );
    assert_eq!(
        TaskKey::collection_install("game-a", "profile", "x").as_str(),
        "collection:game-a\nprofile\nx"
    );
    assert_eq!(
        TaskKey::pack_import("game-a", "recipe.zip").as_str(),
        "pack:game-a\nrecipe.zip"
    );
    assert_eq!(
        TaskKey::pack_download("game-a", "recipe.zip").as_str(),
        "pack-download:game-a\nrecipe.zip"
    );
    assert_eq!(
        TaskKey::pack_link("game-a", "nxm://link").as_str(),
        "pack-link:game-a\nnxm://link"
    );
}

/// Each builder sets the discriminator the window groups rows by, not just the string id.
#[test]
fn task_key_builders_set_the_typed_dedupe() {
    assert_eq!(
        TaskKey::apply("game-a").dedupe(),
        &TaskDedupe::Apply {
            game: "game-a".to_string()
        }
    );
    assert_eq!(
        TaskKey::import("game-a", "/mods/a.zip").dedupe(),
        &TaskDedupe::Import {
            game: "game-a".to_string(),
            path: "/mods/a.zip".to_string()
        }
    );
    assert_eq!(
        TaskKey::update_mod("game-a", "CET").dedupe(),
        &TaskDedupe::Update {
            game: "game-a".to_string()
        }
    );
    assert_eq!(
        TaskKey::update_all("game-a").dedupe(),
        &TaskDedupe::UpdateAll {
            game: "game-a".to_string()
        }
    );
    assert_eq!(
        TaskKey::requirement("game-a").dedupe(),
        &TaskDedupe::Requirement {
            game: "game-a".to_string()
        }
    );
    assert_eq!(
        TaskKey::collection_install("game-a", "profile", "x").dedupe(),
        &TaskDedupe::Collection {
            game: "game-a".to_string(),
            profile: "profile".to_string(),
            collection: "x".to_string()
        }
    );
    assert_eq!(
        TaskKey::pack_import("game-a", "recipe.zip").dedupe(),
        &TaskDedupe::Pack {
            game: "game-a".to_string(),
            path: "recipe.zip".to_string()
        }
    );
    assert_eq!(
        TaskKey::pack_download("game-a", "recipe.zip").dedupe(),
        &TaskDedupe::Pack {
            game: "game-a".to_string(),
            path: "recipe.zip".to_string()
        }
    );
    assert_eq!(
        TaskKey::pack_link("game-a", "nxm://link").dedupe(),
        &TaskDedupe::PackLink {
            game: "game-a".to_string(),
            link: "nxm://link".to_string()
        }
    );
}

/// The typed start path keeps every existing id and dedupe; the string conversion kept for tests and compatibility
/// rebuilds the same pair.
#[test]
fn task_keys_keep_their_id_and_dedupe_on_the_typed_and_string_paths() {
    let keys = [
        TaskKey::apply("game-a"),
        TaskKey::import("game-a", "/mods/a.zip"),
        TaskKey::update_mod("game-a", "CET"),
        TaskKey::update_all("game-a"),
        TaskKey::requirement("game-a"),
        TaskKey::collection_install("game-a", "profile", "x"),
        TaskKey::pack_import("game-a", "recipe.zip"),
        TaskKey::pack_download("game-a", "recipe.zip"),
        TaskKey::pack_link("game-a", "nxm://link"),
        TaskKey::tool(kind::LAUNCH, "game-a", "xedit"),
        TaskKey::tool(kind::INSTALL, "game-a", "xedit"),
    ];

    let tasks = TaskFlags::default();
    for key in keys {
        let (_, handle) = tasks.start_with_handle(key.clone(), None, kind::APPLY);
        let row = tasks
            .record(key.as_str())
            .expect("a typed key starts a record under its id");
        assert_eq!(row.id, key.as_str());
        assert_eq!(&row.dedupe, key.dedupe());
        handle.finish(state::DONE);

        let rebuilt = TaskKey::from(key.as_str().to_string());
        assert_eq!(rebuilt.as_str(), key.as_str());
        assert_eq!(rebuilt.dedupe(), key.dedupe());
    }
}

/// A key handed to `start_with_handle` is used as-is. Its id here parses to no known dedupe, so a registry that
/// re-read the string would use `Other` instead of the typed value the caller supplied.
#[test]
fn start_with_handle_uses_the_typed_dedupe_without_parsing_the_id() {
    let tasks = TaskFlags::default();
    let key = TaskKey::with_parts(
        "never-started".to_string(),
        TaskDedupe::Apply {
            game: "game-a".to_string(),
        },
    );
    let (_flag, _handle) = tasks.start_with_handle(key, Some("game-a".to_string()), kind::APPLY);
    assert_eq!(
        tasks.record("never-started").map(|row| row.dedupe),
        Some(TaskDedupe::Apply {
            game: "game-a".to_string(),
        })
    );
}

/// A pack import and a pack download keep separate flags but share the dedupe group, so only one Working row shows.
#[test]
fn a_pack_import_and_its_download_share_a_dedupe_group_but_not_a_flag() {
    let import = TaskKey::pack_import("game-a", "recipe.zip");
    let download = TaskKey::pack_download("game-a", "recipe.zip");
    assert_ne!(import.as_str(), download.as_str());
    assert_eq!(import.dedupe(), download.dedupe());
}

/// Work with no builder, such as an older journal row, still gets a typed value the window can handle.
#[test]
fn an_unknown_id_gets_the_other_dedupe() {
    assert_eq!(
        TaskKey::from("never-started".to_string()).dedupe(),
        &TaskDedupe::Other
    );
}

#[test]
fn a_flag_starts_clear_and_cancels_once() {
    let tasks = TaskFlags::default();
    let flag = tasks.start(
        "import:game:archive.zip".to_string(),
        Some("game".to_string()),
        kind::IMPORT,
    );
    assert!(!cancelled(Some(flag.as_ref())));

    tasks.cancel("import:game:archive.zip");
    assert!(cancelled(Some(flag.as_ref())));

    tasks.finish("import:game:archive.zip", state::FAILED);
    // The old Arc keeps its state; a cancel of a finished operation cannot reach it.
    assert!(cancelled(Some(flag.as_ref())));
}

#[test]
fn a_new_start_under_the_same_key_is_clear() {
    let tasks = TaskFlags::default();
    let first = tasks.start(
        "update:game".to_string(),
        Some("game".to_string()),
        kind::UPDATE,
    );
    tasks.cancel("update:game");
    tasks.finish("update:game", state::FAILED);

    let second = tasks.start(
        "update:game".to_string(),
        Some("game".to_string()),
        kind::UPDATE,
    );
    assert!(cancelled(Some(first.as_ref())));
    assert!(!cancelled(Some(second.as_ref())));
}

#[test]
fn start_lists_a_running_task_and_finish_removes_a_successful_one() {
    let tasks = TaskFlags::default();
    tasks.start(
        "apply:game".to_string(),
        Some("game".to_string()),
        kind::APPLY,
    );
    assert_eq!(
        tasks.list(),
        [TaskView {
            id: "apply:game".to_string(),
            game: Some("game".to_string()),
            kind: kind::APPLY.to_string(),
            state: state::RUNNING.to_string(),
            percent: None,
            dedupe: TaskDedupe::Apply {
                game: "game".to_string(),
            },
            error: None,
        }]
    );

    tasks.finish("apply:game", state::DONE);
    assert!(tasks.list().is_empty());
    assert_eq!(
        tasks.record("apply:game").map(|task| task.state),
        Some(state::DONE.to_string())
    );
}

#[test]
fn a_failure_stays_listed_for_the_panel() {
    let tasks = TaskFlags::default();
    tasks.start(
        "apply:game".to_string(),
        Some("game".to_string()),
        kind::APPLY,
    );
    tasks.finish("apply:game", state::FAILED);

    let tasks_view = tasks.list();
    assert_eq!(tasks_view.len(), 1);
    assert_eq!(tasks_view[0].state, state::FAILED);
}

#[test]
fn cancel_changes_the_record_and_finish_does_not_overwrite_it() {
    let tasks = TaskFlags::default();
    tasks.start(
        "collection:game:profile:mods".to_string(),
        Some("game".to_string()),
        kind::COLLECTION,
    );
    tasks.cancel("collection:game:profile:mods");
    tasks.finish("collection:game:profile:mods", state::FAILED);

    assert!(tasks.list().is_empty());
    assert_eq!(
        tasks
            .record("collection:game:profile:mods")
            .map(|task| task.state),
        Some(state::CANCELLED.to_string())
    );
}

#[test]
fn a_cancel_after_finish_leaves_the_outcome_alone() {
    let tasks = TaskFlags::default();
    tasks.start(
        "import:game:archive.zip".to_string(),
        Some("game".to_string()),
        kind::IMPORT,
    );
    tasks.finish("import:game:archive.zip", state::DONE);
    tasks.cancel("import:game:archive.zip");

    assert_eq!(
        tasks
            .record("import:game:archive.zip")
            .map(|task| task.state),
        Some(state::DONE.to_string())
    );
}

#[test]
fn cancelling_an_unknown_key_is_a_no_op() {
    let tasks = TaskFlags::default();
    tasks.cancel("never-started");
    assert!(tasks.list().is_empty());
}

#[test]
fn a_second_start_replaces_the_record_of_the_first() {
    let tasks = TaskFlags::default();
    tasks.start("update:game".to_string(), None, kind::UPDATE);
    tasks.start("update:game".to_string(), None, kind::UPDATE_ALL);

    let tasks_view = tasks.list();
    assert_eq!(tasks_view.len(), 1);
    assert_eq!(tasks_view[0].kind, kind::UPDATE_ALL);
    assert_eq!(tasks_view[0].game, None);
    tasks.finish("update:game", state::DONE);
    assert_eq!(
        tasks.record("update:game").map(|task| task.kind),
        Some(kind::UPDATE_ALL.to_string())
    );
}

#[test]
fn a_handle_reports_progress_only_for_its_own_running_task() {
    let tasks = TaskFlags::default();
    tasks.start("job-one".to_string(), None, kind::IMPORT);
    tasks.start("job-two".to_string(), None, kind::UPDATE);
    let handle = tasks.handle("job-one");

    handle.progress(40);
    assert_eq!(
        tasks.record("job-one").and_then(|task| task.percent),
        Some(40)
    );
    assert_eq!(tasks.record("job-two").and_then(|task| task.percent), None);
    assert_eq!(tasks.list()[0].percent, Some(40));
    assert_eq!(tasks.list()[1].percent, None);
}

#[test]
fn reported_progress_is_clamped_to_a_byte() {
    let tasks = TaskFlags::default();
    tasks.start("job".to_string(), None, kind::APPLY);
    let handle = tasks.handle("job");

    handle.progress(u32::MAX);
    assert_eq!(tasks.record("job").and_then(|task| task.percent), Some(100));
}

#[test]
fn progress_after_a_task_ends_cannot_revive_its_row() {
    let tasks = TaskFlags::default();
    tasks.start("job".to_string(), None, kind::APPLY);
    let handle = tasks.handle("job");
    tasks.cancel("job");

    handle.progress(60);
    assert_eq!(tasks.record("job").and_then(|task| task.percent), None);
    assert_eq!(
        tasks.record("job").map(|task| task.state),
        Some(state::CANCELLED.to_string())
    );
}

#[test]
fn a_handle_for_an_unknown_key_is_a_no_op() {
    let tasks = TaskFlags::default();
    tasks.handle("never-started").progress(50);
    assert!(tasks.list().is_empty());
}

#[test]
fn an_old_handle_cannot_settle_the_task_that_replaced_it() {
    let tasks = TaskFlags::default();
    tasks.start("update:game".to_string(), None, kind::UPDATE);
    let first = tasks.handle("update:game");

    tasks.start("update:game".to_string(), None, kind::UPDATE_ALL);
    let second = tasks.handle("update:game");

    // The replaced run's completion is ignored: no event, no flag removal and no change to the newer row.
    first.finish(state::DONE);
    first.finish_with(state::FAILED, Some(&AppError::user("update.replaced")));
    let row = tasks.record("update:game").unwrap();
    assert_eq!(row.state, state::RUNNING);
    assert_eq!(row.kind, kind::UPDATE_ALL);
    assert_eq!(row.error, None);
    assert_eq!(tasks.list().len(), 1);
    assert!(!tasks.flags_are_empty());

    second.finish(state::DONE);
    assert_eq!(
        tasks.record("update:game").map(|task| task.state),
        Some(state::DONE.to_string())
    );
    assert!(tasks.list().is_empty());
    assert!(tasks.flags_are_empty());
}

#[test]
fn an_old_tool_handle_cannot_settle_a_rerun_under_the_same_key() {
    let tasks = TaskFlags::default();
    let key: String = TaskKey::tool(kind::LAUNCH, "game", "helper").into();
    let (first_flag, first) = tasks.start_with_handle(key.clone(), None, kind::LAUNCH);
    tasks.cancel(&key);
    assert!(cancelled(Some(first_flag.as_ref())));

    // The rerun gets its own flag and a handle bound to the generation made with it.
    let (second_flag, second) = tasks.start_with_handle(key.clone(), None, kind::LAUNCH);
    assert!(!cancelled(Some(second_flag.as_ref())));

    // The old program's exit settles only its own run, leaving the rerun running.
    first.finish(state::DONE);
    assert_eq!(
        tasks.record(&key).map(|task| task.state),
        Some(state::RUNNING.to_string())
    );

    second.finish(state::DONE);
    assert_eq!(
        tasks.record(&key).map(|task| task.state),
        Some(state::DONE.to_string())
    );
}

#[test]
fn a_second_start_cancels_the_previous_flag() {
    let tasks = TaskFlags::default();
    let first = tasks.start("update:game".to_string(), None, kind::UPDATE);
    assert!(!cancelled(Some(first.as_ref())));

    let second = tasks.start("update:game".to_string(), None, kind::UPDATE);
    assert!(
        cancelled(Some(first.as_ref())),
        "the replaced run is asked to stop"
    );
    assert!(!cancelled(Some(second.as_ref())));
}

#[test]
fn a_waiting_start_makes_the_previous_run_stale() {
    let tasks = TaskFlags::default();
    let flag = tasks.start("update:game".to_string(), None, kind::UPDATE);
    let first = tasks.handle("update:game");
    tasks.start_waiting("update:game".to_string(), None, kind::UPDATE);
    let second = tasks.handle("update:game");

    first.finish(state::DONE);
    assert_eq!(
        tasks.record("update:game").map(|task| task.state),
        Some(state::WAITING.to_string())
    );
    assert!(cancelled(Some(flag.as_ref())));

    second.finish(state::DONE);
    assert_eq!(
        tasks.record("update:game").map(|task| task.state),
        Some(state::DONE.to_string())
    );
}

/// A registry with a journal in a scratch folder, standing in for one app start.
fn with_journal(name: &str) -> (PathBuf, TaskFlags) {
    let dir = crate::test_support::scratch!(name);
    let tasks = TaskFlags::with_journal(dir.clone());
    (dir, tasks)
}

#[test]
fn a_journal_round_trip_marks_interrupted_work_as_a_failure() {
    let (dir, tasks) = with_journal("tasks-journal-interrupted");
    tasks.start(
        "apply:game".to_string(),
        Some("game".to_string()),
        kind::APPLY,
    );
    tasks.start_waiting(
        "collection:game:p1:c".to_string(),
        Some("game".to_string()),
        kind::COLLECTION,
    );
    tasks.start(
        "import:game:a.zip".to_string(),
        Some("game".to_string()),
        kind::IMPORT,
    );
    tasks.finish("import:game:a.zip", state::FAILED);
    drop(tasks);

    // A new registry over the same folder stands in for the next start.
    let restarted = TaskFlags::with_journal(dir);
    let rows = restarted.list();
    assert_eq!(rows.len(), 3);
    let by_id: HashMap<String, TaskView> =
        rows.into_iter().map(|row| (row.id.clone(), row)).collect();
    assert_eq!(
        by_id.get("apply:game").map(|row| row.state.as_str()),
        Some(state::FAILED),
        "interrupted running work is a failure with a stable state code"
    );
    assert_eq!(
        by_id
            .get("apply:game")
            .and_then(|row| row.error.as_ref())
            .map(|error| error.code.as_str()),
        Some(INTERRUPTED),
        "interrupted work says why it failed"
    );
    assert_eq!(
        by_id
            .get("collection:game:p1:c")
            .map(|row| row.state.as_str()),
        Some(state::WAITING),
        "waiting work survives a restart unchanged"
    );
    assert_eq!(
        by_id.get("import:game:a.zip").map(|row| row.state.as_str()),
        Some(state::FAILED)
    );
    assert!(
        restarted.flags_are_empty(),
        "no cancellation flag is restored for work of an earlier process"
    );
}

#[test]
fn a_failure_keeps_its_reason_until_the_user_dismisses_it() {
    let (dir, tasks) = with_journal("tasks-journal-dismiss");
    tasks.start(
        "import:game:a.zip".to_string(),
        Some("game".to_string()),
        kind::IMPORT,
    );
    tasks.finish_with(
        "import:game:a.zip",
        state::FAILED,
        Some(&AppError::user("import.archive_damaged").with("file", "a.zip")),
    );
    let error = tasks.list()[0].error.clone().unwrap();
    assert_eq!(error.code, "import.archive_damaged");
    assert_eq!(error.params.get("file").map(String::as_str), Some("a.zip"));

    tasks.dismiss("import:game:a.zip");
    assert!(tasks.list().is_empty());
    drop(tasks);
    assert!(
        TaskFlags::with_journal(dir).list().is_empty(),
        "a dismissed failure does not come back after a restart"
    );
}

#[test]
fn a_running_task_cannot_be_dismissed() {
    let tasks = TaskFlags::default();
    tasks.start("apply:game".to_string(), None, kind::APPLY);
    tasks.dismiss("apply:game");
    assert_eq!(tasks.list().len(), 1);
}

#[test]
fn a_finished_task_is_not_restored_after_a_restart() {
    let (dir, tasks) = with_journal("tasks-journal-finished");
    tasks.start(
        "apply:game".to_string(),
        Some("game".to_string()),
        kind::APPLY,
    );
    tasks.finish("apply:game", state::DONE);
    drop(tasks);

    assert!(TaskFlags::with_journal(dir).list().is_empty());
}

#[test]
fn a_waiting_start_is_listed_but_has_no_cancel_flag() {
    let tasks = TaskFlags::default();
    tasks.start_waiting(
        "waiting:game".to_string(),
        Some("game".to_string()),
        kind::COLLECTION,
    );

    let rows = tasks.list();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].state, state::WAITING);
    assert!(tasks.record("waiting:game").is_some());
    // Cancel cannot reach waiting work, and finishing it removes the row from the panel.
    tasks.cancel("waiting:game");
    assert_eq!(tasks.list()[0].state, state::WAITING);
    tasks.finish("waiting:game", state::DONE);
    assert!(tasks.list().is_empty());
}
