//! The rules a definition must keep: a place for mods, one kind per folder, paths inside the game, safe templates.

use super::*;

#[test]
fn a_game_needs_a_place_for_its_mods() {
    let mut game = GameDef::parse(PLUGIN).unwrap();
    game.deploy.slots.clear();
    game.deploy.mirror.clear();
    assert!(
        game.validate()
            .iter()
            .any(|e| e.contains("at least one slot or mirror"))
    );
}

#[test]
fn a_full_root_game_needs_no_slots_or_mirrors_but_cannot_use_slots() {
    let mut game = GameDef::parse(SLOT).unwrap();
    game.deploy.slots.clear();
    game.deploy.mirror.clear();
    game.deploy.full_root = true;
    assert!(game.validate().is_empty(), "{:?}", game.validate());

    has_problem(
        &lf(SLOT).replacen("[deploy]\n", "[deploy]\nfull_root = true\n", 1),
        "cannot use slots",
    );
}

#[test]
fn a_move_rule_names_a_relative_folder_and_lowercase_extensions() {
    let with_move = |body: &str| {
        lf(SLOT).replacen(
            "[[deploy.slots]]",
            &format!("{body}\n\n[[deploy.slots]]"),
            1,
        )
    };
    let valid =
        with_move("[[deploy.move]]\nextensions = [\".archive\", \".xl\"]\nto = \"archive/pc/mod\"");
    let game = GameDef::parse(&valid).unwrap();
    assert!(game.validate().is_empty(), "{:?}", game.validate());

    has_problem(
        &with_move("[[deploy.move]]\nextensions = [\".archive\"]\nto = \"archive/pc/mod/\""),
        "without a trailing slash",
    );
    has_problem(
        &with_move("[[deploy.move]]\nextensions = [\".ARCHIVE\"]\nto = \"archive/pc/mod\""),
        "lowercase",
    );
    has_problem(
        &with_move("[[deploy.move]]\nextensions = [\".archive\"]\nto = \"../mod\""),
        "not a relative path",
    );
}

#[test]
fn an_alias_is_one_folder_name_that_points_inside_a_slot_or_mirror() {
    assert!(problems(&lf(ALIASED)).is_empty());
    has_problem(
        &lf(ALIASED).replacen(
            "plugins = \"BepInEx/plugins\"",
            "\"plugins/x\" = \"BepInEx/plugins\"",
            1,
        ),
        "must be one folder name",
    );
    has_problem(
        &lf(ALIASED).replacen(
            "plugins = \"BepInEx/plugins\"",
            "BepInEx = \"BepInEx/plugins\"",
            1,
        ),
        "already a game folder",
    );
    has_problem(
        &lf(ALIASED).replacen("core = \"BepInEx/core\"", "core = \"Other/core\"", 1),
        "not inside a slot or mirror",
    );
    has_problem(
        &lf(ALIASED).replacen("core = \"BepInEx/core\"", "core = \"BepInEx\\\\core\"", 1),
        "must use '/' in its target",
    );
    has_problem(
        &lf(ALIASED).replacen(
            "config = \"BepInEx/config\"",
            "config = \"BepInEx/config\"\nPlugins = \"BepInEx/plugins\"",
            1,
        ),
        "is repeated (case is ignored)",
    );
}

#[test]
fn a_folder_is_a_slot_or_a_mirror_once() {
    has_problem(
        &lf(SLOT).replacen("dir = \"DLC\"", "dir = \"mods\"", 1),
        "twice",
    );
}

#[test]
fn paths_must_stay_inside_the_game() {
    has_problem(
        &lf(MIRROR).replacen("\"archive/pc/mod\"", "\"../archive\"", 1),
        "mirror is not a relative path",
    );
    has_problem(
        &lf(SLOT).replacen("dir = \"content\"", "dir = \"../content\"", 1),
        "shared is not a relative path",
    );
    has_problem(
        &lf(PLUGIN).replacen("\"BepInEx/core/BepInEx.dll\"]", "\"/BepInEx.dll\"]", 1),
        "required file",
    );
    has_problem(
        &lf(SLOT).replacen(
            "\"bin/game.exe\"\n\n[deploy]",
            "\"C:/game.exe\"\n\n[deploy]",
            1,
        ),
        "executable",
    );
}

#[test]
fn extensions_are_lowercase_with_a_dot() {
    has_problem(
        &lf(SLOT).replacen("\".ws\", \".xml\"", "\"ws\"", 1),
        "start with a dot",
    );
    has_problem(
        &lf(PLUGIN).replacen("[\".dll\"]", "[\".DLL\"]", 1),
        "lowercase",
    );
}

#[test]
fn a_flat_slot_has_no_marker() {
    has_problem(
        &lf(EXTERNAL).replacen("\nmarker = ", "\nflat = true\nmarker = ", 1),
        "flat slot",
    );
}

#[test]
fn root_files_are_bare_names_listed_once_and_not_game_folders() {
    let with_root_files = |line: &str| {
        lf(PLUGIN).replacen(
            "root_files = [\"winhttp.dll\"]",
            &format!("root_files = [{line}]"),
            1,
        )
    };
    let valid =
        GameDef::parse(&with_root_files("\"winhttp.dll\", \"doorstop_config.ini\"")).unwrap();
    assert!(valid.validate().is_empty());
    has_problem(
        &with_root_files("\"BepInEx/winhttp.dll\""),
        "must be a file name",
    );
    has_problem(&with_root_files("\"/winhttp.dll\""), "must be a file name");
    has_problem(
        &with_root_files("\"winhttp.dll\", \"WinHTTP.dll\""),
        "listed twice",
    );
    has_problem(&with_root_files("\"BepInEx\""), "name of a game folder");
}

#[test]
fn a_root_file_is_a_bare_name_that_compares_like_a_path() {
    let game = GameDef::parse(PLUGIN).unwrap();
    assert_eq!(game.deploy.root_file_of("WinHTTP.dll"), Some("winhttp.dll"));
    assert_eq!(game.deploy.root_file_of("BepInEx/winhttp.dll"), None);
    assert_eq!(game.deploy.root_file_of("winhttp.dll.bak"), None);
    assert!(game.deploy.keeps_game_path("winhttp.dll"));
    assert!(game.deploy.keeps_game_path("BepInEx/core/BepInEx.dll"));
    assert!(!game.deploy.keeps_game_path("BepInEx/plugins/mod/x.dll"));
}

#[test]
fn a_profile_data_path_must_be_a_template() {
    let mut game = GameDef::parse(SLOT).unwrap();
    game.profile_data = vec!["saves".to_string()];
    assert!(game.validate().iter().any(|e| e.contains("profile_data")));
}

#[test]
#[allow(
    clippy::literal_string_with_formatting_args,
    reason = "path templates contain braces, not format arguments"
)]
fn a_user_data_path_inside_the_game_is_refused() {
    let mut game = GameDef::parse(SLOT).unwrap();
    game.user_data = vec![UserData {
        id: "documents".into(),
        kind: crate::domain::user_data::Kind::Saves,
        path: "{game}/saves".into(),
        path_linux: None,
        mode: crate::domain::user_data::Mode::Redirect,
        default: crate::domain::user_data::Isolation::Isolated,
        inherit: true,
    }];
    assert!(
        game.validate()
            .iter()
            .any(|problem| problem.contains("inside the game folder"))
    );
}

#[test]
fn a_user_data_linux_template_is_checked_too() {
    let mut game = GameDef::parse(SLOT).unwrap();
    game.user_data = vec![UserData {
        id: "documents".into(),
        kind: crate::domain::user_data::Kind::Saves,
        path: "{documents}/Slot Game".into(),
        path_linux: Some("saves".into()),
        mode: crate::domain::user_data::Mode::Redirect,
        default: crate::domain::user_data::Isolation::Isolated,
        inherit: true,
    }];
    let problems = game.validate();
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("path_linux")),
        "{problems:?}"
    );
}

#[test]
fn user_data_overlap_uses_each_platforms_effective_template() {
    let set = |id: &str, path: &str, linux: &str| UserData {
        id: id.into(),
        kind: crate::domain::user_data::Kind::Saves,
        path: path.into(),
        path_linux: Some(linux.into()),
        mode: crate::domain::user_data::Mode::Redirect,
        default: crate::domain::user_data::Isolation::Isolated,
        inherit: true,
    };
    let mut game = GameDef::parse(SLOT).unwrap();
    game.user_data = vec![
        set("saves", "{documents}/Slot Game", "{home}/Slot Game"),
        set(
            "config",
            "{documents}/Slot Game Config",
            "{home}/Slot Game/config",
        ),
    ];
    let problems = game.validate();
    assert!(
        problems.iter().any(|problem| problem.contains("overlap")),
        "{problems:?}"
    );
}

#[test]
fn the_order_file_is_a_safe_template_of_a_file() {
    has_problem(
        &lf(SLOT).replacen(
            "file = \"{documents}/Slot Game/mods.settings\"",
            "file = \"{documents}/../mods.settings\"",
            1,
        ),
        "load_order.file",
    );
    has_problem(
        &lf(SLOT).replacen("/mods.settings\"", "/\"", 1),
        "must name a file",
    );
}

#[test]
fn a_root_folder_is_a_lowercase_extension() {
    has_problem(
        &lf(PLUGIN).replacen(
            "root_folder_extensions = [\".dll\"]",
            "root_folder_extensions = [\".DLL\"]",
            1,
        ),
        "lowercase",
    );
}

#[test]
fn a_flat_slot_can_take_root_folders() {
    let text = lf(PLUGIN).replacen(
        "dir = \"BepInEx/plugins\"",
        "dir = \"BepInEx/plugins\"\nflat = true",
        1,
    );
    let game = GameDef::parse(&text).unwrap();
    assert!(game.validate().is_empty(), "{:?}", game.validate());
}

#[test]
fn a_game_without_an_executable_is_refused() {
    let mut game = GameDef::parse(SLOT).unwrap();
    game.executables.clear();
    assert!(
        game.validate()
            .iter()
            .any(|problem| problem.contains("at least one executable"))
    );
}

#[test]
fn a_deploy_root_must_start_with_a_path_variable() {
    let mut game = GameDef::parse(SLOT).unwrap();
    game.deploy.root = Some("Mods".into());
    assert!(
        game.validate()
            .iter()
            .any(|problem| problem.contains("must start with a path variable"))
    );
}

#[test]
fn overlapping_slots_or_mirrors_are_refused() {
    let mut game = GameDef::parse(SLOT).unwrap();
    let mut nested = game.deploy.slots[0].clone();
    nested.dir = format!("{}/nested", game.deploy.slots[0].dir);
    game.deploy.slots.push(nested);
    assert!(
        game.validate()
            .iter()
            .any(|problem| problem.contains("overlap"))
    );
}
