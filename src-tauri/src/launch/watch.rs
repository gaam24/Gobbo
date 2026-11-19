//! What changes in the game folder while the game runs. In an overlay session the game folder must not change at all: the
//! game's writes go to the overwrite layer. So the folder is recorded before the launch, and compared after the game exits.
//! A change that is left is a write that got past the layers, and the run report names it. The comparison cannot say
//! which program made the write, only what changed.

use std::collections::BTreeMap;

use rustc_hash::FxHashSet;
use std::path::Path;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::infra::fs::{WalkErrors, walk};

/// What the record knows about one entry of the game folder.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    is_dir: bool,
    bytes: u64,
    modified: Option<SystemTime>,
}

/// The entries of the game folder at one moment, by their path relative to the folder.
#[derive(Debug, Default)]
pub struct Snapshot {
    entries: BTreeMap<String, Entry>,
}

/// One entry that differs between two snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct GameChange {
    /// The path relative to the game folder.
    pub path: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
}

/// Records every file and folder below `root`. A folder that cannot be read is skipped with its entries, so the later
/// comparison reports only what the record could see.
pub fn take(root: &Path) -> Snapshot {
    let mut entries = BTreeMap::new();
    let _ = walk(root, WalkErrors::Skip, &mut |entry, rel, is_dir| {
        let Ok(meta) = entry.metadata() else {
            return Ok(());
        };
        entries.insert(
            rel.to_string(),
            Entry {
                is_dir,
                bytes: if is_dir { 0 } else { meta.len() },
                modified: meta.modified().ok(),
            },
        );
        Ok(())
    });
    Snapshot { entries }
}

/// The changes from `before` to `after`, in path order. A folder counts as changed only when it appears or disappears:
/// its own time moves whenever something inside it changes, and that would repeat every change below it. Paths in
/// `expected` are left out, because Gobbo itself put them there.
pub fn changes(before: &Snapshot, after: &Snapshot, expected: &[String]) -> Vec<GameChange> {
    // The expected paths are a few thousand entries of a game folder; a list lookup inside the loops below was
    // quadratic. Build the set once.
    let expected: FxHashSet<&str> = expected.iter().map(String::as_str).collect();
    let mut found = Vec::new();
    for (path, entry) in &after.entries {
        if expected.contains(path.as_str()) {
            continue;
        }
        match before.entries.get(path) {
            None => found.push(GameChange {
                path: path.clone(),
                kind: ChangeKind::Created,
            }),
            Some(old) if !entry.is_dir && old != entry => found.push(GameChange {
                path: path.clone(),
                kind: ChangeKind::Modified,
            }),
            Some(_) => {}
        }
    }
    for path in before.entries.keys() {
        if !after.entries.contains_key(path) && !expected.contains(path.as_str()) {
            found.push(GameChange {
                path: path.clone(),
                kind: ChangeKind::Removed,
            });
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;
    use std::fs;

    /// The relative path `parts` joined the way the record writes it.
    fn sub(first: &str, second: &str) -> String {
        format!("{first}/{second}")
    }

    fn kinds(changes: &[GameChange]) -> Vec<(String, ChangeKind)> {
        changes
            .iter()
            .map(|change| (change.path.clone(), change.kind))
            .collect()
    }

    #[test]
    fn an_unchanged_folder_has_no_changes() {
        let root = scratch!("same");
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("bin/game.exe"), "game").unwrap();
        let before = take(&root);
        assert!(changes(&before, &take(&root), &[]).is_empty());
    }

    #[test]
    fn created_modified_and_removed_files_are_reported_in_path_order() {
        let root = scratch!("kinds");
        fs::create_dir_all(root.join("r6")).unwrap();
        fs::write(root.join("r6/keep.txt"), "same").unwrap();
        fs::write(root.join("r6/change.txt"), "old").unwrap();
        fs::write(root.join("r6/gone.txt"), "bye").unwrap();
        let before = take(&root);

        fs::write(root.join("r6/change.txt"), "new contents").unwrap();
        fs::remove_file(root.join("r6/gone.txt")).unwrap();
        fs::write(root.join("r6/added.txt"), "hi").unwrap();
        let after = take(&root);

        let found = changes(&before, &after, &[]);
        assert_eq!(
            kinds(&found),
            vec![
                (sub("r6", "added.txt"), ChangeKind::Created),
                (sub("r6", "change.txt"), ChangeKind::Modified),
                (sub("r6", "gone.txt"), ChangeKind::Removed),
            ]
        );
    }

    #[test]
    fn a_new_folder_is_reported_but_its_changed_time_is_not() {
        let root = scratch!("folders");
        fs::create_dir_all(root.join("a")).unwrap();
        let before = take(&root);
        fs::create_dir_all(root.join("a/b")).unwrap();
        let found = changes(&before, &take(&root), &[]);
        assert_eq!(kinds(&found), vec![(sub("a", "b"), ChangeKind::Created)]);
    }

    #[test]
    fn paths_gobbo_placed_itself_are_left_out() {
        let root = scratch!("expected");
        let before = take(&root);
        fs::write(root.join("version.dll"), "placed").unwrap();
        fs::write(root.join("stray.txt"), "game").unwrap();
        let after = take(&root);
        let found = changes(&before, &after, &["version.dll".to_string()]);
        assert_eq!(
            kinds(&found),
            vec![("stray.txt".to_string(), ChangeKind::Created)]
        );
    }
}
