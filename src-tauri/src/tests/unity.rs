//! `BepInEx` loaders, run through every shipped definition that has a `BepInEx` plugin slot. A game takes the same flow
//! without being named here, so a new game with a loader needs no change in this file: the test shows what its
//! definition gets wrong.

use super::*;
use crate::deploy::ops::apply_deployment;
#[cfg(target_os = "linux")]
use crate::deploy::overlay::Session;
use crate::games::GameDef;
#[cfg(target_os = "linux")]
use crate::launch::start::overlay_session_state;
#[cfg(target_os = "linux")]
use crate::launch::tools::run_game;
use crate::test_support::make_zip;
#[cfg(target_os = "linux")]
use crate::test_support::require_overlay_namespaces;
use crate::test_support::scratch;

/// The selected folder and the deployed overlay layers of the first mod.
#[cfg(target_os = "linux")]
fn deployed_layers(
    data: &Data,
    id: &str,
    game_root: &Path,
) -> (String, gobbo_overlay_core::Layers) {
    let folder = status(data, id).unwrap().mods[0].folder.clone();
    let state = overlay_session_state(&data.game(id))
        .unwrap()
        .expect("the overlay is deployed");
    let layers = Session::at(&state, game_root).layers();
    (folder, layers)
}

/// The ids of the shipped games with a `BepInEx` plugin slot: every game that takes a `BepInEx` loader is tested here.
fn bepinex_games() -> Vec<String> {
    let shipped =
        crate::games::load_dir(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../games")))
            .expect("the shipped definitions load");
    shipped
        .into_iter()
        .filter(|game| {
            game.deploy
                .slots
                .iter()
                .any(|slot| slot.dir == "BepInEx/plugins")
        })
        .map(|game| game.id)
        .collect()
}

/// A `BepInEx` archive as the mod sites ship it: the core and its proxy at the root, the proxy's settings, and a plugin.
pub(super) const BEPINEX_ARCHIVE: &[(&str, &str)] = &[
    ("BepInEx/core/BepInEx.dll", "core"),
    ("winhttp.dll", "proxy"),
    ("doorstop_config.ini", "[UnityDoorstop]"),
    ("BepInEx/plugins/FastShop/FastShop.dll", "plugin"),
];

/// The game folder of a Steam install: `steamapps/common/<folder>` with the game's marker files, and the prefix Proton
/// creates on the first run (`compatdata/<app id>/pfx`).
pub(super) fn steam_game(base: &Path, game: &GameDef) -> PathBuf {
    let library = base.join("library");
    let folder = game
        .detect
        .steam_install_dirs
        .first()
        .expect("a Steam game names its install folder");
    let game_root = library.join("steamapps/common").join(folder);

    for marker in &game.detect.markers {
        let path = game_root.join(marker);
        fs::create_dir_all(path.parent().expect("a marker has a folder")).unwrap();
        fs::write(path, b"").unwrap();
    }
    // The fixture is the Windows build under Proton, as Steam installs it for these tests; a native build is a different
    // folder with the Linux program instead.

    let app_id = game.detect.steam_app_ids[0];
    fs::create_dir_all(library.join(format!("steamapps/compatdata/{app_id}/pfx"))).unwrap();
    game_root
}

/// The settings of a game: its folder and its launch variables, every other field empty.
pub(super) fn settings_of(game_root: &Path, launch_env: &str) -> Settings {
    Settings {
        game_root: game_root.display().to_string(),
        launch_env: launch_env.to_string(),
        ..Settings::default()
    }
}

/// Enables every mod of the game in the active profile.
pub(super) fn enable_all(data: &Data, id: &str) {
    let rows: Vec<Row> = status(data, id)
        .unwrap()
        .mods
        .iter()
        .map(|mod_view| row(&mod_view.folder, true))
        .collect();
    save_profile(data, id, &rows).unwrap();
}

/// The codes of the problems the status reports for the game.
fn problem_codes(data: &Data, id: &str) -> Vec<String> {
    status(data, id)
        .unwrap()
        .problems
        .into_iter()
        .map(|problem| problem.code)
        .collect()
}

/// A game installed in a Steam library, with the archive imported and enabled. Returns the data and the game folder.
#[cfg(target_os = "linux")]
pub(super) fn installed_game(
    base: &Path,
    id: &str,
    archive: &[(&str, &str)],
    launch_env: &str,
) -> (Data, PathBuf) {
    let game = test_games::load(id);
    let game_root = steam_game(base, &game);
    let data = Data::new(base.join("data"));
    save_settings(&data, id, settings_of(&game_root, launch_env)).unwrap();
    let zip = make_zip(base, "mod.zip", archive);
    import(&data, id, &zip, None, &mut |_, _| {}).unwrap();
    enable_all(&data, id);
    (data, game_root)
}

#[test]
fn a_loader_is_required_until_an_enabled_mod_provides_it() {
    for id in &bepinex_games() {
        let base = scratch!(&format!("unity-required-{id}"));
        let game = test_games::load(id);
        let game_root = steam_game(&base, &game);
        let data = Data::new(base.join("data"));
        save_settings(&data, id, settings_of(&game_root, "")).unwrap();

        assert!(
            problem_codes(&data, id).contains(&"status.requirement_missing".to_string()),
            "{id}: the loader is missing, so the status must say so"
        );
        let err = apply_deployment(&data, id).unwrap_err();
        assert_eq!(err.code, "deploy.requirement_missing", "{id}");

        let zip = make_zip(&base, "mod.zip", BEPINEX_ARCHIVE);
        import(&data, id, &zip, None, &mut |_, _| {}).unwrap();
        enable_all(&data, id);
        assert!(
            !problem_codes(&data, id).contains(&"status.requirement_missing".to_string()),
            "{id}: the archive provides the loader, so the requirement is met"
        );
        let _ = fs::remove_dir_all(&base);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_bepinex_archive_is_served_where_the_game_loads_it_and_the_game_folder_stays_clean() {
    for id in &bepinex_games() {
        let base = scratch!("unity-overlay");
        let (data, game_root) = installed_game(&base, id, BEPINEX_ARCHIVE, "");
        apply_deployment(&data, id).unwrap();

        // A plugin folder of a slot belongs to its mod: the mod's folder in the library names it in the game.
        let folder = status(&data, id).unwrap().mods[0].folder.clone();
        let plugin = format!("BepInEx/plugins/{folder}/FastShop.dll");
        let state = overlay_session_state(&data.game(id))
            .unwrap()
            .expect("the overlay is deployed");
        let layers = Session::at(&state, &game_root).layers();

        for file in [
            "winhttp.dll",
            "doorstop_config.ini",
            "BepInEx/core/BepInEx.dll",
            plugin.as_str(),
        ] {
            assert!(
                layers.resolve(file, Path::is_file).is_some(),
                "{id}: {file} is not where the game looks for it"
            );
        }

        assert!(!game_root.join("winhttp.dll").exists(), "{id}");
        assert!(!game_root.join("BepInEx").exists(), "{id}");
        let _ = fs::remove_dir_all(&base);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_bare_plugin_at_the_archive_root_goes_to_the_plugins_folder() {
    for id in &bepinex_games() {
        let base = scratch!("unity-bare");
        let loose: &[(&str, &str)] = &[
            ("BepInEx/core/BepInEx.dll", "core"),
            ("winhttp.dll", "proxy"),
            ("BarePlugin.dll", "plugin"),
        ];
        let (data, game_root) = installed_game(&base, id, loose, "");
        apply_deployment(&data, id).unwrap();

        // A bare .dll at the archive root is a plugin of the archive's mod, in the plugins folder of the mod.
        let (folder, layers) = deployed_layers(&data, id, &game_root);
        assert!(
            layers
                .resolve(
                    &format!("BepInEx/plugins/{folder}/BarePlugin.dll"),
                    Path::is_file
                )
                .is_some(),
            "{id}: a bare .dll must reach the plugins folder of its mod"
        );
        let _ = fs::remove_dir_all(&base);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn gobbo_run_gives_the_program_the_launch_settings_and_the_proxy_override() {
    require_overlay_namespaces!();

    for id in &bepinex_games() {
        let base = scratch!("unity-run");
        let (data, _) = installed_game(&base, id, BEPINEX_ARCHIVE, "PROTON_USE_WINED3D=1");
        apply_deployment(&data, id).unwrap();

        // Steam's launch option calls `gobbo run <game> -- %command%`: the program is Proton, which writes what it got.
        let out = base.join("env.txt");
        let proton = base.join("proton");
        write_script(
            &proton,
            &format!(
                "#!/bin/sh\n{{ echo \"$WINEDLLOVERRIDES\"; echo \"$PROTON_USE_WINED3D\"; echo \"$@\"; }} > '{}'\n",
                out.display()
            ),
        );
        let args = ["waitforexitandrun".to_string()];
        let code = run_game(&data, id, &proton.display().to_string(), &args).unwrap();
        assert_eq!(code, 0, "{id}");

        let written = fs::read_to_string(&out).unwrap();
        let lines: Vec<&str> = written.lines().collect();
        assert!(
            lines[0].split(';').any(|part| part == "winhttp.dll=n,b"),
            "{id}: the proxy is not loaded native: {written}"
        );
        assert_eq!(
            lines[1], "1",
            "{id}: the launch settings did not reach the program"
        );
        assert_eq!(lines[2], "waitforexitandrun", "{id}");
        let _ = fs::remove_dir_all(&base);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_plugins_folder_at_the_archive_root_is_served_from_the_bepinex_plugins_folder() {
    for id in &bepinex_games() {
        let base = scratch!("unity-alias");
        // A mod packed with `plugins/<Mod>/<Mod>.dll` at the archive root, the layout the BepInEx preset aliases.
        let aliased: &[(&str, &str)] = &[
            ("BepInEx/core/BepInEx.dll", "core"),
            ("winhttp.dll", "proxy"),
            ("doorstop_config.ini", "[UnityDoorstop]"),
            ("plugins/FastShop/FastShop.dll", "plugin"),
        ];
        let (data, game_root) = installed_game(&base, id, aliased, "");
        apply_deployment(&data, id).unwrap();

        let (folder, layers) = deployed_layers(&data, id, &game_root);
        assert!(
            layers
                .resolve(
                    &format!("BepInEx/plugins/{folder}/FastShop.dll"),
                    Path::is_file
                )
                .is_some(),
            "{id}: a `plugins/` folder at the archive root must reach BepInEx/plugins"
        );
        let _ = fs::remove_dir_all(&base);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_plugin_folder_at_the_archive_root_is_served_from_the_plugins_folder() {
    for id in &bepinex_games() {
        let base = scratch!("unity-folder");
        // A mod shipped as `<Plugin>/<Plugin>.dll` at the archive root, next to the loader it needs.
        let folder_plugin: &[(&str, &str)] = &[
            ("BepInEx/core/BepInEx.dll", "core"),
            ("winhttp.dll", "proxy"),
            ("doorstop_config.ini", "[UnityDoorstop]"),
            ("UnlockPriceOverride/UnlockPriceOverride.dll", "plugin"),
        ];
        let (data, game_root) = installed_game(&base, id, folder_plugin, "");
        apply_deployment(&data, id).unwrap();

        // The folder is the mod: its plugin reaches the plugins folder of the mod in the game.
        let (folder, layers) = deployed_layers(&data, id, &game_root);
        assert!(
            layers
                .resolve(
                    &format!("BepInEx/plugins/{folder}/UnlockPriceOverride.dll"),
                    Path::is_file
                )
                .is_some(),
            "{id}: a plugin folder must reach the plugins folder of its mod"
        );
        let _ = fs::remove_dir_all(&base);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn a_mod_config_is_served_in_the_bepinex_config_folder() {
    for id in &bepinex_games() {
        let base = scratch!("unity-config");
        let mut archive: Vec<(&str, &str)> = BEPINEX_ARCHIVE.to_vec();
        archive.push(("BepInEx/config/FastShop.cfg", "[Settings]"));
        let (data, game_root) = installed_game(&base, id, &archive, "");
        apply_deployment(&data, id).unwrap();

        // A config file of a mod keeps its path: BepInEx reads it from the config folder of the game.
        let state = overlay_session_state(&data.game(id))
            .unwrap()
            .expect("the overlay is deployed");
        let layers = Session::at(&state, &game_root).layers();
        assert!(
            layers
                .resolve("BepInEx/config/FastShop.cfg", Path::is_file)
                .is_some(),
            "{id}: a mod's config must reach BepInEx/config"
        );
        assert!(!game_root.join("BepInEx").exists(), "{id}");
        let _ = fs::remove_dir_all(&base);
    }
}
