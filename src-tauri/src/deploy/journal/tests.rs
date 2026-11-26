use super::*;
use crate::domain::deployment_record::DeploymentRecord;
use crate::test_support::scratch;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn entries_are_read_back_in_order_and_the_journal_is_removed_after_appending() {
    let dir = scratch!("journal");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let journal = Journal::new(dir.join("journal.jsonl"));

    journal
        .append(&JournalOp::DirCreated {
            path: dir.join("a"),
        })
        .unwrap();
    journal
        .append(&JournalOp::FileDeployed {
            path: dir.join("a/x"),
        })
        .unwrap();
    let entries = journal.read().unwrap();
    assert_eq!(
        entries,
        vec![
            JournalOp::DirCreated {
                path: dir.join("a")
            },
            JournalOp::FileDeployed {
                path: dir.join("a/x")
            },
        ]
    );

    // The file is still open from the appends; removing it must work on Windows too.
    journal.remove().unwrap();
    assert!(!journal.exists());
}

#[test]
fn the_completion_marker_is_written_and_removed_with_the_journal() {
    let dir = scratch!("journal-complete");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let journal = Journal::new(dir.join("journal.jsonl"));
    journal
        .append(&JournalOp::FileDeployed {
            path: dir.join("a/x"),
        })
        .unwrap();
    assert!(!journal.is_complete());

    journal
        .complete(&DeploymentRecord {
            schema_version: crate::domain::STATE_SCHEMA,
            files: 1,
            plan_hash: "abc".to_string(),
        })
        .unwrap();
    assert!(journal.is_complete());
    assert!(journal.complete_path().is_file());
    assert_eq!(
        journal.read_complete().map(|record| record.schema_version),
        Some(crate::domain::STATE_SCHEMA)
    );

    journal.remove().unwrap();
    assert!(!journal.exists());
    assert!(!journal.is_complete());
    assert!(!journal.complete_path().exists());
}

#[test]
fn an_empty_or_damaged_marker_is_not_complete_and_is_kept() {
    for (name, content) in [
        ("journal-marker-empty", ""),
        ("journal-marker-corrupt", "{ not json"),
        ("journal-marker-no-fields", "{}"),
    ] {
        let dir = scratch!(name);
        let journal = Journal::new(dir.join("journal.jsonl"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(journal.complete_path(), content).unwrap();

        assert!(!journal.is_complete(), "{name}");
        assert!(journal.read_complete().is_none(), "{name}");
        assert!(
            journal.complete_path().is_file(),
            "the damaged marker is kept for diagnosis: {name}"
        );
    }
}

#[test]
fn a_marker_from_a_newer_build_is_not_complete() {
    let dir = scratch!("journal-marker-newer");
    let journal = Journal::new(dir.join("journal.jsonl"));
    fs::create_dir_all(&dir).unwrap();
    let marker = format!(
        r#"{{"schema_version":{},"method":"hardlink","files":1,"plan_hash":"abc"}}"#,
        crate::domain::STATE_SCHEMA + 1
    );
    fs::write(journal.complete_path(), marker).unwrap();

    assert!(!journal.is_complete());
    assert!(journal.read_complete().is_none());
}

#[test]
fn an_old_marker_without_a_schema_version_is_complete() {
    let dir = scratch!("journal-marker-old");
    let journal = Journal::new(dir.join("journal.jsonl"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        journal.complete_path(),
        r#"{"method":"hardlink","files":1,"plan_hash":"abc"}"#,
    )
    .unwrap();

    let record = journal.read_complete().unwrap();
    assert_eq!(record.schema_version, crate::domain::STATE_SCHEMA);
    assert!(record.is_supported());
    assert!(journal.is_complete());
}

#[test]
fn a_missing_journal_has_no_entries_and_can_be_removed_twice() {
    let dir = scratch!("journal-missing");
    let journal = Journal::new(dir.join("journal.jsonl"));
    assert!(!journal.exists());
    journal
        .for_each(|_| Ok::<(), crate::deploy::DeployError>(()))
        .unwrap();
    assert!(!journal.has_layer_prepared().unwrap());
    journal.remove().unwrap();
    journal.remove().unwrap();
}

#[test]
fn a_journal_exists_after_an_append_and_finds_its_layer_entry() {
    let dir = scratch!("journal-layer");
    let journal = Journal::new(dir.join("journal.jsonl"));
    journal
        .append(&JournalOp::FileDeployed {
            path: dir.join("game/x"),
        })
        .unwrap();
    assert!(journal.exists());
    assert!(!journal.has_layer_prepared().unwrap());

    journal
        .append(&JournalOp::LayerPrepared {
            mods: dir.join("state/mods"),
            overwrite: dir.join("state/overwrite"),
        })
        .unwrap();
    assert!(journal.has_layer_prepared().unwrap());
}

#[test]
fn a_torn_last_line_is_ignored_and_the_earlier_entries_survive() {
    let dir = scratch!("journal-torn");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("journal.jsonl");
    fs::write(
        &path,
        "{\"op\":\"dir_created\",\"path\":\"a\"}\n{\"op\":\"file_dep",
    )
    .unwrap();

    let entries = Journal::new(path).read().unwrap();
    assert_eq!(entries, vec![JournalOp::DirCreated { path: "a".into() }]);
}

#[test]
fn an_unknown_entry_is_skipped_and_the_known_entries_survive() {
    let dir = scratch!("journal-unknown");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("journal.jsonl");
    fs::write(
        &path,
        "{\"op\":\"from_a_newer_build\",\"path\":\"a\"}\n{\"op\":\"dir_created\",\"path\":\"b\"}\n",
    )
    .unwrap();

    let entries = Journal::new(path).read().unwrap();
    assert_eq!(entries, vec![JournalOp::DirCreated { path: "b".into() }]);
}

fn roots(base: &Path) -> JournalRoots {
    JournalRoots::new(
        base.join("state"),
        base.join("game"),
        Some(base.join("settings/mods.settings")),
    )
}

#[test]
fn the_roots_allow_every_path_a_valid_apply_writes() {
    let base = scratch!("journal-roots-valid");
    let roots = roots(&base);
    let entries = [
        JournalOp::DirCreated {
            path: base.join("game/Mods/modA"),
        },
        JournalOp::FileDeployed {
            path: base.join("game/Mods/modA/content/x.ws"),
        },
        JournalOp::DirCreated {
            path: base.join("state/overwrite/cache"),
        },
        JournalOp::FileDeployed {
            path: base.join("state/overwrite/cache/final.bin"),
        },
        JournalOp::FileDeployed {
            path: base.join("state/mods/Mods/modA/content/x.ws"),
        },
        JournalOp::SettingsSections {
            path: base.join("settings/mods.settings"),
            existed: true,
            touched: Vec::new(),
        },
        JournalOp::SettingsSections {
            path: base.join("state/userdata/documents/upper/mods.settings"),
            existed: false,
            touched: Vec::new(),
        },
        JournalOp::LayerPrepared {
            mods: base.join("state/mods"),
            overwrite: base.join("state/overwrite"),
        },
    ];

    for entry in &entries {
        assert_eq!(roots.unsafe_path(entry), None, "{entry:?}");
    }
}

#[test]
fn a_relative_dotdot_or_absolute_escape_is_unsafe() {
    let base = scratch!("journal-roots-escape");
    let roots = roots(&base);
    let outside = base.parent().unwrap().join("outside");
    let entries = [
        JournalOp::FileDeployed {
            path: PathBuf::from("relative/path"),
        },
        JournalOp::DirCreated {
            path: base.join("game/../outside"),
        },
        JournalOp::FileDeployed {
            path: outside.clone(),
        },
        JournalOp::FileDeployed {
            path: PathBuf::from(r"\\server\share\outside"),
        },
        JournalOp::FileDeployed {
            path: PathBuf::from(r"C:\Windows\System32"),
        },
    ];

    for entry in &entries {
        assert!(
            roots.unsafe_path(entry).is_some(),
            "an escaping path must be unsafe: {entry:?}"
        );
    }

    assert_eq!(
        roots.unsafe_path(&JournalOp::FileDeployed {
            path: outside.clone()
        }),
        Some(outside.as_path())
    );
}

#[test]
fn a_path_in_another_games_folder_is_unsafe() {
    let base = scratch!("journal-roots-game");
    let roots = roots(&base);
    let other = base.join("other-game/Mods/modA/content/x.ws");
    assert_eq!(
        roots.unsafe_path(&JournalOp::FileDeployed {
            path: other.clone()
        }),
        Some(other.as_path())
    );
}

#[test]
fn a_state_file_outside_the_layer_folders_is_unsafe() {
    let base = scratch!("journal-roots-state");
    let roots = JournalRoots::new(
        base.join("data/state/p1"),
        base.join("game"),
        Some(base.join("settings/mods.settings")),
    );

    for path in [
        base.join("data/state/p1/journal.jsonl"),
        base.join("data/state/p1/deployment.json"),
        base.join("data/state/p1/userdata/documents/upper/x.sav"),
        base.join("data/state/profiles.json"),
        base.join("data/state/p2/overwrite/x.sav"),
        base.join("data/state/p2/mods/Mods/modA/x.ws"),
    ] {
        assert_eq!(
            roots.unsafe_path(&JournalOp::FileDeployed { path: path.clone() }),
            Some(path.as_path()),
            "a state file is not a deployment path"
        );
    }

    assert_eq!(
        roots.unsafe_path(&JournalOp::FileDeployed {
            path: base.join("data/state/p1/overwrite/x.sav"),
        }),
        None
    );
}

#[test]
fn a_settings_path_outside_the_settings_target_is_unsafe() {
    let base = scratch!("journal-roots-settings");
    let roots = roots(&base);

    for path in [
        base.join("other/mods.settings"),
        base.join("game/mods.settings"),
        base.join("state/other.json"),
        base.join("state/userdata/documents/copy/mods.settings"),
        base.join("state/userdata/documents/base/mods.settings"),
        base.join("state/userdata/upper/mods.settings"),
    ] {
        assert_eq!(
            roots.unsafe_path(&JournalOp::SettingsSections {
                path: path.clone(),
                existed: true,
                touched: Vec::new(),
            }),
            Some(path.as_path())
        );
    }

    assert_eq!(
        roots.unsafe_path(&JournalOp::LayerPrepared {
            mods: base.join("other/mods"),
            overwrite: base.join("state/overwrite"),
        }),
        Some(base.join("other/mods").as_path())
    );
}
