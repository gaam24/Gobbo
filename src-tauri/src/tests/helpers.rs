//! Tests of the small helpers of the service root: the mod folder names, the launch command and the removal of
//! folders.

use crate::domain::naming::is_plain_folder;
use crate::import::remove_folders;
use crate::status::launch_hint;
use crate::test_support::scratch;
use std::fs;

#[test]
fn a_mod_folder_is_one_plain_name_inside_the_library() {
    assert!(is_plain_folder("Mod 1"));
    assert!(!is_plain_folder(""));
    assert!(!is_plain_folder("."));
    assert!(!is_plain_folder(".."));
    assert!(!is_plain_folder("a/b"));
    assert!(!is_plain_folder(r"a\b"));
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn the_launch_hint_starts_the_game_through_gobbo() {
    let hint = launch_hint("slot");
    assert!(hint.starts_with('"'));
    assert!(hint.ends_with(" run slot -- %command%"));
}

#[test]
fn removing_folders_takes_only_the_listed_ones() {
    let base = scratch!("remove-folders");
    fs::create_dir_all(base.join("Gone").join("inner")).unwrap();
    fs::create_dir_all(base.join("Kept")).unwrap();

    remove_folders(&base, &["Gone".to_string()]);

    assert!(!base.join("Gone").exists());
    assert!(base.join("Kept").is_dir());
}
