use super::*;
use crate::infra::io::digest_file;
use crate::test_support::scratch;
use sha2::Sha256;
use std::fs;
use std::path::Path;

#[test]
fn copy_tree_replaces_or_fills_the_target() {
    let dir = scratch!("copy-tree");
    let source = dir.join("source");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::write(source.join("a.txt"), "source-a").unwrap();
    fs::write(source.join("nested/b.txt"), "source-b").unwrap();

    let replaced = dir.join("replaced");
    fs::create_dir_all(&replaced).unwrap();
    fs::write(replaced.join("old.txt"), "old").unwrap();
    copy_tree(&source, &replaced, CopyMode::Replace).unwrap();
    assert!(!replaced.join("old.txt").exists());
    assert!(
        !dir.join("replaced.old").exists(),
        "a plain replace keeps no generation"
    );
    assert_eq!(
        fs::read_to_string(replaced.join("a.txt")).unwrap(),
        "source-a"
    );

    let filled = dir.join("filled");
    fs::create_dir_all(&filled).unwrap();
    fs::write(filled.join("a.txt"), "kept").unwrap();
    copy_tree(&source, &filled, CopyMode::FillMissing).unwrap();
    assert_eq!(fs::read_to_string(filled.join("a.txt")).unwrap(), "kept");
    assert_eq!(
        fs::read_to_string(filled.join("nested/b.txt")).unwrap(),
        "source-b"
    );
}

#[test]
fn a_merge_overwrites_same_names_and_keeps_every_other_target_entry() {
    let dir = scratch!("copy-merge");
    let source = dir.join("source");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::write(source.join("a.txt"), "profile-a").unwrap();
    fs::write(source.join("nested/b.txt"), "profile-b").unwrap();
    let target = dir.join("target");
    fs::create_dir_all(target.join("nested")).unwrap();
    fs::write(target.join("a.txt"), "real-a").unwrap();
    fs::write(target.join("nested/kept.txt"), "real-kept").unwrap();
    fs::write(target.join("keep.txt"), "real-only").unwrap();

    copy_tree(&source, &target, CopyMode::Merge).unwrap();

    assert_eq!(
        fs::read_to_string(target.join("a.txt")).unwrap(),
        "profile-a"
    );
    assert_eq!(
        fs::read_to_string(target.join("nested/b.txt")).unwrap(),
        "profile-b"
    );
    assert_eq!(
        fs::read_to_string(target.join("nested/kept.txt")).unwrap(),
        "real-kept"
    );
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "real-only",
        "a merge never deletes a target entry the source does not name"
    );
}

#[test]
fn a_merge_creates_its_target_and_its_nested_folders() {
    let dir = scratch!("copy-merge-new");
    let source = dir.join("source");
    fs::create_dir_all(source.join("a/b")).unwrap();
    fs::write(source.join("a/b/file.txt"), "new").unwrap();

    copy_tree(&source, &dir.join("target"), CopyMode::Merge).unwrap();

    assert_eq!(
        fs::read_to_string(dir.join("target/a/b/file.txt")).unwrap(),
        "new"
    );
}

/// Unwraps the conflict of a merge that must refuse a file/folder mismatch.
fn merge_conflict(source: &Path, target: &Path) -> MergeConflict {
    match copy_tree(source, target, CopyMode::Merge).unwrap_err() {
        CopyError::Conflict(conflict) => conflict,
        CopyError::Io(error) => panic!("expected a conflict, got {error}"),
    }
}

#[test]
fn a_merge_refuses_a_file_where_the_target_has_a_folder() {
    let dir = scratch!("copy-merge-file-folder");
    let source = dir.join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("shared"), "source-file").unwrap();
    let target = dir.join("target");
    fs::create_dir_all(target.join("shared")).unwrap();
    fs::write(target.join("shared/inside.txt"), "real-inside").unwrap();

    let conflict = merge_conflict(&source, &target);

    assert_eq!(conflict.path, target.join("shared"));
    assert_eq!(conflict.source_kind, EntryKind::File);
    assert_eq!(conflict.target_kind, EntryKind::Folder);
    assert!(target.join("shared").is_dir(), "the folder stays usable");
    assert_eq!(
        fs::read_to_string(target.join("shared/inside.txt")).unwrap(),
        "real-inside"
    );
    assert_eq!(
        fs::read_to_string(source.join("shared")).unwrap(),
        "source-file"
    );
}

#[test]
fn a_merge_refuses_a_folder_where_the_target_has_a_file() {
    let dir = scratch!("copy-merge-folder-file");
    let source = dir.join("source");
    fs::create_dir_all(source.join("shared")).unwrap();
    fs::write(source.join("shared/inside.txt"), "profile-inside").unwrap();
    let target = dir.join("target");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("shared"), "real-file").unwrap();

    let conflict = merge_conflict(&source, &target);

    assert_eq!(conflict.path, target.join("shared"));
    assert_eq!(conflict.source_kind, EntryKind::Folder);
    assert_eq!(conflict.target_kind, EntryKind::File);
    assert_eq!(
        fs::read_to_string(target.join("shared")).unwrap(),
        "real-file"
    );
}

#[test]
fn a_failed_merge_stops_before_the_first_write_and_the_target_stays_usable() {
    let dir = scratch!("copy-merge-conflict-early");
    let source = dir.join("source");
    fs::create_dir_all(source.join("z/child")).unwrap();
    fs::write(source.join("a.txt"), "profile").unwrap();
    fs::write(source.join("z/child/file.txt"), "profile").unwrap();
    let target = dir.join("target");
    fs::create_dir_all(target.join("z")).unwrap();
    fs::write(target.join("z/child"), "real-file").unwrap();
    fs::write(target.join("keep.txt"), "keep").unwrap();

    // The mismatch is deep in the tree; the check still runs before the first write.
    let conflict = merge_conflict(&source, &target);
    assert_eq!(conflict.path, target.join("z/child"));
    assert_eq!(conflict.source_kind, EntryKind::Folder);
    assert_eq!(conflict.target_kind, EntryKind::File);

    assert_eq!(
        fs::read_to_string(target.join("z/child")).unwrap(),
        "real-file"
    );
    assert!(!target.join("a.txt").exists());
    assert_eq!(fs::read_to_string(target.join("keep.txt")).unwrap(), "keep");
}

#[test]
fn a_merge_that_fails_midway_leaves_every_written_file_readable() {
    let dir = scratch!("copy-merge-fail");
    let source = dir.join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("a.txt"), "profile-a").unwrap();
    fs::write(source.join("b.txt"), "profile-b").unwrap();
    let target = dir.join("target");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("keep.txt"), "keep").unwrap();

    fail_after(2);
    let error = copy_tree(&source, &target, CopyMode::Merge).unwrap_err();
    fail_after(0);
    assert!(error.to_string().contains("injected"));
    assert_eq!(fs::read_to_string(target.join("keep.txt")).unwrap(), "keep");
    // A merge is not staged: one file may be there already and the other not, but every written file is complete.
    let written: Vec<_> = ["a.txt", "b.txt"]
        .into_iter()
        .filter(|name| target.join(name).is_file())
        .map(|name| fs::read_to_string(target.join(name)).unwrap())
        .collect();
    assert_eq!(written.len(), 1, "only the file before the failure");
    assert!(
        written[0] == "profile-a" || written[0] == "profile-b",
        "the written file is complete"
    );

    copy_tree(&source, &target, CopyMode::Merge).unwrap();
    assert_eq!(
        fs::read_to_string(target.join("b.txt")).unwrap(),
        "profile-b"
    );
}

#[test]
fn a_tree_is_copied_and_hashed_in_one_pass() {
    let dir = scratch!("copy-tree-hashed");
    let source = dir.join("source");
    fs::create_dir_all(source.join("nested/empty")).unwrap();
    fs::write(source.join("a.txt"), "abc").unwrap();
    fs::write(source.join("nested/b.bin"), "payload").unwrap();
    let target = dir.join("target");

    let files = copy_tree_hashed::<Sha256>(&source, &target).unwrap();
    let mut names: Vec<_> = files
        .iter()
        .map(|(path, size, _)| (path.as_str(), *size))
        .collect();
    names.sort_unstable();
    assert_eq!(names, [("a.txt", 3), ("nested/b.bin", 7)]);

    for (relative, size, sha256) in &files {
        let copy = target.join(relative);
        assert_eq!(fs::metadata(&copy).unwrap().len(), *size);
        assert_eq!(&digest_file::<Sha256>(&copy, &mut |_| {}).unwrap(), sha256);
    }

    assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "abc");
    assert!(target.join("nested/empty").is_dir());
}

#[test]
fn a_staged_tree_replaces_the_target_and_keeps_one_generation() {
    let dir = scratch!("replace-staged");
    let staging = dir.join("staging");
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join("new.txt"), "new").unwrap();
    let target = dir.join("target");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("old.txt"), "old").unwrap();

    replace_with_staged(&staging, &target, true).unwrap();
    assert!(!staging.exists(), "the staged tree becomes the target");
    assert_eq!(fs::read_to_string(target.join("new.txt")).unwrap(), "new");
    assert!(!target.join("old.txt").exists());
    assert_eq!(
        fs::read_to_string(dir.join("target.old/old.txt")).unwrap(),
        "old"
    );
}

#[test]
fn sync_copy_removes_the_target_when_the_source_is_gone() {
    let dir = scratch!("sync-copy");
    let source = dir.join("source");
    let target = dir.join("target");
    fs::write(&target, "old").unwrap();

    sync_copy(&source, &target, true).unwrap();
    assert!(!target.exists());

    fs::write(&target, "old").unwrap();
    sync_copy(&source, &target, false).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "old");
}

#[test]
fn a_failed_replace_leaves_the_old_target_untouched() {
    let dir = scratch!("copy-fail");
    let source = dir.join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("a.txt"), "new-a").unwrap();
    fs::write(source.join("b.txt"), "new-b").unwrap();
    let target = dir.join("target");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("old.txt"), "old").unwrap();

    for countdown in 1..=2 {
        fail_after(countdown);
        let error = copy_tree(&source, &target, CopyMode::Replace).unwrap_err();
        fail_after(0);
        assert!(error.to_string().contains("injected"));
        assert_eq!(fs::read_to_string(target.join("old.txt")).unwrap(), "old");
        assert!(!target.join("a.txt").exists());
        assert!(!target.join("b.txt").exists());
        assert!(!dir.join("target.old").exists());
        let staging: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_name().to_string_lossy().contains("staging")
                    || entry.file_name().to_string_lossy().ends_with(".tmp")
            })
            .collect();
        assert!(staging.is_empty(), "the staging tree was not cleaned up");
    }

    copy_tree(&source, &target, CopyMode::Replace).unwrap();
    assert_eq!(fs::read_to_string(target.join("a.txt")).unwrap(), "new-a");
    assert_eq!(fs::read_to_string(target.join("b.txt")).unwrap(), "new-b");
}

#[test]
fn a_successful_replace_keeps_one_previous_generation() {
    let dir = scratch!("copy-old");
    let source = dir.join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("new.txt"), "first").unwrap();
    let target = dir.join("target");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("old.txt"), "old").unwrap();

    copy_tree(&source, &target, CopyMode::ReplaceKeepPrevious).unwrap();
    assert_eq!(fs::read_to_string(target.join("new.txt")).unwrap(), "first");
    assert!(!target.join("old.txt").exists());
    assert_eq!(
        fs::read_to_string(dir.join("target.old/old.txt")).unwrap(),
        "old"
    );

    fs::write(source.join("new.txt"), "second").unwrap();
    copy_tree(&source, &target, CopyMode::ReplaceKeepPrevious).unwrap();
    assert_eq!(
        fs::read_to_string(target.join("new.txt")).unwrap(),
        "second"
    );
    assert_eq!(
        fs::read_to_string(dir.join("target.old/new.txt")).unwrap(),
        "first"
    );
    let generations: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("target.old")
        })
        .collect();
    assert_eq!(
        generations.len(),
        1,
        "the previous generation must not accumulate"
    );
}

#[test]
fn a_tree_checksum_changes_when_one_file_changes_without_changing_its_size() {
    let dir = scratch!("hash-tree");
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("sub/a.txt"), "one").unwrap();
    fs::write(dir.join("b.txt"), "two").unwrap();
    let first = hash_tree(&dir).unwrap();
    assert_eq!(first.1, 2);
    assert_eq!(first.2, 6);

    // Same length, different content: a size-only report would miss this.
    fs::write(dir.join("sub/a.txt"), "ONE").unwrap();
    let changed = hash_tree(&dir).unwrap();
    assert_ne!(changed.0, first.0);

    // A rename changes the checksum too, while a reread of the same bytes does not.
    fs::write(dir.join("sub/a.txt"), "one").unwrap();
    assert_eq!(hash_tree(&dir).unwrap().0, first.0);
    fs::rename(dir.join("sub/a.txt"), dir.join("sub/c.txt")).unwrap();
    assert_ne!(hash_tree(&dir).unwrap().0, first.0);
}
