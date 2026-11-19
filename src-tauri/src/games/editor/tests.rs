//! The editor rules: a draft in the shape the form writes, a folder inspection, the loader list and the one write.

use std::path::PathBuf;

use super::super::GameOrigin;
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
        &shipped,
        std::slice::from_ref(&loader_dir),
        &valid_text(),
        false,
    )
    .expect_err("a bundled id must be refused");
    assert_eq!(error.code, "games.definition_bundled");

    let saved = save_to(
        &user,
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
        &shipped,
        std::slice::from_ref(&loader_dir),
        &text,
        false,
    )
    .expect_err("an existing definition needs overwrite");
    assert_eq!(error.code, "games.definition_exists");

    save_to(&user, &shipped, &[loader_dir], &text, true).expect("overwrite");
}

#[test]
fn a_slug_folds_common_letters_and_drops_punctuation() {
    assert_eq!(
        slugify("The Witcher 3: Wild Hunt"),
        "the-witcher-3-wild-hunt"
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
    let presets = presets_from(&[(loaders(), GameOrigin::Bundled)]);
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
fn a_user_definition_wins_over_a_bundled_one() {
    let base = scratch!("editor-source");
    let user = base.join("user");
    let shipped = base.join("shipped");
    write(&shipped.join("mine").join("game.toml"), "bundled");
    write(&user.join("mine").join("game.toml"), "user");
    let source = source_from(Some(&user), &shipped, "mine").expect("the user file");
    assert_eq!(source.origin, GameOrigin::User);
    assert_eq!(source.text, "user");
}
