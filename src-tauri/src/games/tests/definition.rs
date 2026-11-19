//! Reading a definition: every folder in `games/`, and the keys a file may use.

use super::*;
use crate::error::ErrorKind;
use crate::test_support::games::{SLOT, lf};
use crate::test_support::{scratch, write};

#[test]
fn a_game_id_never_becomes_a_path_outside_the_definitions() {
    assert!(!is_valid_id("../slot"));
    assert!(!is_valid_id("slot/other"));
    assert!(definition_path("../slot").as_os_str().is_empty());
    assert!(icon_file("../slot").is_none());
}

#[test]
fn every_game_folder_holds_a_valid_definition_with_its_own_id() {
    let dir = Path::new(DEV_GAMES_DIR);
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_dir() {
            continue;
        }
        let game = read_definition(&path.join("game.toml"), Path::new(DEV_LOADERS_DIR))
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            game.validate().is_empty(),
            "{}: {:?}",
            path.display(),
            game.validate()
        );
        let folder = path.file_name().unwrap().to_string_lossy();
        assert!(
            folder.starts_with('_') || game.id == folder,
            "{folder} holds {}",
            game.id
        );
    }
}

#[test]
fn load_dir_reads_every_game_folder_and_skips_the_template() {
    let base = scratch!("load-dir");
    for (folder, id) in [
        ("zeta", "zeta"),
        ("alpha", "alpha"),
        ("_template", "example"),
    ] {
        let text = SLOT.replace("id = \"slot\"", &format!("id = \"{id}\""));
        write(&base.join(folder).join("game.toml"), &text);
    }

    let games = load_dir(&base).expect("the fixture folder must load");
    let ids: Vec<&str> = games.iter().map(|game| game.id.as_str()).collect();
    assert_eq!(ids, ["alpha", "zeta"]);
}

#[test]
fn a_misspelled_key_is_an_error_and_not_a_silent_default() {
    let text = lf(SLOT).replacen("protected = ", "protectd = ", 1);
    let error = GameDef::parse(&text).unwrap_err().to_string();
    assert!(error.contains("unknown field"), "{error}");
}

#[test]
fn a_method_list_is_no_longer_a_definition_key() {
    let text = lf(SLOT).replacen("[deploy]\n", "[deploy]\nmethods = [\"overlay\"]\n", 1);
    let error = GameDef::parse(&text).unwrap_err().to_string();
    assert!(error.contains("unknown field"), "{error}");
}

#[test]
fn cyberpunk_2077_accepts_new_loader_files_and_framework_folders() {
    let path = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../games/cyberpunk-2077/game.toml"
    ));
    let game = read_definition(path, Path::new(DEV_LOADERS_DIR)).expect("the shipped definition");
    assert!(game.deploy.full_root);
    for file in [
        "bin/x64/global.ini",
        "bin/x64/version.dll",
        "bin/x64/winmm.dll",
        "red4ext/plugins/future/Future.dll",
        "futureframework/Future.dll",
    ] {
        assert!(
            game.deploy.keeps_game_path(file),
            "{file} must keep its game path"
        );
    }
    assert_eq!(
        game.deploy.move_destination("loose.archive").as_deref(),
        Some("archive/pc/mod/loose.archive")
    );
    assert!(
        game.deploy
            .move_destination("mods/redmod/loose.archive")
            .is_none()
    );
}

#[test]
fn one_bad_definition_does_not_hide_the_other_games() {
    let base = scratch!("load-dir-bad");
    let good = SLOT.replace("id = \"slot\"", "id = \"good\"");
    write(&base.join("good").join("game.toml"), &good);
    write(
        &base.join("bad").join("game.toml"),
        "schema = 1\nid = \"bad\"\n",
    );

    let games = load_dir(&base).expect("the folder itself is readable");
    assert_eq!(
        games
            .iter()
            .map(|game| game.id.as_str())
            .collect::<Vec<_>>(),
        ["good"]
    );
}

#[test]
fn a_user_definition_loads_beside_the_shipped_one_with_its_origin_and_verification() {
    let base = scratch!("definitions-user");
    let shipped = base.join("shipped");
    let user = base.join("user");
    write(&shipped.join("slot").join("game.toml"), SLOT);

    let text = SLOT.replace("id = \"slot\"", "id = \"usermod\"")
        + "\n[verified]\ndate = \"2026-11-12\"\nos = [\"linux\"]\ngame_version = \"1.0\"\n";
    write(&user.join("usermod").join("game.toml"), &text);

    let (games, problems) =
        definitions_from(&shipped, Some(&user), Path::new(DEV_LOADERS_DIR)).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    let (game, origin) = games
        .iter()
        .find(|(game, _)| game.id == "usermod")
        .expect("the user definition is in the catalog");
    assert_eq!(*origin, GameOrigin::User);
    let verified = game.verified.as_ref().expect("the verified table");
    assert_eq!(verified.date, "2026-11-12");
    assert_eq!(verified.os, ["linux"]);
}

#[test]
fn a_bad_or_duplicate_user_definition_is_reported_instead_of_hiding_the_catalog() {
    let base = scratch!("definitions-problems");
    let shipped = base.join("shipped");
    let user = base.join("user");
    write(&shipped.join("slot").join("game.toml"), SLOT);
    write(&user.join("slot").join("game.toml"), SLOT);
    write(
        &user.join("bad").join("game.toml"),
        "schema = 1\nid = \"bad\"\n",
    );

    let (games, problems) =
        definitions_from(&shipped, Some(&user), Path::new(DEV_LOADERS_DIR)).unwrap();
    assert_eq!(games.len(), 1, "the shipped game still loads");
    assert_eq!(problems.len(), 2);
    assert!(
        problems
            .iter()
            .any(|problem| problem.error.code == "games.duplicate_definition")
    );
    assert!(
        problems
            .iter()
            .any(|problem| problem.error.code == "games.invalid_definition")
    );
    assert!(
        problems
            .iter()
            .all(|problem| problem.error.kind == ErrorKind::UserAction),
        "a user can fix every problem with a user definition"
    );
}

#[test]
fn a_user_definition_folder_must_have_the_game_id() {
    let base = scratch!("definitions-folder-id");
    let shipped = base.join("shipped");
    let user = base.join("user");
    write(&shipped.join("slot").join("game.toml"), SLOT);
    let text = SLOT.replace("id = \"slot\"", "id = \"renamed\"");
    write(&user.join("folder").join("game.toml"), &text);

    let (games, problems) =
        definitions_from(&shipped, Some(&user), Path::new(DEV_LOADERS_DIR)).unwrap();
    assert_eq!(games.len(), 1, "the shipped game still loads");
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].error.code, "games.invalid_definition");
    assert_eq!(problems[0].error.kind, ErrorKind::UserAction);
    assert!(
        problems[0].error.params["detail"].contains("folder"),
        "{:?}",
        problems[0].error.params
    );
}
