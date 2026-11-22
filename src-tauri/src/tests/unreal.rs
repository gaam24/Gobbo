//! The shipped `unreal` preset through the real import, profile and overlay flow. A synthetic project folder keeps the
//! test independent of a game: the paths below are the engine's, not a game's.
#![cfg(target_os = "linux")]

use super::*;

use crate::deploy::ops::apply_deployment;
use crate::deploy::overlay::Session;
use crate::launch::start::overlay_session_state;
use crate::test_support::require_overlay_namespaces;
use crate::tests::unity::enable_all;

#[test]
fn unreal_mods_are_served_under_the_project_folder_and_the_game_stays_clean() {
    require_overlay_namespaces!();

    let base = scratch!("unreal-overlay");
    let f = game_fixture("unreal-overlay-game", UNREAL_GAME, &["Project"]);
    // The loader is a mod like any other: its release keeps its game paths and the overlay serves it.
    let loader = f.zip(
        "UE4SS_release.zip",
        &[("dwmapi.dll", "proxy"), ("ue4ss/UE4SS.dll", "loader")],
    );
    import(&f.data, UNREAL_GAME, &loader, None, &mut |_, _| {}).unwrap();
    let pak = f.zip("PakMod.zip", &[("BetterBeast.pak", "pak")]);
    let lua = f.zip(
        "LuaMod.zip",
        &[("BetterBeast/Scripts/main.lua", "print('hello')")],
    );
    import(&f.data, UNREAL_GAME, &pak, None, &mut |_, _| {}).unwrap();
    import(&f.data, UNREAL_GAME, &lua, None, &mut |_, _| {}).unwrap();

    let state = status(&f.data, UNREAL_GAME).unwrap();
    assert_eq!(state.mods.len(), 3);
    enable_all(&f.data, UNREAL_GAME);
    apply_deployment(&f.data, UNREAL_GAME).unwrap();

    let state = overlay_session_state(&f.data.game(UNREAL_GAME))
        .unwrap()
        .expect("the overlay is deployed");
    let layers = Session::at(&state, &f.game_root).layers();
    let lua_folder = status(&f.data, UNREAL_GAME)
        .unwrap()
        .mods
        .iter()
        .find(|mod_view| mod_view.slot == "Binaries/Win64/ue4ss/Mods")
        .expect("the Lua mod")
        .folder
        .clone();

    for file in [
        "Project/Content/Paks/~mods/BetterBeast.pak".to_string(),
        "Project/Binaries/Win64/dwmapi.dll".to_string(),
        "Project/Binaries/Win64/ue4ss/UE4SS.dll".to_string(),
        format!("Project/Binaries/Win64/ue4ss/Mods/{lua_folder}/Scripts/main.lua"),
        format!("Project/Binaries/Win64/ue4ss/Mods/{lua_folder}/enabled.txt"),
    ] {
        assert!(
            layers.resolve(&file, Path::is_file).is_some(),
            "{file} is not where the game or UE4SS looks for it"
        );
    }

    assert!(
        !f.game_root
            .join("Project/Content/Paks/~mods/BetterBeast.pak")
            .exists()
    );
    // The real mount: from inside the view, a program started in the game folder finds the loader next to the game's
    // program and the pak where the engine reads it, at their game paths.
    let session = Session::with_roots(
        &state,
        &f.game_root.join("Project"),
        &f.game_root,
        Vec::new(),
    );
    let mut probe = crate::deploy::overlay::command(
        &session,
        "sh",
        &[
            "-c".to_string(),
            "test -f Project/Binaries/Win64/dwmapi.dll && test -f Project/Content/Paks/~mods/BetterBeast.pak"
                .to_string(),
        ],
    );
    assert!(
        probe.status().unwrap().success(),
        "the mounted view must show the mod files at their game paths"
    );

    assert!(
        !f.game_root.join("Project/Binaries").exists(),
        "the loader and its mods must not be written into the game folder"
    );
    let _ = fs::remove_dir_all(&base);
}
