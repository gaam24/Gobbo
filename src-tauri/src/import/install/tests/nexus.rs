//! Nexus archives: the folder named after the mod, its id and its version, and the older archives.

use super::*;
use crate::test_support::scratch;

/// The script inside every Example Cleanup Mod archive.
const SCRIPT: &str = "r6/scripts/ExampleCleanupMod/ExampleCleanupMod.reds";
/// Example Cleanup Mod in version 1.2.0, as Nexus names its archive.
const OLD_ARCHIVE: &str = "Example Cleanup Mod 7001 1.2.0 2026-10-23T17-21Z example-uploader.zip";
/// Example Cleanup Mod in version 1.2.1, as Nexus names its archive.
const NEWER_ARCHIVE: &str = "Example Cleanup Mod 7001 1.2.1 2026-11-01T09-00Z Abc123xyz.zip";
/// Another archive of version 1.2.0, uploaded later.
const SAME_VERSION_ARCHIVE: &str = "Example Cleanup Mod 7001 1.2.0 2026-11-01T09-00Z Abc123xyz.zip";
/// The digest of the later archive, which differs from the digest of the first one.
const OTHER_DIGEST: &str = "fedcba9876543210";

#[test]
fn nexus_archive_gets_a_folder_named_after_the_mod_its_id_and_version() {
    let base = scratch!("nexus");
    let library = base.join("library");
    let zip = make_zip(&base, OLD_ARCHIVE, &[(SCRIPT, "x")]);

    let import = install_into(&zip, &library, &mirror_game(), DIGEST);
    let folder = "Example Cleanup Mod 7001 1.2.0 2026-10-23T17-21Z example-uploader";
    assert_eq!(import.mods[0].folder, folder);
    assert!(library.join(folder).join(SCRIPT).is_file());
}

#[test]
fn a_newer_nexus_version_is_kept_next_to_the_old_one() {
    let base = scratch!("nexus-newer");
    let library = base.join("library");
    let first = make_zip(&base, OLD_ARCHIVE, &[(SCRIPT, "x")]);
    install_into(&first, &library, &mirror_game(), DIGEST);

    let newer = make_zip(&base, NEWER_ARCHIVE, &[(SCRIPT, "y")]);
    let import = install_into(&newer, &library, &mirror_game(), OTHER_DIGEST);
    assert_eq!(
        import.mods[0].folder,
        "Example Cleanup Mod 7001 1.2.1 2026-11-01T09-00Z Abc123xyz"
    );

    for folder in [
        "Example Cleanup Mod 7001 1.2.0 2026-10-23T17-21Z example-uploader",
        "Example Cleanup Mod 7001 1.2.1 2026-11-01T09-00Z Abc123xyz",
    ] {
        assert!(library.join(folder).join(SCRIPT).is_file());
    }
}

#[test]
fn two_archives_with_the_same_nexus_version_get_separate_folders() {
    let base = scratch!("nexus-again");
    let library = base.join("library");
    let first = make_zip(&base, OLD_ARCHIVE, &[(SCRIPT, "x")]);
    install_into(&first, &library, &mirror_game(), DIGEST);

    let again = make_zip(&base, SAME_VERSION_ARCHIVE, &[(SCRIPT, "y")]);
    let import = install_into(&again, &library, &mirror_game(), OTHER_DIGEST);
    assert_eq!(
        import.mods[0].folder,
        "Example Cleanup Mod 7001 1.2.0 2026-11-01T09-00Z Abc123xyz"
    );
}

#[test]
fn older_nexus_archive_keeps_the_folder_name_it_always_had() {
    let base = scratch!("nexus-old");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Example Sort Mod-1234-1-1-1584112564.zip",
        &[("r6/scripts/ExampleSortMod/a.reds", "x")],
    );

    let import = install_into(&zip, &library, &mirror_game(), DIGEST);
    assert_eq!(
        import.mods[0].folder,
        "Example Sort Mod-1234-1-1-1584112564"
    );
}
