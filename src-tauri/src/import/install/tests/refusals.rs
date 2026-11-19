//! Archives the import refuses, and what stays untouched when it does.

use super::*;
use crate::test_support::scratch;

#[test]
fn archive_with_a_bin_folder_is_refused_and_nothing_is_written() {
    let base = scratch!("bin");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "bin.zip",
        &[("bin/game.exe", "x"), ("Mods/modA/content/x.ws", "x")],
    );

    let err = install_zip(&zip, &library, &slot_game(), DIGEST, None, &mut |_| {}).unwrap_err();
    assert!(matches!(err, InstallError::UnsupportedRoot { .. }));
    assert!(!library.join("modA").exists());
}

#[test]
fn a_suffixed_folder_already_taken_is_refused_and_nothing_is_written() {
    let base = scratch!("suffix-taken");
    let library = base.join("library");
    fs::create_dir_all(library.join("modA")).unwrap();
    fs::create_dir_all(library.join("modA (01234567)")).unwrap();
    let zip = make_zip(&base, "modA.zip", &[("Mods/modA/content/new.ws", "new")]);

    let err = install_zip(&zip, &library, &slot_game(), DIGEST, None, &mut |_| {}).unwrap_err();
    assert!(matches!(
        err,
        InstallError::AlreadyInLibrary { ref folder } if folder == "modA (01234567)"
    ));
    assert!(!library.join("modA (01234567)/content/new.ws").exists());
}

#[test]
fn archive_without_a_known_layout_is_refused() {
    let base = scratch!("unknown");
    let library = base.join("library");
    let zip = make_zip(&base, "readme.zip", &[("readme.txt", "hello")]);

    let err = install_zip(&zip, &library, &slot_game(), DIGEST, None, &mut |_| {}).unwrap_err();
    assert!(matches!(err, InstallError::UnknownLayout));
}

#[test]
fn path_escaping_the_library_is_refused() {
    let base = scratch!("escape");
    let library = base.join("library");
    let zip = make_zip(&base, "evil.zip", &[("Mods/modA/../../evil.ws", "x")]);

    let err = install_zip(&zip, &library, &slot_game(), DIGEST, None, &mut |_| {}).unwrap_err();
    assert!(matches!(err, InstallError::UnsafeEntry { .. }));
    assert!(!base.join("evil.ws").exists());
}
