//! App settings: the window option, the games list and the data of removed games.

use super::*;
use crate::context::load_settings;
use crate::context::managed::{data_usage, managed_ids};
use crate::context::settings::{add_profile_data, remove_profile_data, set_proton_option};
use crate::migrations::{LEGACY_GAMES_FILE, removed_path};
use crate::settings::config::AppSettings;
use crate::settings::config::load_app_settings;
use crate::settings::config::save_app_settings;
use crate::settings::managed::{delete_removed_game, remove_game, restore_game};

#[test]
fn the_window_option_is_saved_and_an_old_settings_file_remembers_the_window() {
    let f = mirror_fixture("window-option");
    let off = AppSettings {
        remember_window: false,
        ..AppSettings::default()
    };
    save_app_settings(&f.data, off).unwrap();
    assert!(!load_app_settings(&f.data).remember_window);

    let old: AppSettings = serde_json::from_str(r#"{"language":"pl"}"#).unwrap();
    assert!(old.remember_window);
    assert!(AppSettings::default().remember_window);
}

#[test]
fn an_old_settings_file_with_a_deploy_method_still_loads_and_the_migration_removes_it() {
    let f = fixture("settings-old-method");
    let game = f.data.game(SLOT_GAME);
    fs::create_dir_all(&game.dir).unwrap();
    fs::write(
        game.config(),
        r#"{"game_root":"/game","settings_dir":"/settings","deploy_method":"hardlink"}"#,
    )
    .unwrap();

    let settings = load_settings(&game);
    assert_eq!(settings.game_root, "/game");
    assert_eq!(settings.settings_dir, "/settings");

    crate::migrations::run(&f.data.dir, &[SLOT_GAME.to_string()]);
    let text = fs::read_to_string(game.config()).unwrap();
    assert!(!text.contains("deploy_method"), "{text}");
    assert!(text.contains("game_root"), "{text}");
}

#[test]
fn a_blank_profile_data_path_is_dropped_when_the_settings_are_saved() {
    let f = mirror_fixture("profile-data-blank");
    let settings = Settings {
        profile_data: vec!["  ".into(), String::new(), "saves".into()],
        ..f.settings()
    };
    f.save_settings(MIRROR_GAME, settings).unwrap();

    assert_eq!(
        load_settings(&f.data.game(MIRROR_GAME)).profile_data,
        ["saves"]
    );
}

#[test]
fn the_game_folders_are_the_list_and_an_old_list_is_migrated() {
    let f = fixture("games-folder-list");
    save_folders(&f.data, MIRROR_GAME, &f.game_root, "");
    let old = f.data.dir.join(LEGACY_GAMES_FILE);
    fs::create_dir_all(&f.data.dir).unwrap();
    fs::write(&old, format!(r#"{{"added":["{SLOT_GAME}"]}}"#)).unwrap();

    assert_eq!(managed_ids(&f.data).unwrap(), [SLOT_GAME]);
    assert!(f.data.game(SLOT_GAME).config().exists());
    assert!(removed_path(&f.data.dir, MIRROR_GAME).is_dir());
    assert!(!old.exists());
    let _ = fs::remove_dir_all(&f.base);
}

#[test]
fn a_folder_copied_from_a_backup_joins_the_list_without_a_settings_file() {
    let f = fixture("games-folder-backup");
    fs::create_dir_all(f.data.game(MIRROR_GAME).dir).unwrap();
    fs::write(f.data.game(MIRROR_GAME).index(), "{}").unwrap();

    assert_eq!(managed_ids(&f.data).unwrap(), [MIRROR_GAME, SLOT_GAME]);
    let _ = fs::remove_dir_all(&f.base);
}

#[test]
fn a_removed_game_can_be_restored_and_its_data_deleted() {
    let f = fixture("games-removed-data");
    remove_game(&f.data, SLOT_GAME).unwrap();
    assert!(!f.data.game(SLOT_GAME).dir.exists());
    assert!(removed_path(&f.data.dir, SLOT_GAME).is_dir());

    let usage = data_usage(&f.data).unwrap();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].id, SLOT_GAME);
    assert!(usage[0].removed);
    assert!(usage[0].size_kb > 0);

    assert_eq!(restore_game(&f.data, SLOT_GAME).unwrap().len(), 1);
    assert!(f.data.game(SLOT_GAME).config().exists());
    assert!(!removed_path(&f.data.dir, SLOT_GAME).exists());
    let usage = data_usage(&f.data).unwrap();
    assert_eq!(usage.len(), 1);
    assert!(!usage[0].removed);

    remove_game(&f.data, SLOT_GAME).unwrap();
    delete_removed_game(&f.data, SLOT_GAME).unwrap();
    assert!(!removed_path(&f.data.dir, SLOT_GAME).exists());
    assert!(managed_ids(&f.data).unwrap().is_empty());
    let _ = fs::remove_dir_all(&f.base);
}

#[test]
fn deleting_removed_data_never_touches_the_active_folder() {
    let f = fixture("games-removed-active");
    let removed = removed_path(&f.data.dir, SLOT_GAME);
    fs::create_dir_all(&removed).unwrap();
    fs::write(removed.join("old.txt"), "old").unwrap();
    let usage = data_usage(&f.data).unwrap();
    assert_eq!(usage.len(), 2);
    assert_eq!(usage.iter().filter(|entry| entry.removed).count(), 1);

    delete_removed_game(&f.data, SLOT_GAME).unwrap();

    assert!(!removed.exists());
    assert!(f.data.game(SLOT_GAME).config().exists());
    let _ = fs::remove_dir_all(&f.base);
}

#[test]
fn two_quick_profile_data_edits_keep_both_paths_and_a_repeat_adds_nothing() {
    let f = fixture("profile-data-intents");
    add_profile_data(&f.data, SLOT_GAME, "profile-a").unwrap();
    add_profile_data(&f.data, SLOT_GAME, "  profile-b  ").unwrap();
    add_profile_data(&f.data, SLOT_GAME, "profile-a").unwrap();
    add_profile_data(&f.data, SLOT_GAME, "   ").unwrap();
    assert_eq!(
        load_settings(&f.data.game(SLOT_GAME)).profile_data,
        ["profile-a", "profile-b"]
    );

    remove_profile_data(&f.data, SLOT_GAME, "profile-a").unwrap();
    remove_profile_data(&f.data, SLOT_GAME, "not-listed").unwrap();
    assert_eq!(
        load_settings(&f.data.game(SLOT_GAME)).profile_data,
        ["profile-b"]
    );
}

#[cfg(not(windows))]
#[test]
fn two_quick_proton_switches_both_stay_on_and_a_switch_off_keeps_the_rest() {
    let f = fixture("proton-intents");
    set_proton_option(&f.data, SLOT_GAME, "shader_cache", true).unwrap();
    set_proton_option(&f.data, SLOT_GAME, "wined3d", true).unwrap();
    assert_eq!(
        load_settings(&f.data.game(SLOT_GAME)).proton_options,
        ["wined3d", "shader_cache"]
    );

    set_proton_option(&f.data, SLOT_GAME, "wined3d", false).unwrap();
    assert_eq!(
        load_settings(&f.data.game(SLOT_GAME)).proton_options,
        ["shader_cache"]
    );
}

#[cfg(not(windows))]
#[test]
fn switching_a_latency_layer_on_clears_the_other_one_and_an_unknown_id_is_not_saved() {
    let f = fixture("proton-latency");
    set_proton_option(&f.data, SLOT_GAME, "low_latency", true).unwrap();
    set_proton_option(&f.data, SLOT_GAME, "antilag", true).unwrap();
    assert_eq!(
        load_settings(&f.data.game(SLOT_GAME)).proton_options,
        ["antilag"]
    );

    set_proton_option(&f.data, SLOT_GAME, "retired", true).unwrap();
    assert_eq!(
        load_settings(&f.data.game(SLOT_GAME)).proton_options,
        ["antilag"]
    );
}

#[cfg(windows)]
#[test]
fn windows_saves_no_proton_option() {
    let f = fixture("proton-windows");
    set_proton_option(&f.data, SLOT_GAME, "wined3d", true).unwrap();
    assert!(
        load_settings(&f.data.game(SLOT_GAME))
            .proton_options
            .is_empty()
    );
}
