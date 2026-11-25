//! Applying and undoing a deployment: what comes back, what is kept, and how the layer covers the game.

use super::*;
use crate::context::data::DEPLOYMENT_FILE;
use crate::domain::user_data::{Kind, Mode, Set as UserDataSet};
use std::io;

/// Applies modA with a priority the user set, so the settings file gets a row for it. Returns the settings file.
fn apply_with_priority(f: &Fixture) -> &Path {
    let mut list = mods(&["modA"]);
    list[0].priority = Some(1);
    f.apply(&list).unwrap();
    f.target.settings.as_deref().unwrap()
}

#[test]
fn a_redirect_settings_file_starts_from_the_lower_copy() {
    let base = scratch!("redirect-settings");
    let real = base.join("documents/Game");
    let upper = base.join("state/userdata/documents/upper");
    fs::create_dir_all(&real).unwrap();
    fs::create_dir_all(&upper).unwrap();
    fs::write(
        real.join("mods.settings"),
        "[modOld]\nEnabled=1\nPriority=7\n",
    )
    .unwrap();
    let target = Target {
        root: base.join("game"),
        game_root: base.join("game"),
        settings: Some(upper.join("mods.settings")),
        order_file: None,
        state: base.join("state"),
        user_data: vec![UserDataSet {
            id: "documents".into(),
            kind: Kind::Settings,
            real: real.clone(),
            mode: Mode::Redirect,
            isolated: true,
            inherit: true,
            base: real.clone(),
            upper: upper.clone(),
            store: base.join("copy"),
        }],
    };
    let journal = Journal::new(target.state.join(JOURNAL_FILE));
    write_settings(
        &journal,
        &target,
        &[SettingsRow {
            folder: "modNew".into(),
            enabled: true,
            priority: 2,
        }],
    )
    .unwrap();
    let text = fs::read_to_string(upper.join("mods.settings")).unwrap();
    assert!(text.contains("[modOld]"), "{text}");
    assert!(text.contains("[modNew]"), "{text}");
    assert_eq!(
        fs::read_to_string(real.join("mods.settings")).unwrap(),
        "[modOld]\nEnabled=1\nPriority=7\n"
    );
}

#[test]
fn apply_then_undo_leaves_the_game_folder_as_it_was() {
    let f = fixture("apply-undo");
    let (files_before, settings_path, settings_before) = snapshot(&f);

    // Only a mod with a priority the user set gets a row in the settings file.
    let mut list = mods(&["modA", "modB"]);
    list[0].priority = Some(1);
    let report = f.apply(&list).unwrap();
    assert_eq!(report.files, 3);
    assert!(f.target.state.join(DEPLOYMENT_FILE).is_file());

    // The overlay layer holds the mods; the game folder keeps its own files.
    let deployed = f.target.state.join("mods/Mods/modA/content/x.ws");
    assert_eq!(fs::read_to_string(&deployed).unwrap(), "from A");
    #[cfg(unix)]
    assert!(same_file(&deployed, &f.library.join("modA/content/x.ws")));
    assert_eq!(scan::list_files(&f.root).unwrap(), files_before);

    let settings_now = settings::read(&settings_path).unwrap().unwrap();
    assert!(settings_now.contains("[modA]\r\nEnabled=1\r\nPriority=1\r\n"));
    assert!(!settings_now.contains("[modB]"));
    assert!(settings_now.contains("[mod0000_MergedFiles]\r\nEnabled=1\r\nPriority=1\r\n"));

    let warnings = undo(&f.target.journal_roots()).unwrap();
    assert!(warnings.is_empty());
    // The layer stays for the next deployment of this profile, like `overwrite/`.
    assert!(deployed.is_file());
    assert_eq!(scan::list_files(&f.root).unwrap(), files_before);
    assert_eq!(settings::read(&settings_path).unwrap(), settings_before);
    assert!(!f.target.state.join(JOURNAL_FILE).exists());
    assert!(!f.target.state.join(DEPLOYMENT_FILE).exists());
}

#[test]
fn a_foreign_folder_with_the_same_name_is_covered_by_the_layer() {
    let f = fixture("foreign-covered");
    write(
        &f.library.join("mod0000_MergedFiles/content/merged.ws"),
        "mine",
    );

    let report = f.apply(&mods(&["mod0000_MergedFiles"])).unwrap();
    assert_eq!(report.files, 1);
    assert_eq!(
        fs::read_to_string(
            f.target
                .state
                .join("mods/Mods/mod0000_MergedFiles/content/merged.ws")
        )
        .unwrap(),
        "mine"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("Mods/mod0000_MergedFiles/content/merged.ws")).unwrap(),
        "merged",
        "the game's own folder stays untouched"
    );
}

#[test]
fn undo_keeps_what_the_game_wrote_in_other_sections() {
    let f = fixture("settings-changed");
    let settings_path = apply_with_priority(&f);
    let after_apply = settings::read(settings_path).unwrap().unwrap();
    write(
        settings_path,
        &format!("{after_apply}[modGame]\nEnabled=1\nPriority=5\n"),
    );

    undo(&f.target.journal_roots()).unwrap();
    assert!(!f.root.join("Mods/modA").exists());
    let text = settings::read(settings_path).unwrap().unwrap();
    assert!(!text.contains("[modA]"), "{text}");
    assert!(
        text.contains("[modGame]\r\nEnabled=1\r\nPriority=5\r\n"),
        "{text}"
    );
    assert!(text.contains("[mod0000_MergedFiles]"), "{text}");
    assert!(!f.target.state.join(JOURNAL_FILE).exists());
}

#[test]
fn undo_does_not_bring_back_a_settings_file_the_user_emptied() {
    let f = fixture("settings-emptied");
    let settings_path = apply_with_priority(&f);
    write(settings_path, "");

    undo(&f.target.journal_roots()).unwrap();
    assert_eq!(
        settings::read(settings_path).unwrap().unwrap_or_default(),
        ""
    );
    assert!(!f.root.join("Mods/modA").exists());
}

#[test]
fn a_deployment_without_priorities_leaves_the_settings_file_alone() {
    let f = fixture("settings-untouched");
    let settings_path = f.target.settings.clone().unwrap();
    let before = settings::read(&settings_path).unwrap();

    f.apply(&mods(&["modA"])).unwrap();
    assert_eq!(settings::read(&settings_path).unwrap(), before);
    undo(&f.target.journal_roots()).unwrap();
    assert_eq!(settings::read(&settings_path).unwrap(), before);
}

#[test]
fn a_failed_apply_leaves_an_undoable_journal_and_no_completion_marker() {
    let f = fixture("apply-fault-midway");
    let list = mods(&["modA", "modB"]);
    crate::deploy::fail_after(2, io::ErrorKind::Other);
    let error = f.apply(&list).unwrap_err();
    crate::deploy::fail_after(0, io::ErrorKind::Other);
    assert!(matches!(error, DeployError::Io { .. }), "{error:?}");
    assert!(f.target.state.join(JOURNAL_FILE).exists());
    assert!(!f.target.state.join(DEPLOYMENT_FILE).exists());

    // The partial layer is undone from the journal, and the next apply repairs and completes.
    assert!(undo(&f.target.journal_roots()).unwrap().is_empty());
    assert!(!f.target.state.join(JOURNAL_FILE).exists());
    f.apply(&list).unwrap();
    assert!(f.target.state.join(DEPLOYMENT_FILE).is_file());
}

#[test]
fn a_full_disk_or_a_permission_failure_is_reported_as_an_io_error() {
    for kind in [io::ErrorKind::StorageFull, io::ErrorKind::PermissionDenied] {
        let f = fixture(&format!("apply-fault-{kind:?}"));
        crate::deploy::fail_after(1, kind);
        let error = f.apply(&mods(&["modA"])).unwrap_err();
        crate::deploy::fail_after(0, kind);
        assert!(matches!(error, DeployError::Io { .. }), "{error:?}");
        assert!(!f.target.state.join(DEPLOYMENT_FILE).exists());
        undo(&f.target.journal_roots()).unwrap();
    }
}

#[test]
fn undo_cleans_up_after_an_interrupted_apply() {
    let f = fixture("interrupted");
    let settings_path = f.target.settings.clone().unwrap();
    let settings_before = settings::read(&settings_path).unwrap();
    fs::create_dir_all(&f.target.state).unwrap();
    let journal = Journal::new(f.target.state.join(JOURNAL_FILE));
    let dir = f.root.join("Mods/modA");

    // Interruption after linking the first file, before the settings are written.
    journal
        .append(&Entry::DirCreated { path: dir.clone() })
        .unwrap();
    fs::create_dir(&dir).unwrap();
    journal
        .append(&Entry::FileDeployed {
            path: dir.join("x.ws"),
        })
        .unwrap();
    write(&dir.join("x.ws"), "from A");

    let warnings = undo(&f.target.journal_roots()).unwrap();
    assert!(warnings.is_empty());
    assert!(!dir.exists());
    assert_eq!(settings::read(&settings_path).unwrap(), settings_before);
}

#[test]
fn an_interrupted_apply_is_repaired_before_the_next_apply() {
    let f = fixture("repair-before-apply");
    fs::create_dir_all(&f.target.state).unwrap();
    let journal = Journal::new(f.target.state.join(JOURNAL_FILE));
    let dir = f.root.join("Mods/modA");
    journal
        .append(&Entry::DirCreated { path: dir.clone() })
        .unwrap();
    fs::create_dir_all(&dir).unwrap();
    journal
        .append(&Entry::FileDeployed {
            path: dir.join("x.ws"),
        })
        .unwrap();
    write(&dir.join("x.ws"), "partial");
    assert!(!f.target.state.join(DEPLOYMENT_FILE).exists());

    let report = f.apply(&mods(&["modA"])).unwrap();
    assert!(report.files > 0);
    assert!(f.target.state.join(DEPLOYMENT_FILE).is_file());
    assert_eq!(
        fs::read_to_string(f.target.state.join("mods/Mods/modA/content/x.ws")).unwrap(),
        "from A"
    );
}

#[test]
fn undo_leaves_a_folder_where_a_deployed_file_was() {
    let f = fixture("undo-file-folder");
    fs::create_dir_all(&f.target.state).unwrap();
    let journal = Journal::new(f.target.state.join(JOURNAL_FILE));
    let replaced = f.root.join("Mods/modA/x.ws");
    let removed = f.root.join("Mods/modA/y.ws");
    fs::create_dir_all(&replaced).unwrap();
    fs::write(&removed, "deployed").unwrap();
    journal
        .append(&Entry::FileDeployed {
            path: replaced.clone(),
        })
        .unwrap();
    journal
        .append(&Entry::FileDeployed {
            path: removed.clone(),
        })
        .unwrap();

    let warnings = undo(&f.target.journal_roots()).unwrap();
    assert!(
        warnings
            .iter()
            .any(|warning| warning.code == "undo.file_is_folder")
    );
    assert!(replaced.is_dir(), "a user folder must survive undo");
    assert!(!removed.exists(), "the other entry is still undone");
    assert!(!f.target.state.join(JOURNAL_FILE).exists());
}

#[test]
fn undo_leaves_a_file_where_a_created_folder_was() {
    let f = fixture("undo-folder-file");
    fs::create_dir_all(&f.target.state).unwrap();
    let journal = Journal::new(f.target.state.join(JOURNAL_FILE));
    let replaced = f.root.join("Mods/modA");
    fs::create_dir_all(&f.root).unwrap();
    fs::write(&replaced, "user file").unwrap();
    journal
        .append(&Entry::DirCreated {
            path: replaced.clone(),
        })
        .unwrap();

    let warnings = undo(&f.target.journal_roots()).unwrap();
    assert!(
        warnings
            .iter()
            .any(|warning| warning.code == "undo.folder_is_file")
    );
    assert_eq!(fs::read_to_string(&replaced).unwrap(), "user file");
    assert!(!f.target.state.join(JOURNAL_FILE).exists());
}

#[test]
fn the_next_apply_repairs_a_damaged_completion_marker() {
    let f = fixture("repair-damaged-marker");
    fs::create_dir_all(&f.target.state).unwrap();
    let journal = Journal::new(f.target.state.join(JOURNAL_FILE));
    let dir = f.root.join("Mods/modA");
    journal
        .append(&Entry::DirCreated { path: dir.clone() })
        .unwrap();
    fs::create_dir_all(&dir).unwrap();
    journal
        .append(&Entry::FileDeployed {
            path: dir.join("x.ws"),
        })
        .unwrap();
    write(&dir.join("x.ws"), "partial");
    fs::write(f.target.state.join(DEPLOYMENT_FILE), b"{ not json").unwrap();
    assert!(!journal.is_complete(), "a damaged marker is not applied");

    let report = f.apply(&mods(&["modA"])).unwrap();
    assert!(report.files > 0);
    assert!(journal.is_complete());
    assert_eq!(
        fs::read_to_string(f.target.state.join("mods/Mods/modA/content/x.ws")).unwrap(),
        "from A"
    );
}

#[test]
fn undo_repairs_a_damaged_completion_marker() {
    let f = fixture("undo-damaged-marker");
    fs::create_dir_all(&f.target.state).unwrap();
    let journal = Journal::new(f.target.state.join(JOURNAL_FILE));
    let dir = f.root.join("Mods/modA");
    journal
        .append(&Entry::DirCreated { path: dir.clone() })
        .unwrap();
    fs::create_dir_all(&dir).unwrap();
    fs::write(f.target.state.join(DEPLOYMENT_FILE), b"{ not json").unwrap();

    let warnings = undo(&f.target.journal_roots()).unwrap();
    assert!(warnings.is_empty());
    assert!(!dir.exists());
    assert!(!f.target.state.join(JOURNAL_FILE).exists());
    assert!(!f.target.state.join(DEPLOYMENT_FILE).exists());
}

#[test]
fn a_failed_settings_step_leaves_no_completion_marker() {
    let f = fixture("settings-failure");
    // A folder where the settings file belongs: reading it fails after the files are placed and before the marker.
    let settings_path = f.target.settings.clone().unwrap();
    fs::remove_file(&settings_path).unwrap();
    fs::create_dir(&settings_path).unwrap();

    let error = f.apply(&mods(&["modA"])).unwrap_err();
    assert!(matches!(error, DeployError::Io(_)), "{error}");
    assert!(!f.target.state.join(DEPLOYMENT_FILE).exists());
    assert!(
        f.target.state.join(JOURNAL_FILE).is_file(),
        "the interrupted apply stays undoable"
    );

    let warnings = undo(&f.target.journal_roots()).unwrap();
    assert!(warnings.is_empty());
    assert!(!f.target.state.join(JOURNAL_FILE).exists());
    assert!(!f.root.join("Mods/modA").exists());
}
