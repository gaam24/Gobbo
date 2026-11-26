//! The editor rules: a draft in the shape the form writes, a folder inspection, the loader list and the one write.

use std::path::PathBuf;

use super::super::{GameOrigin, layers::DefinitionLayer};
use super::check::{check_with, save_to, source_from};
use super::inspect::{inspect_folder, slugify};
use super::presets::presets_from;
use crate::test_support::games::SLOT;
use crate::test_support::{scratch, write};

fn valid_text() -> String {
    SLOT.to_string()
}

fn loaders() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../loaders")
}

#[test]
fn a_valid_draft_has_no_problems_and_a_summary() {
    let check = check_with(&valid_text(), &[loaders()]);
    assert!(check.ok, "{:?}", check.problems);
    assert_eq!(check.id, "slot");
    let summary = check.summary.expect("a summary");
    assert_eq!(summary.game.id, "slot");
    assert!(!summary.game.places.is_empty());
}

#[test]
fn a_broken_draft_names_the_section() {
    let text = valid_text().replace("path = \"bin/game.exe\"", "path = \"../game.exe\"");
    let check = check_with(&text, &[loaders()]);
    assert!(!check.ok);
    assert!(
        check
            .problems
            .iter()
            .any(|problem| problem.section == "detection")
    );
}

#[test]
fn a_draft_in_the_shape_the_form_writes_validates() {
    let text = r#"
schema = 1
id = "example"
name = "Example"

[detect]
markers = [ "Example.exe" ]

[[deploy.slots]]
dir = "Mods"

[[executables]]
name = "Example"
path = "Example.exe"
"#;
    let check = check_with(text, &[loaders()]);
    assert!(check.ok, "{:?}", check.problems);
    assert_eq!(
        check.summary.expect("a summary").game.places,
        vec!["Mods".to_string()]
    );
}

#[test]
fn a_loader_is_merged_into_the_summary() {
    let text = r#"
schema = 1
id = "unity"
name = "Unity"
loader = { id = "bepinex", variant = "6" }

[detect]
markers = ["Unity.exe"]

[[executables]]
name = "Unity"
path = "Unity.exe"
"#;
    let check = check_with(text, &[loaders()]);
    assert!(check.ok, "{:?}", check.problems);
    let summary = check.summary.expect("a summary");
    assert_eq!(summary.game.loader.expect("a loader").id, "bepinex");
    assert!(
        summary
            .game
            .places
            .iter()
            .any(|place| place == "BepInEx/plugins")
    );
}

#[test]
fn saving_refuses_a_bundled_id_and_writes_a_user_definition_only_once() {
    let base = scratch!("editor-save");
    let user = base.join("user");
    let shipped = base.join("shipped");
    write(&shipped.join("slot").join("game.toml"), &valid_text());
    let loader_dir = loaders();
    let text = valid_text().replace("id = \"slot\"", "id = \"mine\"");

    let error = save_to(
        &user,
        None,
        &shipped,
        std::slice::from_ref(&loader_dir),
        &valid_text(),
        false,
    )
    .expect_err("a bundled id must be refused");
    assert_eq!(error.code, "games.definition_bundled");

    let saved = save_to(
        &user,
        None,
        &shipped,
        std::slice::from_ref(&loader_dir),
        &text,
        false,
    )
    .expect("a new definition");
    assert_eq!(saved.id, "mine");
    assert!(user.join("mine").join("game.toml").is_file());

    let error = save_to(
        &user,
        None,
        &shipped,
        std::slice::from_ref(&loader_dir),
        &text,
        false,
    )
    .expect_err("an existing definition needs overwrite");
    assert_eq!(error.code, "games.definition_exists");

    save_to(&user, None, &shipped, &[loader_dir], &text, true).expect("overwrite");
}

#[test]
fn saving_refuses_an_official_id() {
    let base = scratch!("editor-save-official");
    let user = base.join("user");
    let catalog = base.join("catalog");
    let shipped = base.join("shipped");
    write(&catalog.join("slot").join("game.toml"), &valid_text());
    let loader_dir = loaders();

    let error = save_to(
        &user,
        Some(&catalog),
        &shipped,
        std::slice::from_ref(&loader_dir),
        &valid_text(),
        false,
    )
    .expect_err("an official id must be refused");
    assert_eq!(error.code, "games.definition_bundled");
    assert!(
        !user.join("slot").join("game.toml").exists(),
        "nothing is written for a refused id"
    );
}

#[test]
fn a_slug_folds_common_letters_and_drops_punctuation() {
    assert_eq!(
        slugify("Example Game 3: Second Chapter"),
        "example-game-3-second-chapter"
    );
    assert_eq!(slugify("Zażółć gęślą jaźń"), "zazolc-gesla-jazn");
    assert_eq!(slugify("..."), "game");
}

#[test]
fn a_folder_inspection_finds_the_program_the_folders_and_an_installed_loader() {
    let base = scratch!("editor-inspect");
    let game = base.join("Example Game");
    write(&game.join("Game.exe"), "");
    write(&game.join("bin/tool.exe"), "");
    write(&game.join("winhttp.dll"), "");
    write(&game.join("BepInEx/core/BepInEx.dll"), "");
    std::fs::create_dir_all(game.join("Mods")).unwrap();

    let inspection = inspect_folder(&game.display().to_string()).expect("the folder is readable");
    assert_eq!(inspection.suggested_id, "example-game");
    assert_eq!(inspection.executables[0].path, "Game.exe");
    assert!(inspection.directories.iter().any(|dir| dir == "Mods"));
    assert!(inspection.directories.iter().any(|dir| dir == "BepInEx"));
    let loader = inspection
        .loaders
        .iter()
        .find(|loader| loader.id == "bepinex")
        .expect("the BepInEx files are there");
    assert_eq!(loader.variant.as_deref(), Some("5"));
}

#[test]
fn a_loader_preset_list_reads_the_shipped_presets() {
    let presets = presets_from(&[DefinitionLayer {
        path: loaders(),
        origin: GameOrigin::Bundled,
    }]);
    let bepinex = presets
        .iter()
        .find(|preset| preset.id == "bepinex")
        .expect("the shipped BepInEx preset");
    assert_eq!(bepinex.default_variant.as_deref(), Some("5"));
    assert!(bepinex.variants.iter().any(|variant| variant.name == "6"));
    assert!(
        bepinex
            .places
            .iter()
            .any(|place| place == "BepInEx/plugins")
    );
}

#[test]
fn a_loader_preset_list_prefers_user_then_official_then_bundled() {
    let base = scratch!("presets-precedence");
    let text = |name: &str| {
        format!(
            "schema = 1\nid = \"fixture-loader\"\nname = \"{name}\"\n\n[deploy]\n[[deploy.slots]]\ndir = \"Mods\"\n"
        )
    };
    write(
        &base.join("bundled").join("fixture-loader.toml"),
        &text("Bundled Fixture Loader"),
    );
    write(
        &base.join("catalog").join("fixture-loader.toml"),
        &text("Catalog Fixture Loader"),
    );
    write(
        &base.join("user").join("fixture-loader.toml"),
        &text("User Fixture Loader"),
    );

    let presets = presets_from(&[
        DefinitionLayer {
            path: base.join("user"),
            origin: GameOrigin::User,
        },
        DefinitionLayer {
            path: base.join("catalog"),
            origin: GameOrigin::Official,
        },
        DefinitionLayer {
            path: base.join("bundled"),
            origin: GameOrigin::Bundled,
        },
    ]);
    let loader = presets
        .iter()
        .find(|preset| preset.id == "fixture-loader")
        .expect("the preset");
    assert_eq!(loader.name, "User Fixture Loader");
    assert_eq!(loader.origin, GameOrigin::User);
    assert!(
        presets
            .iter()
            .filter(|preset| preset.id == "fixture-loader")
            .count()
            == 1,
        "the shadowed copies are deduped"
    );
}

#[test]
fn a_user_definition_wins_over_an_official_and_a_bundled_one() {
    let base = scratch!("editor-source");
    let user = base.join("user");
    let catalog = base.join("catalog");
    let shipped = base.join("shipped");
    write(&shipped.join("mine").join("game.toml"), "bundled");
    write(&catalog.join("mine").join("game.toml"), "official");
    write(&user.join("mine").join("game.toml"), "user");

    let source = source_from(Some(&user), Some(&catalog), &shipped, "mine").expect("the user file");
    assert_eq!(source.origin, GameOrigin::User);
    assert_eq!(source.text, "user");

    let source = source_from(None, Some(&catalog), &shipped, "mine").expect("the official file");
    assert_eq!(source.origin, GameOrigin::Official);
    assert_eq!(source.text, "official");

    let source = source_from(None, None, &shipped, "mine").expect("the bundled file");
    assert_eq!(source.origin, GameOrigin::Bundled);
    assert_eq!(source.text, "bundled");
}

#[test]
fn the_shipped_presets_name_their_engines_and_load_without_problems() {
    let presets = presets_from(&[DefinitionLayer {
        path: loaders(),
        origin: GameOrigin::Bundled,
    }]);

    for preset in &presets {
        assert!(
            preset.problems.is_empty(),
            "{}: {:?}",
            preset.id,
            preset.problems
        );
    }

    let unity = presets
        .iter()
        .find(|preset| preset.id == "bepinex")
        .and_then(|preset| preset.engine.as_ref())
        .expect("BepInEx names its engine");
    assert_eq!(unity.name, "Unity");
    assert_eq!(unity.variant, None, "the default variant is the fallback");
    let variants: Vec<&str> = unity
        .variants
        .iter()
        .map(|variant| variant.variant.as_str())
        .collect();
    assert_eq!(variants, ["6", "5"], "IL2CPP is checked before Mono");
    assert_eq!(unity.variants[0].label.as_deref(), Some("IL2CPP"));

    let unreal = presets
        .iter()
        .find(|preset| preset.id == "unreal")
        .and_then(|preset| preset.engine.as_ref())
        .expect("the Unreal preset names its engine");
    assert_eq!(unreal.name, "Unreal Engine");
    assert_eq!(unreal.variant.as_deref(), Some("ue4ss-4"));
    assert_eq!(unreal.root.as_deref(), Some("*/Content/Paks"));
}

#[test]
fn a_unity_game_with_managed_code_and_no_il2cpp_runtime_is_mono() {
    let game = scratch!("editor-engine-mono").join("Example Game");
    write(&game.join("Example.exe"), "");
    write(&game.join("UnityPlayer.dll"), "");
    write(&game.join("Example_Data/Managed/Assembly-CSharp.dll"), "");

    let inspection = inspect_folder(&game.display().to_string()).expect("the folder is readable");
    let engine = inspection.engine.expect("the engine is recognised");
    assert_eq!(engine.loader_id, "bepinex");
    assert_eq!(engine.loader_name, "BepInEx");
    assert_eq!(engine.engine_name, "Unity");
    assert_eq!(engine.variant.as_deref(), Some("5"));
    assert_eq!(engine.label.as_deref(), Some("Mono"));
    assert_eq!(engine.root, None);
    assert!(inspection.loaders.is_empty(), "no loader is installed yet");
}

#[test]
fn a_unity_game_with_an_il2cpp_runtime_is_il2cpp_even_with_a_managed_folder() {
    let game = scratch!("editor-engine-il2cpp").join("Example Game");
    write(&game.join("Example.exe"), "");
    write(&game.join("UnityPlayer.dll"), "");
    write(&game.join("GameAssembly.dll"), "");
    write(&game.join("Example_Data/Managed/UnityEngine.dll"), "");
    write(
        &game.join("Example_Data/il2cpp_data/Metadata/global-metadata.dat"),
        "",
    );

    let inspection = inspect_folder(&game.display().to_string()).expect("the folder is readable");
    let engine = inspection.engine.expect("the engine is recognised");
    assert_eq!(engine.loader_id, "bepinex");
    assert_eq!(engine.variant.as_deref(), Some("6"));
    assert_eq!(engine.label.as_deref(), Some("IL2CPP"));
    assert_eq!(engine.root, None);
}

#[test]
fn an_unreal_game_takes_its_project_folder_and_never_the_engine_folder() {
    let game = scratch!("editor-engine-unreal").join("Example Game");
    write(&game.join("Engine/Binaries/Win64/Engine.dll"), "");
    write(&game.join("Engine/Content/Paks/Engine.pak"), "");
    write(&game.join("Project/Binaries/Win64/Project.exe"), "");
    write(&game.join("Project/Content/Paks/pakchunk0-Windows.pak"), "");

    let inspection = inspect_folder(&game.display().to_string()).expect("the folder is readable");
    let engine = inspection.engine.expect("the engine is recognised");
    assert_eq!(engine.loader_id, "unreal");
    assert_eq!(engine.engine_name, "Unreal Engine");
    assert_eq!(engine.variant.as_deref(), Some("ue4ss-4"));
    assert_eq!(engine.label, None);
    assert_eq!(engine.root.as_deref(), Some("Project"));
}

#[test]
fn a_folder_with_no_engine_marker_has_no_engine_and_no_loader() {
    let game = scratch!("editor-engine-none").join("Example Game");
    write(&game.join("readme.txt"), "");

    let inspection = inspect_folder(&game.display().to_string()).expect("the folder is readable");
    assert!(inspection.engine.is_none());
    assert!(inspection.loaders.is_empty());
}

#[test]
fn an_installed_loader_stays_the_choice_and_the_engine_is_still_reported() {
    let game = scratch!("editor-engine-installed").join("Example Game");
    write(&game.join("UnityPlayer.dll"), "");
    write(&game.join("Example_Data/Managed/Assembly-CSharp.dll"), "");
    write(&game.join("winhttp.dll"), "");
    write(&game.join("BepInEx/core/BepInEx.dll"), "");

    let inspection = inspect_folder(&game.display().to_string()).expect("the folder is readable");
    let installed = inspection
        .loaders
        .iter()
        .find(|loader| loader.id == "bepinex")
        .expect("the installed BepInEx is found");
    assert_eq!(installed.variant.as_deref(), Some("5"));
    let engine = inspection.engine.expect("the engine is still reported");
    assert_eq!(engine.loader_id, "bepinex");
    assert_eq!(engine.label.as_deref(), Some("Mono"));
}

/// The program paths a folder inspection offers, best first.
fn programs_of(game: &std::path::Path) -> Vec<String> {
    inspect_folder(&game.display().to_string())
        .expect("the folder is readable")
        .executables
        .into_iter()
        .map(|program| program.path)
        .collect()
}

#[test]
fn a_large_folder_that_sorts_first_does_not_hide_the_programs_at_the_root() {
    let game = scratch!("editor-inspect-large").join("Example Game");

    for index in 0..2_100 {
        write(
            &game.join("Assets").join(format!("asset-{index:04}.dat")),
            "",
        );
    }

    write(&game.join("Example.exe"), "");
    write(&game.join("Checksum.exe"), "");
    let programs = programs_of(&game);
    assert!(
        programs.contains(&"Example.exe".to_string()),
        "{programs:?}"
    );
    assert!(
        programs.contains(&"Checksum.exe".to_string()),
        "{programs:?}"
    );
}

#[test]
fn the_program_named_after_the_game_comes_first_then_the_others_and_a_helper_last() {
    let game = scratch!("editor-inspect-rank").join("Example's Game");
    write(&game.join("Checksum.exe"), &"x".repeat(5_000));
    write(&game.join("Launcher.exe"), "x");
    write(&game.join("Beta.exe"), &"x".repeat(500));
    write(&game.join("unins000.exe"), "x");
    write(&game.join("bin/Examples Game.exe"), "x");

    assert_eq!(
        programs_of(&game),
        [
            "bin/Examples Game.exe",
            "Beta.exe",
            "Launcher.exe",
            "Checksum.exe"
        ],
        "the name matches first, the larger file of two comes first, the helper is last, and the installer is not offered"
    );
}

#[test]
fn a_program_deep_in_the_engine_folder_is_offered_beside_the_root_program() {
    let game = scratch!("editor-inspect-deep").join("Example");
    write(&game.join("Example.exe"), "");
    write(
        &game.join("Project/Binaries/Win64/Example-Win64-Shipping.exe"),
        "",
    );
    write(
        &game.join("Project/Binaries/Win64/Example-Win64-Shipping.exe.tmp"),
        "",
    );

    let programs = programs_of(&game);
    assert_eq!(programs.first().map(String::as_str), Some("Example.exe"));
    assert!(
        programs.contains(&"Project/Binaries/Win64/Example-Win64-Shipping.exe".to_string()),
        "{programs:?}"
    );
}

#[test]
fn every_program_is_offered_not_only_the_first_few() {
    let game = scratch!("editor-inspect-many").join("Example Game");

    for index in 0..20 {
        write(&game.join(format!("Tool {index:02}.exe")), "");
    }

    assert_eq!(programs_of(&game).len(), 20);
}

#[test]
fn the_folder_list_is_shallow_first_and_stops_at_the_second_level() {
    let game = scratch!("editor-inspect-folders").join("Example Game");
    write(&game.join("Zed/Deep/Deeper/file.txt"), "");
    write(&game.join("Mods/readme.txt"), "");

    let inspection = inspect_folder(&game.display().to_string()).expect("the folder is readable");
    let folders = inspection.directories;
    let position = |wanted: &str| folders.iter().position(|folder| folder == wanted);
    assert!(position("Mods").is_some(), "{folders:?}");
    assert!(position("Zed/Deep").is_some(), "{folders:?}");
    assert!(position("Zed/Deep/Deeper").is_none(), "{folders:?}");
    assert!(position("Mods") < position("Zed/Deep"), "{folders:?}");
}

#[test]
fn a_program_whose_name_is_part_of_the_game_name_comes_before_an_unrelated_one() {
    let game = scratch!("editor-inspect-partial").join("Example Game Remastered");
    write(&game.join("Launcher.exe"), "x");
    write(&game.join("bin/Example Game.exe"), "x");

    assert_eq!(
        programs_of(&game),
        ["bin/Example Game.exe", "Launcher.exe"],
        "the program whose name is part of the game's name comes first, though it is one folder deeper"
    );
}
