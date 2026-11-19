//! Mirrored mods: the game paths kept, and one wrapper folder dropped.

use super::*;
use crate::test_support::scratch;

#[test]
fn mirror_keeps_the_game_paths_and_skips_the_rest() {
    let base = scratch!("mirror");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Foo-1.zip",
        &[
            ("archive/pc/mod/foo.archive", "a"),
            ("r6/scripts/Foo/a.reds", "b"),
            ("bin/x64/game.exe", "game"),
            ("readme.txt", "hello"),
        ],
    );

    let import = install_zip(&zip, &library, &mirror_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert_eq!(import.mods.len(), 1);
    assert_eq!(import.mods[0].folder, "Foo-1");
    assert_eq!(import.mods[0].slot, "");
    assert_eq!(import.mods[0].files, 2);
    assert_eq!(import.skipped, 2);
    assert!(library.join("Foo-1/archive/pc/mod/foo.archive").is_file());
    assert!(library.join("Foo-1/r6/scripts/Foo/a.reds").is_file());
}

#[test]
fn mirror_drops_one_wrapper_folder() {
    let base = scratch!("mirror-wrapper");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Wrapped.zip",
        &[("Wrapped/red4ext/plugins/W/w.dll", "w")],
    );

    let import = install_zip(&zip, &library, &mirror_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert!(library.join("Wrapped/red4ext/plugins/W/w.dll").is_file());
    assert_eq!(import.mods[0].files, 1);
}
