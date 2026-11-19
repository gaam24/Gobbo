//! Where files land: a flat slot, mirrored game paths, and a deployment root outside the game folder. Every file goes
//! to the overlay layer; the game folder is never written.

use super::*;

#[test]
fn a_flat_slot_puts_the_file_directly_in_the_layer() {
    let (f, external) = external_fixture("flat-slot");
    let report = f.apply(&profile(&["modA"], "source")).unwrap();
    assert_eq!(report.files, 1);
    assert!(f.target.state.join("mods/source/custom.pak").is_file());
    assert!(!external.join("source/custom.pak").exists());
    assert!(!external.join("source/modA/custom.pak").exists());

    undo(&f.target.journal_roots()).unwrap();
    assert!(!external.join("source/custom.pak").exists());
}

#[test]
fn mirrored_files_land_at_their_game_paths_in_the_layer() {
    let f = fixture("mirrored");
    write(&f.library.join("sharedutils/content/a.ws"), "a");
    write(
        &f.library
            .join("sharedutils/DLC/dlcsharedutils/blob0.bundle"),
        "b",
    );
    write(
        &f.library.join("sharedutils/bin/config/r4game/menu.xml"),
        "m",
    );

    f.apply(&mods(&["sharedutils"])).unwrap();
    let layer = f.target.state.join("mods");
    assert!(layer.join("Mods/modsharedutils/content/a.ws").is_file());
    assert!(layer.join("DLC/dlcsharedutils/blob0.bundle").is_file());
    assert!(layer.join("bin/config/r4game/menu.xml").is_file());
    assert!(!layer.join("DLC/modsharedutils").exists());
    assert!(!f.root.join("DLC").exists());
    assert!(!f.root.join("bin/config/r4game/menu.xml").exists());

    undo(&f.target.journal_roots()).unwrap();
    assert!(!f.root.join("DLC").exists());
    assert!(!f.root.join("bin/config").exists());
}

#[test]
fn a_mod_may_cover_a_file_the_game_already_has() {
    let f = mirror_fixture("mirror-cover");
    write(&f.root.join("archive/pc/mod/a.archive"), "game's own");

    let plan = f.plan(&profile(&["modA"], "")).unwrap();
    assert!(plan.problems.is_empty(), "{:?}", plan.problems);
    let report = f.apply(&profile(&["modA"], "")).unwrap();
    assert_eq!(report.files, 2);
    assert_eq!(
        fs::read_to_string(f.root.join("archive/pc/mod/a.archive")).unwrap(),
        "game's own"
    );
    assert_eq!(
        fs::read_to_string(f.target.state.join("mods/archive/pc/mod/a.archive")).unwrap(),
        "from A"
    );
}

#[test]
fn an_external_root_is_staged_in_the_layer_and_the_folder_stays_empty() {
    let (f, external) = external_fixture("overlay-external");
    let report = f.apply(&profile(&["modA"], "source")).unwrap();
    assert_eq!(report.files, 1);
    assert!(f.target.state.join("mods/source/custom.pak").is_file());
    assert!(!external.join("source").exists());
}
