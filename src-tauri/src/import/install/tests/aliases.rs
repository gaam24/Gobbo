//! Archive folder aliases (`[deploy.aliases]`): a folder at the archive root stands for a game folder, so a loader pack's
//! `plugins/`, `core/`, `config/`, `patchers/` and `monomod/` folders reach the game where the game reads them.

use super::*;
use crate::test_support::scratch;

#[test]
fn a_root_plugin_folder_becomes_a_plugin_of_the_slot() {
    let base = scratch!("alias-plugin");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "ExampleLib.zip",
        &[
            ("plugins/ExampleLib/ExampleLib.dll", "dll"),
            ("readme.txt", "hello"),
        ],
    );

    let import = install_zip(
        &zip,
        &library,
        &test_games::aliased(),
        DIGEST,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert_mods(
        import.mods,
        vec![installed(
            "ExampleLib",
            "BepInEx/plugins",
            &["ExampleLib.dll"],
            3,
        )],
    );
    assert_eq!(import.skipped, 1);
    assert!(library.join("ExampleLib/ExampleLib.dll").is_file());
}

#[test]
fn a_loader_pack_at_the_root_reaches_its_bepinex_folders() {
    let base = scratch!("alias-pack");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Pack.zip",
        &[
            ("core/BepInEx.dll", "core"),
            ("plugins/Mod/Mod.dll", "dll"),
            ("config/Mod.cfg", "cfg"),
        ],
    );

    let import = install_zip(
        &zip,
        &library,
        &test_games::aliased(),
        DIGEST,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert_mods(
        import.mods,
        vec![installed(
            "Pack",
            "BepInEx/plugins",
            &[
                "BepInEx/config/Mod.cfg",
                "BepInEx/core/BepInEx.dll",
                "Mod.dll",
            ],
            10,
        )],
    );
    assert_eq!(import.skipped, 0);
    assert!(library.join("Pack/BepInEx/core/BepInEx.dll").is_file());
}

#[test]
fn an_alias_is_found_under_a_wrapper_folder() {
    let base = scratch!("alias-wrapped");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Wrapped.zip",
        &[
            ("Wrapper/plugins/Mod/Mod.dll", "dll"),
            ("Wrapper/config/Mod.cfg", "cfg"),
        ],
    );

    let import = install_zip(
        &zip,
        &library,
        &test_games::aliased(),
        DIGEST,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert_mods(
        import.mods,
        vec![installed(
            "Wrapped",
            "BepInEx/plugins",
            &["BepInEx/config/Mod.cfg", "Mod.dll"],
            6,
        )],
    );
    assert!(library.join("Wrapped/BepInEx/config/Mod.cfg").is_file());
}

#[test]
fn an_alias_matches_the_folder_name_in_any_case() {
    let base = scratch!("alias-case");
    let library = base.join("library");
    let zip = make_zip(&base, "Case.zip", &[("PLUGINS/Mod/Mod.dll", "dll")]);

    let import = install_zip(
        &zip,
        &library,
        &test_games::aliased(),
        DIGEST,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(import.mods[0].slot, "BepInEx/plugins");
    assert_eq!(import.mods[0].paths, vec!["Mod.dll".to_string()]);
}

#[test]
fn a_root_plugin_folder_is_no_mod_without_an_alias() {
    let base = scratch!("alias-missing");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "ExampleLib.zip",
        &[("plugins/ExampleLib/ExampleLib.dll", "dll")],
    );

    let err = install_zip(&zip, &library, &plugin_game(), DIGEST, None, &mut |_| {}).unwrap_err();
    assert!(matches!(err, InstallError::UnknownLayout));
    assert!(!library.exists(), "a refused archive writes nothing");
}

#[test]
fn loader_folders_at_the_archive_root_reach_their_bepinex_folders() {
    let base = scratch!("alias-loader-folders");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Pack.zip",
        &[
            ("plugins/Mod/Mod.dll", "dll"),
            ("patchers/Mod.Patcher.dll", "patcher"),
            ("monomod/Mod.Mono.dll", "mono"),
        ],
    );

    // The shipped BepInEx preset: `plugins/`, `patchers/` and `monomod/` at the root stand for its folders.
    let import = install_zip(
        &zip,
        &library,
        &test_games::load("lethal-company"),
        DIGEST,
        None,
        &mut |_| {},
    )
    .unwrap();
    assert_mods(
        import.mods,
        vec![installed(
            "Pack",
            "BepInEx/plugins",
            &[
                "BepInEx/monomod/Mod.Mono.dll",
                "BepInEx/patchers/Mod.Patcher.dll",
                "Mod.dll",
            ],
            14,
        )],
    );
    assert_eq!(import.skipped, 0);
    assert!(
        library
            .join("Pack/BepInEx/patchers/Mod.Patcher.dll")
            .is_file()
    );
    assert!(library.join("Pack/BepInEx/monomod/Mod.Mono.dll").is_file());
}
