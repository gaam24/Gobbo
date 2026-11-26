use super::resolve::seed_redirect;
use super::store::{capture_copy, restore_copy};
use super::*;
use crate::context::Data;
use crate::context::Settings;
use crate::domain::profiles::Profile;
use crate::domain::user_data::{Isolation, Kind, Mode, Set};
use crate::games::GameDef;
use crate::test_support::scratch;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> (Data, GameDef, Settings, PathBuf) {
    let base = scratch!(name);
    let game_root = base.join("game");
    let settings_dir = base.join("settings");
    fs::create_dir_all(&game_root).unwrap();
    fs::create_dir_all(&settings_dir).unwrap();
    let mut game = crate::test_support::games::slot();
    game.user_data = vec![crate::games::UserData {
        id: "documents".into(),
        kind: Kind::Saves,
        path: "{settings}/user".into(),
        path_linux: None,
        mode: Mode::Redirect,
        default: Isolation::Isolated,
        inherit: true,
    }];
    let settings = Settings {
        game_root: game_root.display().to_string(),
        settings_dir: settings_dir.display().to_string(),
        ..Settings::default()
    };
    (
        Data::new(base.join("data")),
        game,
        settings,
        settings_dir.join("user"),
    )
}

fn profile(isolated: bool) -> Profile {
    let mut profile = Profile {
        id: "p1".into(),
        name: "Default".into(),
        mods: Vec::new(),
        separators: Vec::new(),
        groups: Vec::new(),
        deployed_mods: Vec::new(),
        deployed_at: None,
        winners: BTreeMap::default(),
        deployed_winners: BTreeMap::default(),
        user_data: BTreeMap::default(),
        pack: None,
        collections: Vec::new(),
        waiting: Vec::new(),
    };

    if isolated {
        profile.set_user_data_isolation("documents", Isolation::Isolated);
    }

    profile
}

// The Windows hook performs a redirect; the other platforms use the copy fallback for an isolated set, so only
// Windows can resolve a set as `Mode::Redirect`.
#[cfg(windows)]
#[test]
fn a_resolved_set_uses_the_profile_upper_layer_for_a_redirect() {
    let (data, game, settings, _) = fixture("user-data-set");
    let profile = profile(true);
    let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
    assert_eq!(sets.len(), 1);
    assert!(sets[0].is_redirected());
    assert_eq!(sets[0].kind, Kind::Saves);
}

// An owned Proton prefix exists only on Linux: Windows runs a Windows install directly, with no prefix.
#[cfg(not(windows))]
#[test]
fn a_shared_set_uses_the_known_owned_prefix_before_the_first_launch() {
    let (data, mut game, settings, _) = fixture("user-data-owned-prefix");
    game.user_data[0].path = "{local_appdata}/Example".into();
    let settings = Settings {
        source: "manual".into(),
        ..settings
    };
    let sets = sets(
        &data,
        &game,
        &settings,
        &profile(false),
        &data.profile_state("p1"),
    )
    .unwrap();
    assert_eq!(sets.len(), 1);
    assert!(!sets[0].isolated);
    assert!(
        sets[0]
            .real
            .starts_with(data.game(&game.id).dir.join("compatdata/pfx"))
    );
}

/// The legacy copy list must see the same known prefix as the user-data sets, without creating it first.
#[cfg(not(windows))]
#[test]
fn the_legacy_copy_list_uses_the_known_owned_prefix_root() {
    let (data, mut game, settings, _) = fixture("user-data-owned-prefix-copy-list");
    game.user_data[0].path = "{local_appdata}/Example".into();
    let settings = Settings {
        source: "manual".into(),
        ..settings
    };
    let sets = sets(
        &data,
        &game,
        &settings,
        &profile(false),
        &data.profile_state("p1"),
    )
    .unwrap();
    let inside = |path: &Path| {
        crate::context::profile::data::inside_user_data(&data, &game, &settings, path)
    };

    assert!(inside(&sets[0].real.join("save.txt")));
    let sibling = sets[0].real.with_file_name("Sibling").join("save.txt");
    assert!(
        !inside(&sibling),
        "a sibling folder is not user data: {sibling:?}"
    );
}

#[test]
fn a_missing_choice_is_shared() {
    let (data, game, settings, _) = fixture("user-data-shared");
    let profile = profile(false);
    let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
    assert!(!sets[0].isolated);
}

#[cfg(not(windows))]
#[test]
fn a_linux_template_is_used_when_it_is_set() {
    let (data, mut game, settings, _) = fixture("user-data-linux");
    game.user_data[0].path_linux = Some("{settings}/linux-user".into());
    let profile = profile(false);
    let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
    assert_eq!(
        sets[0].real,
        PathBuf::from(&settings.settings_dir).join("linux-user")
    );
}

#[test]
fn a_copy_set_is_captured_and_restored() {
    let (data, game, settings, real) = fixture("user-data-copy");
    let profile = profile(true);
    let mut game = game;
    game.user_data[0].mode = Mode::Copy;
    let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
    fs::create_dir_all(&real).unwrap();
    fs::write(real.join("save.txt"), "one").unwrap();
    capture_copy(&sets).unwrap();
    fs::write(real.join("save.txt"), "two").unwrap();
    restore_copy(&sets, true).unwrap();
    assert_eq!(fs::read_to_string(real.join("save.txt")).unwrap(), "one");
}

#[cfg(windows)]
#[test]
fn a_non_inherited_redirect_is_seeded_once() {
    let (data, mut game, settings, real) = fixture("user-data-seed");
    game.user_data[0].inherit = false;
    let profile = profile(true);
    fs::create_dir_all(real.join("gamesaves")).unwrap();
    fs::write(real.join("gamesaves/a.sav"), "save").unwrap();
    let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
    prepare(&sets).unwrap();
    seed_redirect(&sets[0]).unwrap();
    let seeded = sets[0].upper.join("gamesaves/a.sav");
    assert_eq!(fs::read_to_string(&seeded).unwrap(), "save");

    // The marker keeps a file the user deleted in the game from coming back on the next launch.
    fs::remove_file(&seeded).unwrap();
    seed_redirect(&sets[0]).unwrap();
    assert!(!seeded.exists());
}

#[test]
fn the_seed_marker_is_written_whole_and_only_once() {
    let (data, mut game, settings, real) = fixture("user-data-seed-marker");
    game.user_data[0].inherit = false;
    let profile = profile(true);
    fs::create_dir_all(real.join("gamesaves")).unwrap();
    fs::write(real.join("gamesaves/a.sav"), "save").unwrap();
    let mut sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
    // A redirect is a Windows mode; the marker is written the same way on every platform, so the test sets the mode.
    sets[0].mode = Mode::Redirect;

    seed_redirect(&sets[0]).unwrap();
    let marker = seeded(&sets[0].upper);
    assert!(marker.is_file());
    assert_eq!(fs::read(&marker).unwrap(), b"");
    // A second seed is a no-op: the marker is the proof that the seed already ran.
    fs::remove_file(sets[0].upper.join("gamesaves/a.sav")).unwrap();
    seed_redirect(&sets[0]).unwrap();
    assert!(!sets[0].upper.join("gamesaves/a.sav").exists());
}

#[cfg(windows)]
#[test]
fn the_order_file_of_a_redirect_is_written_to_the_profile_upper() {
    let base = scratch!("user-data-order");
    let root = base.join("game");
    let settings_dir = base.join("settings");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&settings_dir).unwrap();
    let text = r#"
schema = 1
id = "test"
name = "Test"
[detect]
markers = ["game.exe"]
[deploy]
[[deploy.slots]]
dir = "Mods"
[load_order]
format = "mods_settings"
file = "{settings}/mods.settings"
[[user_data]]
id = "documents"
path = "{settings}"
mode = "redirect"
default = "isolated"
"#;
    let game = GameDef::parse(text).unwrap();
    let profile = profile(true);
    let settings = Settings {
        game_root: root.display().to_string(),
        settings_dir: settings_dir.display().to_string(),
        ..Settings::default()
    };
    let data = Data::new(base.join("data"));
    let target = crate::deploy::target::target_for(
        &data,
        &game,
        &settings,
        data.profile_state("p1"),
        &profile,
    )
    .unwrap();
    assert_eq!(
        target.settings,
        Some(
            data.profile_state("p1")
                .join("userdata/documents/upper/mods.settings")
        )
    );
}

#[cfg(windows)]
#[test]
fn a_redirect_prepares_the_upper_layer_and_the_real_folder() {
    let (data, game, settings, real) = fixture("user-data-prepare");
    let profile = profile(true);
    let sets = sets(&data, &game, &settings, &profile, &data.profile_state("p1")).unwrap();
    let _ = fs::remove_dir_all(&real);
    prepare(&sets).unwrap();
    assert!(real.is_dir());
    assert!(sets[0].upper.is_dir());
}

#[test]
fn prepare_does_not_create_a_missing_parent_folder() {
    let base = scratch!("user-data-parent");
    let real = base.join("missing/child");
    let set = Set {
        id: "saves".into(),
        kind: Kind::Saves,
        real: real.clone(),
        mode: Mode::Redirect,
        isolated: true,
        inherit: true,
        base: real.clone(),
        upper: base.join("upper"),
        store: base.join("copy"),
    };
    let error = prepare(&[set]).unwrap_err();
    assert_eq!(error.code, "user_data.parent_missing");
    assert!(!real.parent().unwrap().exists(), "the parent stays absent");
}
