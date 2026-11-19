//! Mod folders from archives: the folder a mod gets, the wrapper folders taken off, and the names kept apart.

use super::*;
use crate::test_support::scratch;

#[test]
fn mods_folder_layout_keeps_the_mod_folder_name() {
    let base = scratch!("mods-layout");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "modAlphaSort.zip",
        &[
            ("Mods/modAlphaSort/content/x.dat", "x"),
            ("Mods/modAlphaSort/content/scripts/a.dat", "a"),
            ("readme.txt", "hello"),
        ],
    );

    let import = install_into(&zip, &library, &slot_game(), DIGEST);
    assert_one_mod(&import, "modAlphaSort", 2);
    assert_eq!(import.skipped, 1);
    assert_eq!(
        fs::read_to_string(library.join("modAlphaSort/content/x.dat")).unwrap(),
        "x"
    );
    assert!(!library.join(STAGING).exists());
}

#[test]
fn a_dlc_folder_alone_is_a_mod_of_mirrored_files() {
    let base = scratch!("dlc-layout");
    let library = base.join("library");
    let zip = make_zip(&base, "dlcPack.zip", &[("DLC/dlcPack/content/x.dat", "x")]);

    let import = install_into(&zip, &library, &slot_game(), DIGEST);
    assert_eq!(import.mods[0].folder, "dlcPack");
    assert_eq!(import.mods[0].slot, "");
    assert!(library.join("dlcPack/DLC/dlcPack/content/x.dat").is_file());
}

#[test]
fn a_folder_without_the_slot_prefix_keeps_the_archive_name_in_the_library() {
    let base = scratch!("prefix");
    let library = base.join("library");
    let zip = make_zip(&base, "sample.zip", &[("Mods/Sample/content/x.dat", "x")]);

    let import = install_into(&zip, &library, &slot_game(), DIGEST);
    assert_eq!(import.mods[0].folder, "sample");
    assert!(library.join("sample/content/x.dat").is_file());
}

#[test]
fn loose_content_is_wrapped_in_a_folder_named_after_the_zip() {
    let base = scratch!("loose");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Alpha Sort-1234.zip",
        &[("content/x.dat", "x"), ("content/scripts/a.dat", "a")],
    );

    let import = install_into(&zip, &library, &slot_game(), DIGEST);
    assert_eq!(import.mods[0].folder, "Alpha Sort-1234");
    assert_eq!(import.mods[0].files, 2);
    assert!(
        library
            .join("Alpha Sort-1234/content/scripts/a.dat")
            .is_file()
    );
}

#[test]
fn a_menu_mod_keeps_its_config_xml_in_the_same_mod() {
    let base = scratch!("layout-menu");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "menu.zip",
        &[
            (
                "mods/modExampleMenu/content/scripts/local/core/Main.dat",
                "x",
            ),
            ("bin/config/example/settings/pc/menu.xml", "<UserConfig/>"),
            ("Installation Instructions.txt", "read me"),
        ],
    );

    let import = install_into(&zip, &library, &slot_game(), DIGEST);
    assert_one_mod(&import, "menu", 2);
    assert_eq!(import.skipped, 1);
    assert!(
        library
            .join("menu/content/scripts/local/core/Main.dat")
            .is_file()
    );
    assert!(
        library
            .join("menu/bin/config/example/settings/pc/menu.xml")
            .is_file()
    );
}

#[test]
fn a_wrapper_folder_around_the_game_folders_is_taken_off() {
    let base = scratch!("layout-wrapper");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "wrapper.zip",
        &[
            ("wrapper/mods/modWrapped/content/scripts/a.dat", "a"),
            (
                "wrapper/bin/config/example/settings/pc/a.xml",
                "<UserConfig/>",
            ),
            ("wrapper/readme.txt", "read me"),
        ],
    );

    let import = install_into(&zip, &library, &slot_game(), DIGEST);
    assert_one_mod(&import, "wrapper", 2);
    assert!(library.join("wrapper/content/scripts/a.dat").is_file());
    assert!(
        library
            .join("wrapper/bin/config/example/settings/pc/a.xml")
            .is_file()
    );
}

#[test]
fn shared_lib_mods_and_dlc_folders_are_one_mod() {
    let base = scratch!("layout-shared");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "shared.zip",
        &[
            ("mods/modzzz_shared/content/info.json", "{}"),
            (
                "mods/modzzz_shared/content/scripts/shared/helpers/log.dat",
                "x",
            ),
            ("dlc/dlcshared/content/blob0.dat", "b"),
        ],
    );

    let import = install_into(&zip, &library, &slot_game(), DIGEST);
    assert_one_mod(&import, "shared", 3);
    assert!(
        library
            .join("shared/DLC/dlcshared/content/blob0.dat")
            .is_file()
    );
}

#[test]
fn a_dlc_folder_without_the_prefix_gets_it_as_before() {
    let base = scratch!("layout-prefix");
    let library = base.join("library");
    let zip = make_zip(&base, "extra.zip", &[("DLC/Extra/content/x.dat", "x")]);

    let import = install_into(&zip, &library, &slot_game(), DIGEST);
    assert_eq!(import.mods[0].slot, "");
    assert!(library.join("extra/DLC/dlcExtra/content/x.dat").is_file());
}

#[test]
fn a_mod_already_in_the_library_is_kept_under_a_suffix_and_the_old_one_stays() {
    let base = scratch!("exists");
    let library = base.join("library");
    fs::create_dir_all(library.join("modA/content")).unwrap();
    fs::write(library.join("modA/content/old.dat"), "old").unwrap();
    let zip = make_zip(&base, "modA.zip", &[("Mods/modA/content/new.dat", "new")]);

    let import = install_into(&zip, &library, &slot_game(), DIGEST);
    assert_eq!(import.mods[0].folder, "modA (01234567)");
    assert!(library.join("modA (01234567)/content/new.dat").is_file());
    assert!(library.join("modA/content/old.dat").is_file());
    assert!(!library.join("modA/content/new.dat").exists());
}

#[test]
fn a_game_folder_at_the_archive_root_is_not_a_wrapper() {
    let base = scratch!("layout-game-root");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Pack.zip",
        &[
            ("BepInEx/plugins/Mod/Mod.dll", "dll"),
            ("BepInEx/patchers/Mod.Patcher.dll", "patcher"),
        ],
    );

    let import = install_into(&zip, &library, &test_games::aliased(), DIGEST);
    assert_eq!(import.mods[0].slot, "BepInEx/plugins");
    assert_eq!(import.mods[0].files, 2);
    assert!(library.join("Pack/Mod.dll").is_file());
    assert!(
        library
            .join("Pack/BepInEx/patchers/Mod.Patcher.dll")
            .is_file()
    );
}

#[test]
fn a_doubled_game_folder_keeps_one_wrapper() {
    let base = scratch!("layout-doubled-root");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Pack.zip",
        &[("BepInEx/BepInEx/plugins/Mod/Mod.dll", "dll")],
    );

    let import = install_into(&zip, &library, &test_games::aliased(), DIGEST);
    assert_eq!(import.mods[0].slot, "BepInEx/plugins");
    assert_eq!(import.mods[0].files, 1);
    assert!(library.join("Pack/Mod.dll").is_file());
}

#[test]
fn unit_candidates_try_the_longest_root_prefix_first() {
    let base = scratch!("layout-deepest-root");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "nested.zip",
        &[
            ("Pack/Inner/ModInfo.xml", "<ModInfo/>"),
            ("Pack/Inner/content/x.dat", "x"),
            ("Pack/ModInfo.xml", "<ModInfo/>"),
            ("ModInfo.xml", "<ModInfo/>"),
        ],
    );

    install_into(&zip, &library, &external_game(), DIGEST);
    // The deepest marker root (`Pack/Inner`) takes its own content file: the outer `Pack` root and the archive-root
    // marker must not claim it.
    assert!(library.join("nested - Inner/content/x.dat").is_file());
    assert!(!library.join("nested - Pack/Inner/content/x.dat").exists());
    assert!(
        !library
            .join("nested - nested/Pack/Inner/content/x.dat")
            .exists()
    );
}

#[test]
fn a_wrapper_around_a_game_folder_is_still_taken_off() {
    let base = scratch!("layout-wrapped-root");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Wrapped.zip",
        &[("Wrapped/BepInEx/plugins/Mod/Mod.dll", "dll")],
    );

    let import = install_into(&zip, &library, &test_games::aliased(), DIGEST);
    assert_eq!(import.mods[0].slot, "BepInEx/plugins");
    assert_eq!(import.mods[0].files, 1);
    assert!(library.join("Wrapped/Mod.dll").is_file());
}
