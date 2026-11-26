use super::decision::{Decision, DecisionMode};
use super::*;
use crate::domain::storage::StorageMode;
use crate::test_support::scratch;
use std::fs;
use std::path::{Path, PathBuf};

fn dirs(name: &str) -> (PathBuf, PathBuf) {
    let root = scratch!(name);
    let exe = root.join("program");
    let global = root.join("global");
    fs::create_dir_all(&exe).unwrap();
    (exe, global)
}

fn write_decision_file(path: &Path, version: u32, mode: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!(r#"{{"version":{version},"mode":"{mode}"}}"#)).unwrap();
}

fn known(resolved: Resolved) -> StorageLocation {
    match resolved {
        Resolved::Known(location) => location,
        Resolved::Choice(_) => panic!("expected a known location"),
    }
}

fn choice(resolved: Resolved) -> PromptContext {
    match resolved {
        Resolved::Choice(context) => context,
        Resolved::Known(_) => panic!("expected a question"),
    }
}

#[test]
fn a_custom_folder_wins_and_has_no_decision_file() {
    let (exe, global) = dirs("storage-custom");
    let custom = exe.join("elsewhere");
    let resolved = resolve(&exe, Some(&global), Some(custom.clone()));
    let location = known(resolved);
    assert_eq!(location.mode, StorageMode::Custom);
    assert_eq!(location.root, custom);
    assert!(location.decision_file.as_os_str().is_empty());
    assert!(location.tauri_root().is_some());
}

#[test]
fn a_local_portable_decision_wins() {
    let (exe, global) = dirs("storage-portable");
    write_decision_file(&local_decision_file(&exe), 1, "portable");
    let resolved = resolve(&exe, Some(&global), None);
    let location = known(resolved);
    assert_eq!(location.mode, StorageMode::Portable);
    assert_eq!(location.root, portable_root(&exe));
    assert_eq!(location.decision_file, local_decision_file(&exe));
    assert_eq!(location.tauri_root(), Some(portable_root(&exe)));
}

#[test]
fn a_global_decision_in_the_data_folder_is_used() {
    let (exe, global) = dirs("storage-global-decision");
    write_decision_file(&global_decision_file(&global), 1, "global");
    let resolved = resolve(&exe, Some(&global), None);
    let location = known(resolved);
    assert_eq!(location.mode, StorageMode::Global);
    assert_eq!(location.root, global);
    assert!(location.tauri_root().is_none());
}

#[test]
fn a_local_global_decision_wins_over_the_global_file() {
    let (exe, global) = dirs("storage-local-global");
    write_decision_file(&local_decision_file(&exe), 1, "global");
    write_decision_file(&global_decision_file(&global), 1, "portable");
    let resolved = resolve(&exe, Some(&global), None);
    let location = known(resolved);
    assert_eq!(location.mode, StorageMode::Global);
    assert_eq!(location.decision_file, local_decision_file(&exe));
}

#[test]
fn a_portable_mode_in_the_global_file_is_ignored() {
    let (exe, global) = dirs("storage-global-portable");
    write_decision_file(&global_decision_file(&global), 1, "portable");
    assert!(matches!(
        resolve(&exe, Some(&global), None),
        Resolved::Choice(_)
    ));
}

#[test]
fn a_damaged_decision_is_kept_aside_and_the_question_returns() {
    let (exe, global) = dirs("storage-broken");
    let path = local_decision_file(&exe);
    fs::write(&path, "{ not json").unwrap();
    assert!(matches!(
        resolve(&exe, Some(&global), None),
        Resolved::Choice(_)
    ));
    assert!(path.with_extension("json.broken").is_file());
}

#[test]
fn a_marked_portable_copy_defaults_to_portable_without_global_data() {
    let (exe, global) = dirs("storage-default-portable");
    fs::write(portable_marker(&exe), "").unwrap();
    let context = choice(resolve(&exe, Some(&global), None));
    assert!(context.portable_marker);
    assert!(context.portable_available);
    assert!(!context.global_data_exists);
    assert_eq!(context.default_mode, StorageMode::Portable);
}

#[test]
fn an_installed_copy_defaults_to_global() {
    let (exe, global) = dirs("storage-default-global");
    let context = choice(resolve(&exe, Some(&global), None));
    assert!(!context.portable_marker);
    assert_eq!(context.default_mode, StorageMode::Global);
}

#[test]
fn a_games_folder_counts_as_existing_data() {
    let root = scratch!("storage-existing-games");
    fs::create_dir_all(root.join("games").join("slot")).unwrap();
    assert!(has_existing_data(&root));
}

#[test]
fn existing_global_data_keeps_global_as_the_default() {
    let (exe, global) = dirs("storage-default-existing");
    fs::write(portable_marker(&exe), "").unwrap();
    fs::create_dir_all(&global).unwrap();
    fs::write(global.join("settings.json"), "{}").unwrap();
    let context = choice(resolve(&exe, Some(&global), None));
    assert!(context.global_data_exists);
    assert_eq!(context.default_mode, StorageMode::Global);
}

#[test]
fn a_writable_program_folder_is_detected() {
    let (exe, _global) = dirs("storage-writable");
    assert!(can_write(&exe));
    assert!(fs::read_dir(&exe).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("gobbo-write")
    }));
}

#[test]
fn a_file_cannot_be_used_as_a_program_folder() {
    let root = scratch!("storage-not-a-dir");
    let file = root.join("program");
    fs::write(&file, "x").unwrap();
    assert!(!can_write(&file));
}

#[test]
fn committing_portable_creates_the_data_folder_and_the_local_decision() {
    let (exe, global) = dirs("storage-commit-portable");
    let context = choice(resolve(&exe, Some(&global), None));
    let location = commit(&context, StorageMode::Portable).unwrap();
    assert_eq!(location.mode, StorageMode::Portable);
    assert!(portable_root(&exe).is_dir());
    assert!(local_decision_file(&exe).is_file());
    let saved: Decision =
        serde_json::from_str(&fs::read_to_string(local_decision_file(&exe)).unwrap()).unwrap();
    assert_eq!(saved.version, 1);
    assert_eq!(saved.mode, DecisionMode::Portable);
}

#[test]
fn committing_global_prefers_the_local_decision() {
    let (exe, global) = dirs("storage-commit-global");
    let context = choice(resolve(&exe, Some(&global), None));
    let location = commit(&context, StorageMode::Global).unwrap();
    assert_eq!(location.mode, StorageMode::Global);
    assert_eq!(location.root, global);
    assert_eq!(location.decision_file, local_decision_file(&exe));
    assert!(global.is_dir());
    assert!(local_decision_file(&exe).is_file());
}

#[test]
fn committing_global_falls_back_to_the_global_decision() {
    let (exe, global) = dirs("storage-commit-global-fallback");
    let mut context = choice(resolve(&exe, Some(&global), None));
    // Make the local write fail by putting a file where its parent folder should be.
    context.local_decision_file = exe.join("blocked").join(DECISION_FILE);
    fs::write(exe.join("blocked"), "x").unwrap();
    let location = commit(&context, StorageMode::Global).unwrap();
    assert_eq!(location.decision_file, global_decision_file(&global));
    assert!(global_decision_file(&global).is_file());
}

#[test]
fn committing_portable_refuses_a_read_only_folder() {
    let (exe, global) = dirs("storage-commit-read-only");
    let mut context = choice(resolve(&exe, Some(&global), None));
    context.portable_available = false;
    assert_eq!(
        commit(&context, StorageMode::Portable).unwrap_err().code,
        "storage.portable_unavailable"
    );
}

#[test]
fn the_cli_default_matches_the_first_question() {
    let (exe, global) = dirs("storage-cli-default");
    fs::write(portable_marker(&exe), "").unwrap();
    let context = choice(resolve(&exe, Some(&global), None));
    assert_eq!(default_root(&context).unwrap(), portable_root(&exe));
    // The default does not write a decision; the first window start still asks.
    assert!(!local_decision_file(&exe).exists());
}
