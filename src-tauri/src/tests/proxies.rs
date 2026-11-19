//! Proton launches: the Proton options and the variables a game starts with, and which loader DLLs load native. A loader
//! counts only when the launched view of the game has it next to the program.

use super::*;
use crate::games::proxy::{native_dlls, program_dir};
use crate::launch::start::game_env;

#[test]
fn the_settings_variable_takes_the_value_over_a_proton_option() {
    let settings = Settings {
        proton_options: vec!["wined3d".to_string()],
        launch_env: "PROTON_USE_WINED3D=0\nDXVK_HUD=fps".to_string(),
        ..Settings::default()
    };
    let env = game_env(&settings, true, &[], None).unwrap();
    assert_eq!(
        env,
        vec![
            ("PROTON_USE_WINED3D".to_string(), "0".to_string()),
            ("DXVK_HUD".to_string(), "fps".to_string()),
        ]
    );
}

#[test]
fn proton_options_only_apply_under_proton() {
    let settings = Settings {
        proton_options: vec!["wined3d".to_string()],
        ..Settings::default()
    };
    assert!(game_env(&settings, false, &[], None).unwrap().is_empty());
}

#[test]
fn the_override_keeps_the_names_the_process_inherits() {
    let native = ["winhttp.dll".to_string()];
    let env = game_env(&Settings::default(), true, &native, Some("version=n,b")).unwrap();
    assert_eq!(
        env,
        vec![(
            "WINEDLLOVERRIDES".to_string(),
            "version=n,b;winhttp.dll=n,b".to_string()
        )]
    );
}

#[test]
fn the_settings_keep_their_own_dll_override() {
    let settings = Settings {
        launch_env: "WINEDLLOVERRIDES=winhttp.dll=d".to_string(),
        ..Settings::default()
    };
    let native = ["winhttp.dll".to_string()];
    let env = game_env(&settings, true, &native, None).unwrap();
    assert_eq!(
        env,
        vec![("WINEDLLOVERRIDES".to_string(), "winhttp.dll=d".to_string())]
    );
}

#[test]
fn every_shipped_proxy_of_a_game_is_native_when_it_is_next_to_the_program() {
    test_games::install();
    let shipped =
        crate::games::load_dir(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../games")))
            .unwrap();
    let mut checked = 0;
    for game in &shipped {
        let proxies = game.deploy.proxy_dlls();
        if proxies.is_empty() {
            continue;
        }
        checked += 1;
        let folder = game
            .executables
            .first()
            .map_or("", |exe| program_dir(&exe.path));
        let native = native_dlls(game, folder, |_| true);
        for dll in &proxies {
            assert!(
                native.iter().any(|name| name.eq_ignore_ascii_case(dll)),
                "{}: {dll} is not loaded native",
                game.id
            );
        }
    }
    assert!(checked > 0, "a shipped game has a proxy DLL");
}
