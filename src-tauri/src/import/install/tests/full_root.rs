//! A full-root game: every path is a game path, loose archives move to the folder the game reads, and a wrapper is
//! stripped even around a framework folder the game has never seen.

use super::*;
use crate::test_support::scratch;

#[test]
fn a_full_root_game_takes_new_files_and_moves_loose_archives() {
    let base = scratch!("full-root-layout");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Future.zip",
        &[
            ("futureframework/Future.dll", "dll"),
            ("bin/x64/global.ini", "ini"),
            ("loose.archive", "archive"),
            ("loose.xl", "xl"),
            ("mods/redmod/packed/foo.archive", "redmod"),
            ("docs/readme.md", "docs"),
            ("README.md", "readme"),
        ],
    );

    let import = install_into(&zip, &library, &test_games::full_root(), DIGEST);
    assert_eq!(import.mods.len(), 1);
    assert_eq!(import.mods[0].slot, "");
    let folder = library.join(&import.mods[0].folder);
    assert!(folder.join("futureframework/Future.dll").is_file());
    assert!(folder.join("bin/x64/global.ini").is_file());
    assert!(folder.join("archive/pc/mod/loose.archive").is_file());
    assert!(folder.join("archive/pc/mod/loose.xl").is_file());
    assert!(folder.join("mods/redmod/packed/foo.archive").is_file());
    assert!(!folder.join("docs").exists());
    assert!(!folder.join("README.md").exists());
    // The docs folder is archive metadata and is not counted; the bare README is a file no rule takes.
    assert_eq!(import.skipped, 1);
}

#[test]
fn a_plain_archive_skips_only_the_installer_files_under_the_fomod_folder() {
    let base = scratch!("full-root-fomod-metadata");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Plain.zip",
        &[
            ("fomod/ModuleConfig.xml", "<config/>"),
            ("fomod/info.xml", "metadata"),
            ("fomod/parts/archive/pc/mod/data.xl", "payload"),
            ("bin/x64/global.ini", "ini"),
        ],
    );

    let import = install_into(&zip, &library, &test_games::full_root(), DIGEST);
    assert_eq!(import.mods.len(), 1);
    let folder = library.join(&import.mods[0].folder);
    assert!(folder.join("bin/x64/global.ini").is_file());
    assert!(!folder.join("fomod").exists());
    let mut metadata: Vec<&str> = import
        .skipped_files
        .iter()
        .filter(|file| file.reason == crate::domain::library::SkipReason::Metadata)
        .map(|file| file.path.as_str())
        .collect();
    metadata.sort_unstable();
    assert_eq!(
        metadata,
        [
            "fomod/ModuleConfig.xml",
            "fomod/info.xml",
            "fomod/parts/archive/pc/mod/data.xl"
        ]
    );
}

#[test]
fn a_version_folder_around_a_new_framework_is_stripped() {
    let base = scratch!("full-root-version");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Codeware.zip",
        &[
            ("1.21.4/red4ext/RED4ext.dll", "dll"),
            ("1.21.4/red4ext/plugins/Codeware/Codeware.dll", "plugin"),
        ],
    );

    let import = install_into(&zip, &library, &test_games::full_root(), DIGEST);
    assert_eq!(import.mods[0].files, 2);
    let folder = library.join(&import.mods[0].folder);
    assert!(folder.join("red4ext/RED4ext.dll").is_file());
    assert!(
        folder
            .join("red4ext/plugins/Codeware/Codeware.dll")
            .is_file()
    );
}

#[test]
fn a_wrapper_around_a_known_game_folder_is_stripped_with_the_game_roots() {
    let base = scratch!("full-root-wrapper");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "UI.zip",
        &[
            ("UI/bin/x64/plugins/nativeSettings/init.lua", "lua"),
            ("UI/readme.txt", "readme"),
        ],
    );

    let import = install_into_with_roots(
        &zip,
        &library,
        &test_games::full_root(),
        &["bin".to_string()],
        DIGEST,
    );
    assert_eq!(import.mods[0].files, 1);
    let folder = library.join(&import.mods[0].folder);
    assert!(
        folder
            .join("bin/x64/plugins/nativeSettings/init.lua")
            .is_file()
    );
    assert_eq!(import.skipped, 1);
}
