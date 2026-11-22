//! Loader presets: the reference in `game.toml`, the merge under the game's own rules, variants and every shipped
//! preset.

use std::fs;

use super::super::loaders as presets;
use super::*;
use crate::test_support::games::{LOADER, SLOT};
use crate::test_support::{scratch, write};

const GAME_ROOT: &str = concat!("{", "game", "}");

/// A game with one place, enough for a loader that brings its own slots, mirrors and aliases.
const MINIMAL_GAME: &str = r#"
schema = 1
id = "loader-test"
name = "Loader Test"

[detect]
markers = ["game.exe"]

[[executables]]
name = "Game"
path = "game.exe"

[deploy]
[[deploy.slots]]
dir = "Mods"
"#;

fn loader_text_with(loader: &str) -> String {
    SLOT.replacen(
        "providers = { nexus = { key = \"slot\" } }",
        &format!("loader = {loader}"),
        1,
    )
}

#[test]
fn a_game_can_take_its_whole_layout_from_a_loader() {
    let game = crate::test_support::games::loaded();
    assert_eq!(game.requires, ["proxy.dll", "Core/b.dll"]);
    assert_eq!(game.deploy.root_files, ["proxy.dll"]);
    assert!(
        game.deploy.slots.iter().any(|slot| slot.dir == "Mods"),
        "{:?}",
        game.deploy.slots
    );
    assert!(
        game.deploy.mirror.iter().any(|mirror| mirror.dir == "Core")
            && game
                .deploy
                .mirror
                .iter()
                .any(|mirror| mirror.dir == "Extra"),
        "{:?}",
        game.deploy.mirror
    );
}

#[test]
fn the_view_names_the_loader_and_the_variant_it_uses() {
    let chosen = crate::test_support::games::loaded()
        .view()
        .loader
        .expect("the game names a loader");
    assert_eq!(chosen.id, "fixture-loader");
    assert_eq!(chosen.name, "Fixture Loader");
    assert_eq!(chosen.variant.as_deref(), Some("b"));
    assert!(!chosen.default_variant);

    let default = crate::test_support::games::load(crate::test_support::games::LOADED_DEFAULT_ID)
        .view()
        .loader
        .expect("the game names a loader");
    assert_eq!(default.variant.as_deref(), Some("a"));
    assert!(default.default_variant);

    let none = GameDef::parse(MINIMAL_GAME).unwrap().view();
    assert!(none.loader.is_none());
}

#[test]
fn a_loader_is_a_string_or_a_table_and_its_keys_are_checked() {
    let shorthand = GameDef::parse(&loader_text_with("\"bepinex\"")).unwrap();
    assert_eq!(shorthand.loader.as_ref().unwrap().id(), "bepinex");
    assert_eq!(shorthand.loader.as_ref().unwrap().variant(), None);

    let chosen =
        GameDef::parse(&loader_text_with("{ id = \"bepinex\", variant = \"6\" }")).unwrap();
    assert_eq!(chosen.loader.as_ref().unwrap().id(), "bepinex");
    assert_eq!(chosen.loader.as_ref().unwrap().variant(), Some("6"));

    // An unknown key, a table without an id and a value of another type are each refused.
    for bad in [
        "{ id = \"bepinex\", extra = \"x\" }",
        "{ variant = \"6\" }",
        "5",
    ] {
        assert!(GameDef::parse(&loader_text_with(bad)).is_err(), "{bad}");
    }
}

#[test]
fn a_bad_loader_reference_is_a_definition_problem() {
    let game = GameDef::parse(&loader_text_with("\"../evil\"")).unwrap();
    assert!(
        game.validate()
            .iter()
            .any(|problem| problem.contains("loader id")),
        "{:?}",
        game.validate()
    );

    let game =
        GameDef::parse(&loader_text_with("{ id = \"bepinex\", variant = \"6/0\" }")).unwrap();
    assert!(
        game.validate()
            .iter()
            .any(|problem| problem.contains("loader variant")),
        "{:?}",
        game.validate()
    );
}

#[test]
fn a_loader_is_read_from_its_own_file() {
    let base = scratch!("loader-file");
    write(&base.join("fixture-loader.toml"), LOADER);
    let loader = presets::load(&base, "fixture-loader").expect("the fixture loader loads");
    assert_eq!(loader.id, "fixture-loader");
    assert_eq!(loader.default_variant.as_deref(), Some("a"));

    let error = presets::load(&base, "../evil").unwrap_err();
    assert!(error.contains("not a slug"), "{error}");

    let error = presets::load(&base, "missing").unwrap_err();
    assert!(error.contains("missing.toml"), "{error}");

    write(&base.join("wrong.toml"), LOADER);
    let error = presets::load(&base, "wrong").unwrap_err();
    assert!(error.contains("does not match"), "{error}");
}

#[test]
fn a_variant_is_added_to_the_base_and_the_default_is_used_when_none_is_named() {
    let loader = presets::LoaderDef::parse(LOADER).unwrap();

    let default = loader.rules(None).unwrap();
    assert_eq!(default.requires, ["proxy.dll", "Core/a.dll"]);
    assert!(
        default
            .deploy
            .mirror
            .iter()
            .all(|mirror| mirror.dir != "Extra")
    );

    let chosen = loader.rules(Some("b")).unwrap();
    assert_eq!(chosen.requires, ["proxy.dll", "Core/b.dll"]);
    assert!(
        chosen
            .deploy
            .mirror
            .iter()
            .any(|mirror| mirror.dir == "Extra"),
        "{:?}",
        chosen.deploy.mirror
    );

    let error = loader.rules(Some("missing")).unwrap_err();
    assert!(error.contains("no variant"), "{error}");
}

#[test]
fn the_game_rules_win_over_the_loader_and_can_add_to_it() {
    let text = r#"
schema = 1
id = "merge-game"
name = "Merge Game"
loader = { id = "fixture-loader", variant = "b" }
requires = ["proxy.dll", "extra.dll"]

[detect]
markers = ["game.exe"]

[[executables]]
name = "Game"
path = "game.exe"

[deploy]
root = "{game}"

[[deploy.slots]]
dir = "Mods"
prefix = "mod"

[[deploy.mirror]]
dir = "Core"
extensions = [".bin"]

[deploy.aliases]
plugins = "Extra"

[[deploy.mirror]]
dir = "Extra"
"#;
    let mut game = GameDef::parse(text).unwrap();
    presets::LoaderDef::parse(LOADER)
        .unwrap()
        .rules(Some("b"))
        .unwrap()
        .apply_to(&mut game);
    assert!(game.validate().is_empty(), "{:?}", game.validate());

    assert_eq!(game.requires, ["proxy.dll", "Core/b.dll", "extra.dll"]);
    assert_eq!(game.deploy.root.as_deref(), Some(GAME_ROOT));
    let slot = game
        .deploy
        .slots
        .iter()
        .find(|slot| slot.dir == "Mods")
        .expect("the loader's slot");
    assert_eq!(slot.prefix, "mod");
    assert!(slot.root_extensions.is_empty());
    let core = game
        .deploy
        .mirror
        .iter()
        .find(|mirror| mirror.dir == "Core")
        .expect("the core mirror");
    assert_eq!(core.extensions, [".bin"]);
    assert_eq!(
        game.deploy.aliases.get("plugins").map(String::as_str),
        Some("Extra")
    );
}

#[test]
fn a_loader_for_a_newer_app_is_refused() {
    let text = LOADER.replacen("name = ", "min_gobbo = \"999.0.0\"\nname = ", 1);
    let problems = presets::LoaderDef::parse(&text).unwrap().validate();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("loader needs Gobbo 999.0.0")),
        "{problems:?}"
    );
}

#[test]
fn loader_validation_checks_the_variant_and_the_root() {
    let no_default = LOADER.replacen("default_variant = \"a\"\n", "", 1);
    let problems = presets::LoaderDef::parse(&no_default).unwrap().validate();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("default_variant")),
        "{problems:?}"
    );

    let with_default = r#"
schema = 1
id = "x"
name = "X"
default_variant = "5"
"#;
    let problems = presets::LoaderDef::parse(with_default).unwrap().validate();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("no variants")),
        "{problems:?}"
    );

    let with_root = LOADER.replacen("[deploy]\n", "[deploy]\nroot = \"{appdata}/Example\"\n", 1);
    let problems = presets::LoaderDef::parse(&with_root).unwrap().validate();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("deployment root")),
        "{problems:?}"
    );

    let with_variant_root = r#"
schema = 1
id = "x"
name = "X"
[deploy]
[[deploy.slots]]
dir = "Mods"
[variants."a"]
[variants."a".deploy]
root = "{appdata}/Example"
"#;
    let problems = presets::LoaderDef::parse(with_variant_root)
        .unwrap()
        .validate();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("deployment root")),
        "{problems:?}"
    );

    let bad_variant = r#"
schema = 1
id = "x"
name = "X"
default_variant = "BAD"
[variants."BAD"]
"#;
    let problems = presets::LoaderDef::parse(bad_variant).unwrap().validate();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("variant name")),
        "{problems:?}"
    );

    let bad_id = r#"
schema = 1
id = "Bad"
name = "Bad"
"#;
    let problems = presets::LoaderDef::parse(bad_id).unwrap().validate();
    assert!(
        problems.iter().any(|problem| problem.contains("loader id")),
        "{problems:?}"
    );

    let empty_name = LOADER.replacen("name = \"Fixture Loader\"", "name = \"\"", 1);
    let problems = presets::LoaderDef::parse(&empty_name).unwrap().validate();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("name is empty")),
        "{problems:?}"
    );

    let bad_require = LOADER.replacen(
        "requires = [\"proxy.dll\"]",
        "requires = [\"/proxy.dll\"]",
        1,
    );
    let problems = presets::LoaderDef::parse(&bad_require).unwrap().validate();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("required file")),
        "{problems:?}"
    );
}

#[test]
fn a_duplicate_inside_one_layer_is_not_hidden_by_the_merge() {
    let duplicate_slots = SLOT.replacen(
        "[[deploy.slots]]\ndir = \"Mods\"",
        "[[deploy.slots]]\ndir = \"Mods\"\n\n[[deploy.slots]]\ndir = \"mods\"",
        1,
    );
    let game = GameDef::parse(&duplicate_slots).unwrap();
    let problems = super::super::validate::duplicate_problems(&game.deploy);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("slot or mirror twice")),
        "{problems:?}"
    );

    let duplicate_alias = LOADER.replacen(
        "[deploy.aliases]\nplugins = \"Mods\"",
        "[deploy.aliases]\nplugins = \"Mods\"\nPlugins = \"Core\"",
        1,
    );
    let problems = presets::LoaderDef::parse(&duplicate_alias)
        .unwrap()
        .validate();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("alias") && problem.contains("repeated")),
        "{problems:?}"
    );
}

#[test]
fn every_shipped_loader_and_variant_merges_into_a_valid_game() {
    let dir = Path::new(super::super::DEV_LOADERS_DIR);

    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let is_toml = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"));
        if !path.is_file() || !is_toml || name.starts_with('_') {
            continue;
        }

        let id = path.file_stem().unwrap().to_string_lossy().into_owned();
        let loader = presets::load(dir, &id).unwrap_or_else(|error| panic!("{name}: {error}"));
        let mut variants: Vec<Option<&str>> = loader
            .variants
            .keys()
            .map(String::as_str)
            .map(Some)
            .collect();
        if variants.is_empty() {
            variants.push(None);
        }

        for variant in variants {
            let mut game = GameDef::parse(MINIMAL_GAME).unwrap();
            loader
                .rules(variant)
                .unwrap_or_else(|error| panic!("{name} {variant:?}: {error}"))
                .apply_to(&mut game);
            assert!(
                game.validate().is_empty(),
                "{name} {variant:?}: {:?}",
                game.validate()
            );
        }
    }
}

#[test]
fn the_unreal_preset_has_the_pak_and_ue4ss_layouts() {
    let dir = Path::new(super::super::DEV_LOADERS_DIR);
    let loader = presets::load(dir, "unreal").unwrap();
    assert_eq!(loader.default_variant.as_deref(), Some("pak"));

    let pak = loader.rules(None).unwrap();
    let slot = pak
        .deploy
        .slots
        .iter()
        .find(|slot| slot.dir == "Content/Paks/~mods")
        .expect("the pak slot");
    assert!(slot.flat);
    assert!(slot.extensions.iter().any(|extension| extension == ".utoc"));
    assert!(
        slot.root_extensions
            .iter()
            .any(|extension| extension == ".pak")
    );
    assert!(pak.deploy.mirror.iter().any(|mirror| {
        mirror.dir == "Content/Paks/LogicMods"
            && mirror
                .extensions
                .iter()
                .any(|extension| extension == ".pak")
    }));
    assert!(pak.deploy.aliases.contains_key("~mods"));
    assert!(pak.deploy.aliases.contains_key("LogicMods"));
    assert!(
        !pak.deploy
            .slots
            .iter()
            .any(|slot| slot.dir.contains("ue4ss"))
    );

    let four = loader.rules(Some("ue4ss-4")).unwrap();
    let slot = four
        .deploy
        .slots
        .iter()
        .find(|slot| slot.dir == "Binaries/Win64/ue4ss/Mods")
        .expect("the UE4SS 4 slot");
    assert_eq!(slot.markers, ["Scripts", "dlls"]);
    assert_eq!(slot.enable_file.as_deref(), Some("enabled.txt"));

    let three = loader.rules(Some("ue4ss-3")).unwrap();
    let slot = three
        .deploy
        .slots
        .iter()
        .find(|slot| slot.dir == "Binaries/Win64/Mods")
        .expect("the UE4SS 3 slot");
    assert_eq!(slot.markers, ["Scripts", "dlls"]);
    assert_eq!(slot.enable_file.as_deref(), Some("enabled.txt"));
}

#[test]
fn a_game_adds_its_file_to_the_loader_package() {
    let text = SLOT.replacen(
        "[deploy]",
        "[install]\ncheck = [\"ue4ss/layout.ini\"]\n\n[deploy]",
        1,
    );
    let mut game = GameDef::parse(&text).unwrap();
    let dir = Path::new(super::super::DEV_LOADERS_DIR);
    let loader = presets::load(dir, "unreal").unwrap();
    loader.rules(Some("ue4ss-4")).unwrap().apply_to(&mut game);
    let install = game.install.expect("the merged package");
    assert_eq!(install.name, "UE4SS");
    assert_eq!(install.version, "4.0.0");
    assert!(install.url.contains("UE4SS"));
    assert!(install.check.iter().any(|path| path == "ue4ss/layout.ini"));
}

#[test]
fn the_bepinex_preset_has_the_five_and_six_rules() {
    let loader = presets::load(Path::new(super::super::DEV_LOADERS_DIR), "bepinex").unwrap();
    let five = loader.rules(None).unwrap();
    assert!(five.requires.iter().any(|path| path == "winhttp.dll"));
    assert!(
        five.requires
            .iter()
            .any(|path| path == "BepInEx/core/BepInEx.dll")
    );
    assert!(
        five.deploy
            .slots
            .iter()
            .any(|slot| slot.dir == "BepInEx/plugins"
                && slot.root_extensions == [".dll"]
                && slot.root_folder_extensions == [".dll"])
    );
    assert_eq!(
        five.deploy.aliases.get("plugins").map(String::as_str),
        Some("BepInEx/plugins")
    );
    assert_eq!(
        five.deploy.aliases.get("patchers").map(String::as_str),
        Some("BepInEx/patchers")
    );
    assert_eq!(
        five.deploy.aliases.get("monomod").map(String::as_str),
        Some("BepInEx/monomod")
    );
    assert!(
        five.deploy
            .mirror
            .iter()
            .all(|mirror| mirror.dir != "dotnet")
    );

    let six = loader.rules(Some("6")).unwrap();
    assert!(six.requires.iter().any(|path| path == "winhttp.dll"));
    assert!(
        six.requires
            .iter()
            .all(|path| path != "BepInEx/core/BepInEx.dll")
    );
    assert!(
        six.deploy
            .mirror
            .iter()
            .any(|mirror| mirror.dir == "dotnet"),
        "{:?}",
        six.deploy.mirror
    );

    for dir in [
        "BepInEx/core",
        "BepInEx/config",
        "BepInEx/patchers",
        "BepInEx/monomod",
    ] {
        assert!(
            six.deploy.mirror.iter().any(|mirror| mirror.dir == dir),
            "{dir} is missing from {:?}",
            six.deploy.mirror
        );
    }
}

#[test]
fn requirement_metadata_merges_by_path_and_the_game_wins() {
    let loader = r#"
schema = 1
id = "requirement-loader"
name = "Requirement Loader"
requires = ["proxy.dll", "core/b.dll"]

[[requirements]]
name = "Proxy"
path = "proxy.dll"
url = "https://example.invalid/proxy"

[[variants."b".requirements]]
name = "Core from the loader"
path = "core/b.dll"
"#;
    let game = r#"
schema = 1
id = "requirement-game"
name = "Requirement Game"
loader = { id = "requirement-loader", variant = "b" }
requires = ["proxy.dll", "core/b.dll"]

[detect]
markers = ["game.exe"]

[[executables]]
name = "Game"
path = "game.exe"

[deploy]
[[deploy.slots]]
dir = "Mods"

[[requirements]]
name = "Core from the game"
path = "core/b.dll"
url = "https://example.invalid/core"
"#;
    let mut game = GameDef::parse(game).unwrap();
    presets::LoaderDef::parse(loader)
        .unwrap()
        .rules(Some("b"))
        .unwrap()
        .apply_to(&mut game);
    assert!(game.validate().is_empty(), "{:?}", game.validate());
    assert_eq!(game.requirements.len(), 2);
    let proxy = game
        .requirements
        .iter()
        .find(|requirement| requirement.path == "proxy.dll")
        .expect("the loader requirement");
    assert_eq!(proxy.name, "Proxy");
    assert_eq!(proxy.url.as_deref(), Some("https://example.invalid/proxy"));
    let core = game
        .requirements
        .iter()
        .find(|requirement| requirement.path == "core/b.dll")
        .expect("the game requirement");
    assert_eq!(core.name, "Core from the game");
    assert_eq!(core.url.as_deref(), Some("https://example.invalid/core"));
}

/// A preset that names its files only in `[[requirements]]`, with one variant that adds a second file the same way.
const IMPLIED_LOADER: &str = r#"
schema = 1
id = "implied-loader"
name = "Implied Loader"
default_variant = "a"

[[requirements]]
name = "Proxy"
path = "proxy.dll"

[variants."a"]
[[variants."a".requirements]]
name = "Core from the variant"
path = "Core/a.dll"
"#;

/// The minimal game with `loader` set to `loader_line` at the top level, so the rest of the text stays as it is.
fn minimal_game_with_loader(loader_line: &str) -> String {
    MINIMAL_GAME.replacen(
        "id = \"loader-test\"\n",
        &format!("id = \"loader-test\"\n{loader_line}\n"),
        1,
    )
}

#[test]
fn a_requirement_is_required_without_a_requires_entry() {
    let rules = presets::LoaderDef::parse(IMPLIED_LOADER)
        .unwrap()
        .rules(None)
        .unwrap();
    assert_eq!(rules.requires, ["proxy.dll", "Core/a.dll"]);

    let dir = scratch!("implied-requirements");
    write(&dir.join("implied-loader.toml"), IMPLIED_LOADER);
    let mut game =
        GameDef::parse(&minimal_game_with_loader("loader = \"implied-loader\"")).unwrap();
    presets::apply(&mut game, std::slice::from_ref(&dir)).unwrap();
    assert!(game.validate().is_empty(), "{:?}", game.validate());
    assert_eq!(game.requires, ["proxy.dll", "Core/a.dll"]);
}

#[test]
fn a_requirement_path_already_in_requires_is_not_listed_twice() {
    let dir = scratch!("implied-duplicate");
    write(&dir.join("implied-loader.toml"), IMPLIED_LOADER);
    let mut game = GameDef::parse(&minimal_game_with_loader(
        "loader = \"implied-loader\"\nrequires = [\"PROXY.dll\", \"Core/a.dll\"]",
    ))
    .unwrap();
    presets::apply(&mut game, std::slice::from_ref(&dir)).unwrap();
    assert_eq!(game.requires, ["proxy.dll", "Core/a.dll"]);
    let proxy = game
        .requires
        .iter()
        .filter(|path| path.eq_ignore_ascii_case("proxy.dll"))
        .count();
    assert_eq!(proxy, 1, "{:?}", game.requires);
}

/// A game that names a loader which each folder supplies under the same id, with a different name per folder.
const PRECEDENCE_GAME: &str = r#"
schema = 1
id = "precedence-game"
name = "Precedence Game"
loader = "fixture-loader"

[detect]
markers = ["game.exe"]

[[executables]]
name = "Game"
path = "game.exe"
"#;

fn loader_named(name: &str) -> String {
    format!(
        "schema = 1\nid = \"fixture-loader\"\nname = \"{name}\"\n\n[deploy]\n[[deploy.slots]]\ndir = \"Mods\"\n"
    )
}

#[test]
fn a_loader_prefers_user_then_official_then_bundled() {
    let base = scratch!("loader-precedence");
    let user = base.join("user");
    let catalog = base.join("catalog");
    let bundled = base.join("bundled");
    write(
        &user.join("fixture-loader.toml"),
        &loader_named("User Loader"),
    );
    write(
        &catalog.join("fixture-loader.toml"),
        &loader_named("Official Loader"),
    );
    write(
        &bundled.join("fixture-loader.toml"),
        &loader_named("Bundled Loader"),
    );
    let game_path = base.join("game.toml");
    write(&game_path, PRECEDENCE_GAME);

    let resolved = |dirs: Vec<std::path::PathBuf>| {
        read_definition_with(&game_path, &dirs)
            .expect("the game loads with any of the presets")
            .resolved_loader
            .expect("the game names a loader")
            .name
    };
    assert_eq!(
        resolved(loader_dirs(Some(&user), Some(&catalog), &bundled)),
        "User Loader"
    );
    assert_eq!(
        resolved(loader_dirs(None, Some(&catalog), &bundled)),
        "Official Loader"
    );
    assert_eq!(
        resolved(loader_dirs(None, None, &bundled)),
        "Bundled Loader"
    );
}

#[test]
fn a_game_requirement_is_required_on_top_of_a_loader_and_without_one() {
    let dir = scratch!("implied-game-requirement");
    write(&dir.join("implied-loader.toml"), IMPLIED_LOADER);

    // The game adds one file and repeats the loader's proxy with another name: the entry is replaced, not doubled.
    let text = format!(
        "{}\n[[requirements]]\nname = \"Extra\"\npath = \"Extra/extra.dll\"\n\n[[requirements]]\nname = \"Proxy from the game\"\npath = \"PROXY.dll\"\n",
        minimal_game_with_loader("loader = \"implied-loader\""),
    );
    let mut game = GameDef::parse(&text).unwrap();
    presets::apply(&mut game, std::slice::from_ref(&dir)).unwrap();
    assert!(game.validate().is_empty(), "{:?}", game.validate());
    assert_eq!(
        game.requires,
        ["proxy.dll", "Core/a.dll", "Extra/extra.dll"]
    );
    assert_eq!(game.requirements.len(), 3);

    // Without any loader, a game's own requirement is still required.
    let text =
        format!("{MINIMAL_GAME}\n[[requirements]]\nname = \"Extra\"\npath = \"Extra/extra.dll\"\n");
    let mut plain = GameDef::parse(&text).unwrap();
    presets::apply(&mut plain, &[]).unwrap();
    assert_eq!(plain.requires, ["Extra/extra.dll"]);
}

fn unreal_ue4ss_game() -> GameDef {
    crate::test_support::games::unreal()
}

#[test]
fn the_loader_is_needed_only_by_mods_in_its_folders() {
    use crate::domain::resolver::DeployPlan;

    let game = unreal_ue4ss_game();
    let root = scratch!("loader-needed-by");
    let plan = |slots: &[&str], files: &[&str]| {
        let plan = DeployPlan {
            slots_used: slots.iter().map(|slot| (*slot).to_string()).collect(),
            file_count: u32::try_from(files.len()).unwrap(),
            ..DeployPlan::default()
        };
        let placed: std::collections::BTreeSet<String> = files
            .iter()
            .map(|path| crate::domain::pathkey::path_key(path))
            .collect();
        (plan, placed)
    };
    let missing = |slots: &[&str], files: &[&str]| {
        let (plan, placed) = plan(slots, files);
        game.missing_loader(&root, &plan, &placed).is_some()
    };

    assert!(!missing(&[], &[]));
    assert!(!missing(&[], &["Content/Paks/~mods/Better.pak"]));
    assert!(missing(&["Binaries/Win64/ue4ss/Mods"], &[]));
    assert!(missing(&[], &["Content/Paks/LogicMods/Better.pak"]));

    for file in [
        "Binaries/Win64/dwmapi.dll",
        "Binaries/Win64/ue4ss/UE4SS.dll",
    ] {
        write(&root.join(file), "loader");
    }
    assert!(!missing(&["Binaries/Win64/ue4ss/Mods"], &[]));
}

#[test]
fn the_loader_release_is_recognised_by_its_file_names_wherever_it_sits() {
    let game = unreal_ue4ss_game();
    assert!(game.is_loader_archive(["dwmapi.dll", "ue4ss/UE4SS.dll", "ue4ss/Mods/mods.txt"]));
    assert!(game.is_loader_archive([
        "Pal/Binaries/Win64/DWMAPI.dll",
        "Pal/Binaries/Win64/ue4ss/UE4SS.dll"
    ]));
    assert!(!game.is_loader_archive(["Better/Scripts/main.lua"]));
    assert!(!game.is_loader_archive(["Better.pak", "dwmapi.dll"]));
}

#[test]
fn a_game_names_its_own_build_of_the_loader_and_keeps_its_checks() {
    let text = SLOT.replacen(
        "[deploy]",
        "[install]\nversion = \"game-build\"\nurl = \"https://example.com/game-build.zip\"\nsha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n\n[deploy]",
        1,
    );
    let mut game = GameDef::parse(&text).unwrap();
    let loader = presets::load(Path::new(super::super::DEV_LOADERS_DIR), "unreal").unwrap();
    loader.rules(Some("ue4ss-4")).unwrap().apply_to(&mut game);
    let install = game.install.expect("the merged package");
    assert_eq!(install.version, "game-build");
    assert_eq!(install.url, "https://example.com/game-build.zip");
    assert_eq!(install.size, None);
    assert_eq!(install.to, "Binaries/Win64");
    assert!(install.check.iter().any(|path| path.ends_with("UE4SS.dll")));
    assert!(!install.needed_by.is_empty());
}

#[test]
fn a_release_without_its_configuration_file_is_still_the_loader() {
    let mut game = unreal_ue4ss_game();
    game.install
        .as_mut()
        .unwrap()
        .check
        .push("Binaries/Win64/ue4ss/Layout.ini".into());
    assert!(game.is_loader_archive(["dwmapi.dll", "ue4ss/UE4SS.dll"]));
}
