//! The setup checklist shows the loader package and its data, not only a missing file.

use super::*;
use crate::setup::LoaderSetupState;
use crate::test_support::scratch;

#[test]
fn the_setup_check_shows_the_loader_package() {
    let base = scratch!("setup-loader-view");
    let f = game_fixture("setup-loader-view-game", UNREAL_GAME, &["Project"]);
    let report = crate::status::preflight::preflight(&f.data, UNREAL_GAME).unwrap();
    let loader = report
        .loader
        .expect("the Unreal fixture has a loader package");
    assert_eq!(loader.name, "UE4SS");
    assert_eq!(loader.version, "4.0.0");
    assert_eq!(loader.state, LoaderSetupState::Missing);
    assert!(loader.source.contains("UE4SS"));
    assert_eq!(loader.sha256.len(), 64);
    assert_eq!(loader.target, "Binaries/Win64");
    let _ = fs::remove_dir_all(&base);
}

#[test]
fn plain_paks_do_not_ask_for_the_loader_but_a_lua_mod_does() {
    let f = game_fixture("setup-loader-needed", UNREAL_GAME, &["Project"]);
    let pak = f.zip("PakMod.zip", &[("Better.pak", "pak")]);
    import(&f.data, UNREAL_GAME, &pak, None, &mut |_, _| {}).unwrap();
    crate::tests::unity::enable_all(&f.data, UNREAL_GAME);

    let state = status(&f.data, UNREAL_GAME).unwrap();
    assert!(
        !state
            .problems
            .iter()
            .any(|problem| problem.code == crate::status::stage::LOADER_MISSING),
        "{:?}",
        state.problems
    );
    let report = crate::status::preflight::preflight(&f.data, UNREAL_GAME).unwrap();
    let loader = report
        .checks
        .iter()
        .find(|check| check.id == "loader")
        .unwrap();
    assert_eq!(loader.message.code, "preflight.loader.optional");

    let lua = f.zip("LuaMod.zip", &[("Better/Scripts/main.lua", "print('hi')")]);
    import(&f.data, UNREAL_GAME, &lua, None, &mut |_, _| {}).unwrap();
    crate::tests::unity::enable_all(&f.data, UNREAL_GAME);

    let state = status(&f.data, UNREAL_GAME).unwrap();
    let problem = state
        .problems
        .iter()
        .find(|problem| problem.code == crate::status::stage::LOADER_MISSING)
        .expect("a Lua mod needs the loader");
    assert_eq!(
        problem.params.get("name").map(String::as_str),
        Some("UE4SS")
    );
    assert_eq!(
        state.next_action,
        crate::status::stage::NextAction::InstallLoader
    );
}

#[test]
fn a_dropped_loader_repack_is_one_mod_at_its_game_paths() {
    let f = game_fixture("setup-loader-archive", UNREAL_GAME, &["Project"]);
    let repack = f.zip(
        "UE4SS-repack.zip",
        &[
            ("Pal/Binaries/Win64/dwmapi.dll", "proxy"),
            ("Pal/Binaries/Win64/ue4ss/UE4SS.dll", "loader"),
            ("Pal/Binaries/Win64/ue4ss/Mods/mods.txt", "list"),
            ("readme.txt", "notes"),
        ],
    );
    let view = import(&f.data, UNREAL_GAME, &repack, None, &mut |_, _| {}).unwrap();
    assert_eq!(view.folders.len(), 1);
    crate::tests::unity::enable_all(&f.data, UNREAL_GAME);

    let state = status(&f.data, UNREAL_GAME).unwrap();
    let report = crate::status::preflight::preflight(&f.data, UNREAL_GAME).unwrap();
    assert_eq!(report.loader.unwrap().state, LoaderSetupState::Installed);
    assert!(state.problems.is_empty(), "{:?}", state.problems);
}
