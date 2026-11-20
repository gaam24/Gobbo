//! Tests for the service layer. The fixtures build a full library on disk through the public
//! functions, the same way the UI drives them.

#[cfg(target_os = "linux")]
use crate::deploy::ops::apply_deployment;
use crate::domain::pathkey::path_key;
use crate::domain::profiles::Row;
use crate::error::Message;
use crate::games::GameDef;
#[cfg(target_os = "linux")]
use crate::launch::start::{Launch, launch_command};
use crate::packs::model::PackArchives;
use crate::packs::model::PackInfo;
use crate::packs::model::PackOptions;
use crate::profiles::actions::create_profile;
use crate::profiles::actions::save_profile;
use crate::providers::nexus::NexusRequirement;
use crate::settings::config::Settings;
use crate::settings::config::save_settings;
use crate::status::sync_profiles as status;
use crate::test_support::import;

mod acquire;
mod catalog;
mod community;
mod deploy;
mod downloads;
mod health;
mod helpers;
mod history;
mod import;
mod launch;
mod library;
mod mo2;
mod mod_edits;
mod packs;
mod parity;
mod profiles;
mod proxies;
mod requirements;
mod settings;
mod snapshots;
mod unity;
mod updates;
mod winners;

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::context::{Data, load_library, load_profiles};

use crate::error::AppError;
use crate::library::{Family, set_mod_source};
use crate::providers::picture::read_picture;
use crate::settings::guard::may_open;
use crate::sources::downloads::{DownloadEvent, DownloadState, Downloads};
use crate::sources::update::fill_source_info;
use crate::test_support::games as test_games;
use crate::test_support::{row, rows, scratch, separator_row};

/// A fixture with a mod slot, mirrors, shared files and an order file.
const SLOT_GAME: &str = test_games::SLOT_ID;

/// A fixture without slots, whose mods keep their game paths.
const MIRROR_GAME: &str = test_games::MIRROR_ID;

/// A fixture whose mods live in a user data folder.
const EXTERNAL_GAME: &str = test_games::EXTERNAL_ID;

/// A fixture that requires a loader in the game folder.
const PLUGIN_GAME: &str = test_games::PLUGIN_ID;

/// A fresh folder, separate for each test. It holds a test game and a settings folder, nothing real.
struct Fixture {
    data: Data,
    game_root: PathBuf,
    settings_dir: PathBuf,
    base: PathBuf,
}

fn fixture(name: &str) -> Fixture {
    test_games::install();
    let base = scratch!(name);
    let game_root = base.join("game");
    let settings_dir = base.join("settings");
    fs::create_dir_all(game_root.join("Mods")).unwrap();
    fs::create_dir_all(game_root.join("DLC")).unwrap();
    fs::create_dir_all(&settings_dir).unwrap();
    let fixture = Fixture {
        data: Data::new(base.join("data")),
        game_root,
        settings_dir,
        base,
    };
    save_settings(&fixture.data, SLOT_GAME, fixture.settings()).unwrap();
    fixture
}

/// A game in the mirror layout: just the game, without a settings folder.
fn mirror_fixture(name: &str) -> Fixture {
    let base = scratch!(name);
    let game_root = base.join("game");
    fs::create_dir_all(game_root.join("bin/x64")).unwrap();
    mirror_layout_fixture(base, game_root)
}

/// Saves a game's settings with its game folder and its mods folder set; every other field is empty.
fn save_folders(data: &Data, game_id: &str, game_root: &Path, mods_root: &str) {
    test_games::install();
    save_settings(
        data,
        game_id,
        Settings {
            game_root: game_root.display().to_string(),
            mods_root: mods_root.to_string(),
            ..Settings::default()
        },
    )
    .unwrap();
}

/// The fixture of a mirror-layout game at `game_root`, with its settings saved and no settings folder.
fn mirror_layout_fixture(base: PathBuf, game_root: PathBuf) -> Fixture {
    test_games::install();
    let fixture = Fixture {
        data: Data::new(base.join("data")),
        game_root,
        settings_dir: PathBuf::new(),
        base,
    };
    save_settings(&fixture.data, MIRROR_GAME, fixture.settings()).unwrap();
    fixture
}

/// A fixture for any test game: its game folder with `folders`, and its settings saved.
fn game_fixture(name: &str, game_id: &str, folders: &[&str]) -> Fixture {
    test_games::install();
    let base = scratch!(name);
    let game_root = base.join("game");

    for folder in folders {
        fs::create_dir_all(game_root.join(folder)).unwrap();
    }

    let fixture = Fixture {
        data: Data::new(base.join("data")),
        game_root,
        settings_dir: PathBuf::new(),
        base,
    };
    save_settings(&fixture.data, game_id, fixture.settings()).unwrap();
    fixture
}

/// A slot-game folder with the executable and the folders the definition marks.
fn slot_game_folder(base: &Path) -> PathBuf {
    test_games::install();
    let game = base.join("Slot Game");
    fs::create_dir_all(game.join("bin")).unwrap();
    fs::create_dir_all(game.join("content/base")).unwrap();
    fs::write(game.join("bin/game.exe"), b"").unwrap();
    game
}

/// Applies a checked Nexus page to a family, as the update engine does, so the page tests keep their fixture.
fn apply_nexus_page(
    family: &mut Family,
    nexus_id: u32,
    page: &crate::providers::nexus::ModPage,
    checked_at: u32,
    picture: Option<String>,
) {
    let old = family
        .source("nexus")
        .and_then(|source| source.meta.as_ref());
    let meta = crate::providers::nexus::checked_page_meta(old, page, checked_at, picture);
    let entry = family
        .sources
        .entry(crate::domain::source::ProviderId::new("nexus"))
        .or_default();
    if entry.mod_id.is_empty() {
        entry.mod_id = nexus_id.to_string();
    }

    entry.meta = Some(meta);
}

/// Two versions of a Nexus mod, 1.2.0 and 1.2.1, as zips in the fixture's folder.
fn two_mod_versions(f: &Fixture) -> (PathBuf, PathBuf) {
    let old = f.zip(
        "Test Mod 35686 1.2.0 2026-10-23T17-21Z Q6xF6llIa.zip",
        &[("r6/scripts/TestMod/a.reds", "old")],
    );
    let new = f.zip(
        "Test Mod 35686 1.2.1 2026-11-01T09-00Z Abc123xyz.zip",
        &[("r6/scripts/TestMod/a.reds", "new")],
    );
    (old, new)
}

/// Imports one mod into the mirror fixture and deploys it through the overlay.
#[cfg(target_os = "linux")]
fn deploy_overlay(f: &Fixture) {
    let zip = f.zip("pack.zip", &[("r6/scripts/Mod/a.reds", "x")]);
    import(&f.data, MIRROR_GAME, &zip, None, &mut |_, _| {}).unwrap();
    f.enable_all(MIRROR_GAME);
    apply_deployment(&f.data, MIRROR_GAME).unwrap();
}

/// Starts the mirror game of the fixture the way the app does, and waits for it.
#[cfg(target_os = "linux")]
fn start_mirror(f: &Fixture) {
    let (launch, _, _) =
        launch_command(&test_games::mirror(), &f.data, MIRROR_GAME, None, &f.base).unwrap();
    run_launch(launch);
}

/// Writes a script and makes it runnable, the way a test sets up a fake Proton.
#[cfg(target_os = "linux")]
fn write_script(path: &Path, text: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

impl Fixture {
    fn settings(&self) -> Settings {
        Settings {
            game_root: self.game_root.display().to_string(),
            settings_dir: self.settings_dir.display().to_string(),
            ..Settings::default()
        }
    }

    /// A zip with mods in the test folder, laid out like an archive from Nexus.
    fn zip(&self, name: &str, entries: &[(&str, &str)]) -> PathBuf {
        crate::test_support::make_zip(&self.base, name, entries)
    }

    /// Two mods with the same file `content/x.ws`, so there is one conflict: modA wins.
    fn import_two_mods(&self) {
        for (name, folder, text) in [
            ("modA.zip", "modA", "from A"),
            ("modB.zip", "modB", "from B"),
        ] {
            let entry = format!("Mods/{folder}/content/x.ws");
            let zip = self.zip(name, &[(entry.as_str(), text)]);
            import(&self.data, SLOT_GAME, &zip, None, &mut |_, _| {}).unwrap();
        }
    }

    /// Enables every mod of the library in the active profile. An import adds mods disabled, so
    /// tests that deploy or count conflicts have to switch them on first.
    fn enable_all(&self, game_id: &str) {
        let rows: Vec<Row> = status(&self.data, game_id)
            .unwrap()
            .mods
            .iter()
            .map(|mod_view| row(&mod_view.folder, true))
            .collect();
        save_profile(&self.data, game_id, &rows).unwrap();
    }

    fn active_id(&self, game_id: &str) -> String {
        status(&self.data, game_id)
            .unwrap()
            .profiles
            .into_iter()
            .find(|p| p.active)
            .unwrap()
            .id
    }

    fn profile_id(&self, game_id: &str, name: &str) -> String {
        status(&self.data, game_id)
            .unwrap()
            .profiles
            .into_iter()
            .find(|p| p.name == name)
            .unwrap()
            .id
    }

    fn save_settings(&self, game_id: &str, settings: Settings) -> Result<(), AppError> {
        save_settings(&self.data, game_id, settings)
    }

    /// Creates a profile named Light next to the active one. Returns the ids of the active profile and of Light.
    fn default_and_light(&self) -> (String, String) {
        let default = self.active_id(SLOT_GAME);
        create_profile(&self.data, SLOT_GAME, "Light", None, false).unwrap();
        (default, self.profile_id(SLOT_GAME, "Light"))
    }
}

/// Starts a launch and waits for it. The tests use both plain commands and overlay sessions.
#[cfg(target_os = "linux")]
fn run_launch(launch: Launch) {
    match launch {
        Launch::Plain(mut command) => assert!(command.status().unwrap().success()),
        Launch::Layered {
            session,
            program,
            args,
            env,
        } => {
            let mut running =
                crate::deploy::overlay::start(&session, &program, &args, &env, None).unwrap();
            assert_eq!(running.wait().unwrap(), 0);
        }
    }
}

/// The Steam library as on disk: the game in `steamapps/common`, the prefix in `compatdata`, a fake Proton. Proton does not
/// start the game. It writes to `out` the Steam variables, the arguments, and whether the mod file is visible.
#[cfg(target_os = "linux")]
fn steam_fixture(name: &str) -> (Fixture, PathBuf) {
    let base = scratch!(name);
    let library = base.join("library");
    let game_root = library.join("steamapps/common/Mirror Game");
    fs::create_dir_all(game_root.join("bin/x64")).unwrap();
    fs::create_dir_all(library.join("steamapps/compatdata/2002/pfx")).unwrap();
    let proton_dir = library.join("steamapps/common/Proton 9.0");
    fs::create_dir_all(&proton_dir).unwrap();

    let out = base.join("out.txt");
    let probe = game_root.join("r6/scripts/Mod/a.reds");
    let script = format!(
        "#!/bin/sh\n{{ printf '%s\\n' \"$STEAM_COMPAT_DATA_PATH\" \"$SteamAppId\" \"$@\"; \
             if [ -f '{}' ]; then echo visible; else echo hidden; fi; }} > '{}'\n",
        probe.display(),
        out.display()
    );
    write_script(&proton_dir.join("proton"), &script);
    (mirror_layout_fixture(base, game_root), out)
}

/// The mirror fixture without Steam app ids, so the launch goes the native way.
fn native_game() -> GameDef {
    let mut game = test_games::mirror();
    game.detect.steam_app_ids.clear();
    game
}

/// A game folder outside any Steam library, with the exe on disk: a native game (GOG, from disk).
fn native_fixture(name: &str) -> Fixture {
    let base = scratch!(name);
    let game_root = base.join("Games/Native Game");
    fs::create_dir_all(game_root.join("bin/x64")).unwrap();
    fs::write(game_root.join("bin/x64/game.exe"), "#!/bin/sh\n").unwrap();
    mirror_layout_fixture(base, game_root)
}

/// Pack options for every profile of the fixture's game.
fn pack_options(f: &Fixture, version: &str, archives: PackArchives) -> PackOptions {
    PackOptions {
        info: PackInfo {
            id: String::new(),
            name: "Test pack".into(),
            version: version.into(),
            author: "Gobbo".into(),
            description: String::new(),
        },
        profiles: status(&f.data, SLOT_GAME)
            .unwrap()
            .profiles
            .into_iter()
            .map(|p| p.id)
            .collect(),
        archives,
    }
}
