//! Unreal Engine archives: pak/IoStore files, `LogicMods` and the UE4SS mod folders. The game takes the shipped preset,
//! so the test covers the real rules instead of a copied layout.

use super::*;
use crate::test_support::{installed, make_zip, scratch};

fn unreal_game() -> GameDef {
    test_games::unreal()
}

const UE4SS: &str = "Binaries/Win64/ue4ss/Mods";

#[test]
fn a_bare_pak_and_its_iostore_files_keep_their_names_in_one_flat_slot() {
    let base = scratch!("unreal-pak");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "BetterBeast.zip",
        &[
            ("BetterBeast.pak", "pak"),
            ("BetterBeast.utoc", "utoc"),
            ("BetterBeast.ucas", "ucas"),
            ("BetterBeast.sig", "sig"),
            ("readme.txt", "hello"),
        ],
    );

    let import = install_into(&zip, &library, &unreal_game(), DIGEST);
    assert_mods(
        import.mods,
        vec![installed(
            "BetterBeast",
            "Content/Paks/~mods",
            &[
                "BetterBeast.pak",
                "BetterBeast.sig",
                "BetterBeast.ucas",
                "BetterBeast.utoc",
            ],
            14,
        )],
    );
    assert_eq!(import.skipped, 1);
    assert!(library.join("BetterBeast/BetterBeast.pak").is_file());
}

#[test]
fn a_pak_under_the_game_folder_is_flattened_into_the_slot() {
    let base = scratch!("unreal-pak-wrapped");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Wrapped.zip",
        &[("Content/Paks/~mods/Wrapped.pak", "pak")],
    );

    let import = install_into(&zip, &library, &unreal_game(), DIGEST);
    assert_eq!(import.mods.len(), 1);
    assert_eq!(import.mods[0].folder, "Wrapped");
    assert_eq!(import.mods[0].slot, "Content/Paks/~mods");
    assert_eq!(import.mods[0].paths, ["Wrapped.pak"]);
    assert!(library.join("Wrapped/Wrapped.pak").is_file());
}

#[test]
fn a_folder_around_a_pak_names_the_mod_and_the_pak_stays_flat() {
    let base = scratch!("unreal-pak-folder");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Pack.zip",
        &[("Pack/Pack.pak", "pak"), ("Pack/readme.txt", "hello")],
    );

    let import = install_into(&zip, &library, &unreal_game(), DIGEST);
    assert_eq!(import.mods.len(), 1);
    assert_eq!(import.mods[0].folder, "Pack");
    assert_eq!(import.mods[0].slot, "Content/Paks/~mods");
    assert_eq!(import.mods[0].paths, ["Pack.pak"]);
    assert_eq!(import.skipped, 1);
}

#[test]
fn a_blueprint_pak_keeps_the_logic_mods_path() {
    let base = scratch!("unreal-logic-mods");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Blueprint.zip",
        &[
            ("LogicMods/Blueprint.pak", "pak"),
            ("LogicMods/Blueprint/config.lua", "return {}"),
        ],
    );

    let import = install_into(&zip, &library, &unreal_game(), DIGEST);
    assert_eq!(import.mods.len(), 1);
    assert_eq!(import.mods[0].slot, "");
    assert_eq!(
        import.mods[0].paths,
        [
            "Content/Paks/LogicMods/Blueprint.pak",
            "Content/Paks/LogicMods/Blueprint/config.lua",
        ]
    );
    assert!(
        library
            .join("Blueprint/Content/Paks/LogicMods/Blueprint.pak")
            .is_file()
    );
    assert!(
        library
            .join("Blueprint/Content/Paks/LogicMods/Blueprint/config.lua")
            .is_file()
    );
}

#[test]
fn a_lua_mod_is_found_by_its_scripts_folder_and_gets_enabled_txt() {
    let base = scratch!("unreal-lua");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "LuaMod.zip",
        &[("BetterBeast/Scripts/main.lua", "print('hello')")],
    );

    let import = install_into(&zip, &library, &unreal_game(), DIGEST);
    assert_eq!(import.mods[0].integrity.as_ref().unwrap().files, 2);
    assert_mods(
        import.mods,
        vec![installed(
            "LuaMod",
            UE4SS,
            &["Scripts/main.lua", "enabled.txt"],
            14,
        )],
    );
    let enabled = library.join("LuaMod/enabled.txt");
    assert!(enabled.is_file());
    assert_eq!(fs::read_to_string(enabled).unwrap(), "");
}

#[test]
fn a_cpp_mod_is_found_by_its_dlls_folder_and_gets_enabled_txt() {
    let base = scratch!("unreal-cpp");
    let library = base.join("library");
    let zip = make_zip(&base, "CppMod.zip", &[("BetterBeast/dlls/main.dll", "dll")]);

    let import = install_into(&zip, &library, &unreal_game(), DIGEST);
    assert_mods(
        import.mods,
        vec![installed(
            "CppMod",
            UE4SS,
            &["dlls/main.dll", "enabled.txt"],
            3,
        )],
    );
}

#[test]
fn a_mods_wrapper_is_found_by_the_same_markers() {
    let base = scratch!("unreal-mods-wrapper");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Wrapped.zip",
        &[("Mods/BetterBeast/Scripts/main.lua", "print('hello')")],
    );

    let import = install_into(&zip, &library, &unreal_game(), DIGEST);
    assert_eq!(import.mods.len(), 1);
    assert_eq!(import.mods[0].folder, "Wrapped");
    assert_eq!(import.mods[0].slot, UE4SS);
    assert_eq!(import.mods[0].paths, ["Scripts/main.lua", "enabled.txt"]);
}

#[test]
fn markers_match_case_insensitively_and_share_one_root() {
    let base = scratch!("unreal-markers-case");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Both.zip",
        &[
            ("BetterBeast/scripts/main.lua", "x"),
            ("BetterBeast/dlls/main.dll", "dll"),
        ],
    );

    let import = install_into(&zip, &library, &unreal_game(), DIGEST);
    assert_mods(
        import.mods,
        vec![installed(
            "Both",
            UE4SS,
            &["dlls/main.dll", "enabled.txt", "scripts/main.lua"],
            4,
        )],
    );
}

#[test]
fn a_reinstall_keeps_only_the_enable_file_of_the_chosen_mod() {
    let base = scratch!("unreal-reinstall");
    let zip = make_zip(
        &base,
        "TwoMods.zip",
        &[
            ("Alpha/Scripts/main.lua", "a"),
            ("Beta/Scripts/main.lua", "b"),
        ],
    );
    let first = base.join("library");
    let import = install_zip_named_with_roots(
        &zip,
        &first,
        &unreal_game(),
        Naming {
            base_name: "TwoMods",
            digest: DIGEST,
            only: None,
        },
        None,
        None,
        &[],
        &mut |_| {},
    )
    .unwrap();
    let alpha = import
        .mods
        .iter()
        .find(|installed| installed.folder.contains("Alpha"))
        .expect("the Alpha mod")
        .folder
        .clone();

    let second = base.join("library2");
    let again = install_zip_named_with_roots(
        &zip,
        &second,
        &unreal_game(),
        Naming {
            base_name: "TwoMods",
            digest: DIGEST,
            only: Some(alpha.as_str()),
        },
        None,
        None,
        &[],
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(again.mods.len(), 1);
    assert_eq!(again.mods[0].folder, alpha);
    assert!(second.join(&alpha).join("enabled.txt").is_file());
}

#[test]
fn an_archive_that_ships_enabled_txt_keeps_its_copy() {
    let base = scratch!("unreal-enabled-present");
    let library = base.join("library");
    let zip = make_zip(
        &base,
        "Present.zip",
        &[
            ("BetterBeast/Scripts/main.lua", "x"),
            ("BetterBeast/enabled.txt", "1"),
        ],
    );

    let import = install_into(&zip, &library, &unreal_game(), DIGEST);
    assert_eq!(import.mods.len(), 1);
    assert_eq!(import.mods[0].paths, ["Scripts/main.lua", "enabled.txt"]);
    assert_eq!(import.mods[0].size_bytes, 2);
    assert_eq!(
        fs::read_to_string(library.join("Present/enabled.txt")).unwrap(),
        "1"
    );
}
