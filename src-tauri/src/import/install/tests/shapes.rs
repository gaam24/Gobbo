//! The mod shapes a definition can describe: marker folders, plugins and flat slots.

use super::*;
use crate::test_support::scratch;

#[test]
fn a_marker_folder_is_a_mod_even_without_a_mods_wrapper() {
    let base = scratch!("marker-unit");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "GoodModlet.zip",
        &[
            ("GoodModlet/ModInfo.xml", "<xml/>"),
            ("GoodModlet/Config/items.xml", "<items/>"),
            ("readme.txt", "hello"),
        ],
    );

    let import = install_zip(&zip, &library, &external_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert_mods(
        import.mods,
        vec![installed(
            "GoodModlet",
            "Mods",
            &["Config/items.xml", "ModInfo.xml"],
            14,
        )],
    );
    assert_eq!(import.skipped, 1);
    assert!(library.join("GoodModlet/ModInfo.xml").is_file());
    assert!(library.join("GoodModlet/Config/items.xml").is_file());
}

#[test]
fn a_marker_folder_under_the_mods_folder_is_found() {
    let base = scratch!("marker-wrapped");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Wrapped.zip",
        &[
            ("Mods/Wrapped/ModInfo.xml", "<xml/>"),
            ("Mods/Wrapped/Config/x.xml", "<x/>"),
        ],
    );

    let import = install_zip(&zip, &library, &external_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert_eq!(import.mods[0].folder, "Wrapped");
    assert_eq!(import.mods[0].files, 2);
    assert!(library.join("Wrapped/Config/x.xml").is_file());
    assert!(!library.join("Mods").exists());
}

#[test]
fn every_marker_folder_becomes_its_own_mod() {
    let base = scratch!("marker-many");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Pack.zip",
        &[
            ("A/ModInfo.xml", "<xml/>"),
            ("A/Config/a.xml", "<a/>"),
            ("B/ModInfo.xml", "<xml/>"),
            ("B/Config/b.xml", "<b/>"),
        ],
    );

    let import = install_zip(&zip, &library, &external_game(), DIGEST, None, &mut |_| {}).unwrap();
    let folders: Vec<&str> = import.mods.iter().map(|m| m.folder.as_str()).collect();
    assert_eq!(folders, ["Pack - A", "Pack - B"]);
    assert!(library.join("Pack - A/Config/a.xml").is_file());
    assert!(library.join("Pack - B/Config/b.xml").is_file());
}

#[test]
fn a_bare_plugin_gets_a_folder_of_its_own() {
    let base = scratch!("plugin-bare");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "FastShop.zip",
        &[("FastShop.dll", "dll"), ("readme.txt", "hello")],
    );

    let import = install_zip(&zip, &library, &plugin_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert_mods(
        import.mods,
        vec![installed(
            "FastShop",
            "BepInEx/plugins",
            &["FastShop.dll"],
            3,
        )],
    );
    assert_eq!(import.skipped, 1);
    assert!(library.join("FastShop/FastShop.dll").is_file());
}

#[test]
fn a_plugin_under_the_slot_folder_keeps_its_files() {
    let base = scratch!("plugin-slot");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Named.zip",
        &[("BepInEx/plugins/FastShop/FastShop.dll", "dll")],
    );

    let import = install_zip(&zip, &library, &plugin_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert_eq!(import.mods[0].folder, "Named");
    assert_eq!(import.mods[0].files, 1);
    assert!(library.join("Named/FastShop.dll").is_file());
}

#[test]
fn a_flat_slot_accepts_only_its_extension_and_flattens_it() {
    let base = scratch!("flat-slot-pak");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "BetterBeast.zip",
        &[("data_custom.pak", "pak"), ("readme.txt", "hello")],
    );

    let import = install_zip(&zip, &library, &flat_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert_mods(
        import.mods,
        vec![installed(
            "BetterBeast",
            "ph_ft/source",
            &["data_custom.pak"],
            3,
        )],
    );
    assert_eq!(import.skipped, 1);
    assert!(library.join("BetterBeast/data_custom.pak").is_file());
    assert!(!library.join("BetterBeast/readme.txt").exists());
}

#[test]
fn a_flat_slot_takes_a_root_folder_as_one_mod() {
    let base = scratch!("flat-slot-root-folder");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Pack.zip",
        &[
            ("Pack/data_custom.pak", "pak"),
            ("Pack/readme.txt", "hello"),
        ],
    );
    let mut game = flat_game();
    game.deploy.slots[0].root_folder_extensions = vec![".pak".to_string()];

    let import = install_zip(&zip, &library, &game, DIGEST, None, &mut |_| {}).unwrap();
    assert_mods(
        import.mods,
        vec![installed("Pack", "ph_ft/source", &["data_custom.pak"], 3)],
    );
    assert_eq!(import.skipped, 1);
    assert!(library.join("Pack/data_custom.pak").is_file());
    assert!(!library.join("Pack/readme.txt").exists());
}

#[test]
fn a_flat_slot_wins_over_its_protected_parent() {
    let base = scratch!("flat-slot-protected-parent");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Source.zip",
        &[("ph_ft/source/data_custom.pak", "pak")],
    );
    let mut game = flat_game();
    game.deploy.protected = vec!["ph_ft".to_string()];

    let import = install_zip(&zip, &library, &game, DIGEST, None, &mut |_| {}).unwrap();
    assert_eq!(import.mods[0].folder, "Source");
    assert_eq!(import.mods[0].files, 1);
    assert!(library.join("Source/data_custom.pak").is_file());
}

#[test]
fn a_flat_slot_finds_a_file_under_its_folder() {
    let base = scratch!("flat-slot-source");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Source.zip",
        &[("ph_ft/source/data_custom.pak", "pak")],
    );

    let import = install_zip(&zip, &library, &flat_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert_eq!(import.mods[0].folder, "Source");
    assert_eq!(import.mods[0].files, 1);
    assert!(library.join("Source/data_custom.pak").is_file());
}

#[test]
fn a_bare_file_named_as_a_root_file_keeps_its_name_in_the_game_folder() {
    let base = scratch!("root-file-import");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Loader.zip",
        &[
            ("winhttp.dll", "proxy"),
            ("doorstop_config.ini", "[general]"),
            ("BepInEx/core/BepInEx.dll", "core"),
        ],
    );

    let plugin = crate::test_support::games::plugin();
    let import = install_zip(&zip, &library, &plugin, DIGEST, None, &mut |_| {}).unwrap();
    assert_eq!(import.mods.len(), 1);
    assert!(import.mods[0].paths.contains(&"winhttp.dll".to_string()));
    assert!(
        import.mods[0]
            .paths
            .contains(&"BepInEx/core/BepInEx.dll".to_string())
    );
    assert_eq!(import.skipped, 1, "the config file is no rule's file");
    assert!(library.join("Loader/winhttp.dll").is_file());
    assert!(!library.join("Loader/doorstop_config.ini").exists());
}

#[test]
fn a_plugin_folder_at_the_archive_root_is_a_mod() {
    let base = scratch!("plugin-folder");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "UnlockPriceOverride.zip",
        &[
            ("UnlockPriceOverride/UnlockPriceOverride.dll", "dll"),
            ("readme.txt", "hello"),
        ],
    );

    let import = install_zip(&zip, &library, &plugin_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert_mods(
        import.mods,
        vec![installed(
            "UnlockPriceOverride",
            "BepInEx/plugins",
            &["UnlockPriceOverride.dll"],
            3,
        )],
    );
    assert_eq!(import.skipped, 1);
    assert!(
        library
            .join("UnlockPriceOverride/UnlockPriceOverride.dll")
            .is_file()
    );
}

#[test]
fn every_plugin_folder_becomes_its_own_mod() {
    let base = scratch!("plugin-folder-many");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Pack.zip",
        &[
            ("PriceA/PriceA.dll", "a"),
            ("PriceB/PriceB.dll", "b"),
            ("PriceB/PriceB.cfg", "cfg"),
        ],
    );

    let import = install_zip(&zip, &library, &plugin_game(), DIGEST, None, &mut |_| {}).unwrap();
    let folders: Vec<&str> = import.mods.iter().map(|m| m.folder.as_str()).collect();
    assert_eq!(folders, ["Pack - PriceA", "Pack - PriceB"]);
    assert!(library.join("Pack - PriceB/PriceB.dll").is_file());
}

#[test]
fn a_readme_in_each_of_several_plugin_roots_is_skipped() {
    let base = scratch!("plugin-roots-readme");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Pack.zip",
        &[
            ("PriceA/PriceA.dll", "a"),
            ("PriceA/readme.txt", "readme a"),
            ("PriceB/PriceB.dll", "b"),
            ("PriceB/readme.txt", "readme b"),
            ("PriceC/PriceC.dll", "c"),
            ("PriceC/readme.txt", "readme c"),
        ],
    );

    let mut game = plugin_game();
    game.deploy.slots[0].extensions = vec![".dll".to_string()];
    let import = install_zip(&zip, &library, &game, DIGEST, None, &mut |_| {}).unwrap();
    let folders: Vec<&str> = import.mods.iter().map(|m| m.folder.as_str()).collect();
    assert_eq!(folders, ["Pack - PriceA", "Pack - PriceB", "Pack - PriceC"]);
    assert_eq!(import.skipped, 3);

    for folder in ["Pack - PriceA", "Pack - PriceB", "Pack - PriceC"] {
        assert!(!library.join(folder).join("readme.txt").is_file());
    }
}

#[test]
fn a_folder_without_a_plugin_at_its_top_is_no_mod() {
    let base = scratch!("plugin-folder-none");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Docs.zip",
        &[
            ("Docs/readme.txt", "hello"),
            ("Nested/Deep/plugin.dll", "dll"),
        ],
    );

    let err = install_zip(&zip, &library, &plugin_game(), DIGEST, None, &mut |_| {}).unwrap_err();
    assert!(matches!(err, InstallError::UnknownLayout));
    assert!(!library.exists(), "a refused archive writes nothing");
}

#[test]
fn a_game_folder_with_a_loose_plugin_is_no_mod() {
    let base = scratch!("plugin-game-folder");
    let library = base.join("library");
    let zip = make_zip(&base, "Stray.zip", &[("BepInEx/Stray.dll", "dll")]);

    let err = install_zip(&zip, &library, &plugin_game(), DIGEST, None, &mut |_| {}).unwrap_err();
    assert!(matches!(err, InstallError::UnknownLayout));
}

#[test]
fn a_slot_without_root_folders_takes_no_plugin_folder() {
    let base = scratch!("slot-no-root-folder");
    let library = base.join("library");
    let zip = make_zip(&base, "Pack.zip", &[("Pack/Pack.dll", "dll")]);

    let err = install_zip(&zip, &library, &slot_game(), DIGEST, None, &mut |_| {}).unwrap_err();
    assert!(matches!(err, InstallError::UnknownLayout));
}

#[test]
fn a_config_file_of_a_plugin_keeps_its_bepinex_path() {
    let base = scratch!("plugin-config");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Mod.zip",
        &[
            ("BepInEx/plugins/Mod/Mod.dll", "dll"),
            ("BepInEx/config/Mod.cfg", "cfg"),
        ],
    );

    let import = install_zip(&zip, &library, &plugin_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert_mods(
        import.mods,
        vec![installed(
            "Mod",
            "BepInEx/plugins",
            &["BepInEx/config/Mod.cfg", "Mod.dll"],
            6,
        )],
    );
    assert_eq!(import.skipped, 0);
    assert!(library.join("Mod/BepInEx/config/Mod.cfg").is_file());
}

#[test]
fn a_config_only_archive_is_a_mod_without_a_slot() {
    let base = scratch!("config-only");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Settings.zip",
        &[("BepInEx/config/Settings.cfg", "cfg")],
    );

    let import = install_zip(&zip, &library, &plugin_game(), DIGEST, None, &mut |_| {}).unwrap();
    assert_mods(
        import.mods,
        vec![installed(
            "Settings",
            "",
            &["BepInEx/config/Settings.cfg"],
            3,
        )],
    );
    assert!(
        library
            .join("Settings/BepInEx/config/Settings.cfg")
            .is_file()
    );
}
