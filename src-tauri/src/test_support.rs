//! Shared helpers for unit tests: a fresh folder, a text file, a zip, and the game definitions the tests use.
//!
//! The definitions below are fixtures, not shipped games. Each one covers a capability (a slot, a mirror, a root
//! outside the game, a required file, a flat slot), so adding a game to `games/` never changes a test here. The one
//! test that reads `games/` walks every folder, so a new game is checked without a new test.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::domain::profiles::entry::ProfileMod;
use crate::domain::profiles::{Entry, Profile, Profiles, Row};
use crate::library::{ModInfo, ModInfoEdit, ModVersion};

/// A fresh folder for the test that calls it, named after the calling module: `scratch!("name")`.
macro_rules! scratch {
    ($name:expr) => {
        $crate::test_support::scratch_in(module_path!(), $name)
    };
}

pub(crate) use scratch;

/// A fresh folder in the system temporary directory, separate for each test. A folder with the same
/// name from an earlier run is removed first, so a failed run does not poison the next one.
pub fn scratch_in(module: &str, name: &str) -> PathBuf {
    clean_stale_scratch();
    // The whole module path keeps two test modules with the same test name apart.
    let module = module.replace("::", "-");
    let dir = std::env::temp_dir().join(format!("gobbo-{module}-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Whether this machine can start the user and mount namespace the Linux overlay needs. A GitHub-hosted runner can
/// refuse `unshare` even with the tools installed; the answer is cached because the kernel does not change mid-run.
#[cfg(unix)]
pub fn overlay_namespaces_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        if cfg!(not(target_os = "linux")) {
            return true;
        }

        std::process::Command::new("unshare")
            .args([
                "--user",
                "--map-root-user",
                "--mount",
                "--propagation",
                "private",
                "--",
                "true",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// Returns from a test that needs a real overlay session when this machine cannot create the namespace. Production
/// reports the same limit as `deploy.overlay_unavailable`; a test cannot make the runner allow it.
#[cfg(unix)]
macro_rules! require_overlay_namespaces {
    () => {
        if !$crate::test_support::overlay_namespaces_available() {
            eprintln!("skipping: this machine cannot create the overlay user namespace");
            return;
        }
    };
}
#[cfg(unix)]
pub(crate) use require_overlay_namespaces;

/// Removes old scratch folders once per test process. Every test leaves its folder for inspection; without this, a
/// long series of runs fills `/tmp` (and a full `/tmp` breaks the next C++ build).
fn clean_stale_scratch() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
            return;
        };
        let now = std::time::SystemTime::now();

        for entry in entries.flatten() {
            let name = entry.file_name();

            if !name.to_string_lossy().starts_with("gobbo-") {
                continue;
            }

            let old = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .is_some_and(|age| age > std::time::Duration::from_secs(3600));
            if old {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    });
}

/// A text file, with the parent folders created.
pub fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// The names directly inside `dir`, sorted. Tests use it to prove that a failed write left no temporary file.
pub fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_unstable();
    names
}

/// A zip with the given entries (name, content) in the folder `dir`.
pub fn make_zip(dir: &Path, file_name: &str, entries: &[(&str, &str)]) -> PathBuf {
    let path = dir.join(file_name);
    let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();

    for (name, text) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(text.as_bytes()).unwrap();
    }

    writer.finish().unwrap();
    path
}

/// A profile row with the given folder and enabled flag.
pub fn row(folder: &str, enabled: bool) -> Row {
    Row {
        folder: folder.to_string(),
        enabled,
        separator: None,
    }
}

/// Profile rows from `(folder, enabled)` pairs, in order.
pub fn rows(rows: &[(&str, bool)]) -> Vec<Row> {
    rows.iter()
        .map(|(folder, enabled)| row(folder, *enabled))
        .collect()
}

/// A separator row; its `folder` is the separator id.
pub fn separator_row(id: &str, name: &str) -> Row {
    Row {
        folder: id.to_string(),
        enabled: false,
        separator: Some(name.to_string()),
    }
}

/// A profile entry in the form the deployment understands.
pub fn profile_mod(folder: &str, slot: &str, enabled: bool) -> ProfileMod {
    ProfileMod {
        id: folder.to_string(),
        folder: folder.to_string(),
        slot: slot.to_string(),
        enabled,
        priority: None,
    }
}

/// Profile entries for `folders`, all in `slot` and enabled.
pub fn profile_mods(folders: &[&str], slot: &str) -> Vec<ProfileMod> {
    folders
        .iter()
        .map(|folder| profile_mod(folder, slot, true))
        .collect()
}

/// A profile with one entry per `(family, folder)` pair.
pub fn profile(id: &str, mods: &[(&str, &str)]) -> Profile {
    let mods = mods
        .iter()
        .map(|(family, folder)| Entry {
            id: (*family).to_string(),
            folder: (*folder).to_string(),
            enabled: true,
            priority: None,
        })
        .collect();
    Profile::new(id.to_string(), id.to_string(), mods)
}

/// A profile set with `active` selected and no last launch.
pub fn profiles(active: &str, profiles: Vec<Profile>) -> Profiles {
    Profiles {
        schema_version: crate::domain::STATE_SCHEMA,
        active: active.to_string(),
        last_launched: None,
        profiles,
    }
}

/// One installed version of a mod, with empty files and no archive.
pub fn mod_version(folder: &str, family: &str, name: &str, version: Option<&str>) -> ModVersion {
    ModVersion {
        folder: folder.to_string(),
        family: family.to_string(),
        slot: String::new(),
        files: Arc::from(Vec::new()),
        size_bytes: 0,
        info: ModInfo {
            name: name.to_string(),
            version: version.map(str::to_string),
            author: None,
            description: None,
        },
        edit: ModInfoEdit::default(),
        archive: None,
        parsed_sources: BTreeMap::new(),
        parsed_parts: BTreeMap::new(),
        installed_at: 0,
        fomod: Vec::new(),
        fomod_installer: false,
        integrity: None,
    }
}

/// One mod an import placed in the library.
pub fn installed(
    folder: &str,
    slot: &str,
    paths: &[&str],
    size_bytes: u64,
) -> crate::import::install::Installed {
    crate::import::install::Installed {
        folder: folder.to_string(),
        slot: slot.to_string(),
        files: paths.len(),
        paths: paths.iter().map(|path| (*path).to_string()).collect(),
        size_bytes,
        integrity: None,
    }
}

/// A deployment target under `base`: the game folder `base/game`, the library `base/library` and the state folder.
pub fn target(base: &Path) -> crate::deploy::Target {
    let root = base.join("game");
    crate::deploy::Target {
        root: root.clone(),
        game_root: root,
        settings: None,
        state: base.join("state"),
        user_data: Vec::new(),
    }
}

/// The game definitions of the tests. Every capability a definition can have is covered by one fixture, and no
/// fixture names a shipped game. Tests that need a real definition call `load` after `install`; tests that only
/// parse one use the constants.
pub mod games {
    use std::fs;
    use std::path::Path;
    use std::sync::OnceLock;

    use crate::games::{GameDef, set_definition_dirs};

    pub const SLOT_ID: &str = "slot";
    pub const MIRROR_ID: &str = "mirror";
    pub const FULL_ROOT_ID: &str = "full-root";
    pub const EXTERNAL_ID: &str = "external";
    pub const USER_DATA_ID: &str = "user-data";
    pub const USER_DATA_COPY_ID: &str = "user-data-copy";
    pub const PLUGIN_ID: &str = "plugin";
    pub const FLAT_ID: &str = "flat";
    pub const ALIASED_ID: &str = "aliased";
    pub const LOADER_ID: &str = "fixture-loader";
    pub const LOADED_ID: &str = "loaded";
    pub const LOADED_DEFAULT_ID: &str = "loaded-default";

    /// A game with a mod slot, a mirrored folder, shared files, an order file and a tool.
    pub const SLOT: &str = r#"
schema = 1
id = "slot"
name = "Slot Game"
providers = { nexus = { key = "slot" } }

[detect]
steam_app_ids = [1001]
steam_install_dirs = ["Slot Game"]
markers = ["bin/game.exe", "content/base"]

[[executables]]
name = "Game"
path = "bin/game.exe"

[deploy]
protected = ["bin"]

[[deploy.slots]]
dir = "Mods"
prefix = "mod"
marker = "content"

[[deploy.mirror]]
dir = "DLC"
prefix = "dlc"

[[deploy.mirror]]
dir = "bin/config"
extensions = [".xml", ".ini"]

[[deploy.shared]]
dir = "content"
extensions = [".ws", ".xml", ".png", ".dds"]

[load_order]
format = "mods_settings"
file = "{documents}/Slot Game/mods.settings"

[[tools]]
id = "merger"
name = "Script Merger"
detect = ["tools/tool.exe"]
output = "generated-mod"
[tools.source]
provider = "nexus"
mod_id = "4242"
[[tools.prerequisites]]
name = "Runtime 8.0"
url = "https://builds.example.com/runtime.exe"
sha512 = "b622106783dee5e1ce993ec849ecd43fa147c6230daecf910348f6adcd28f63abf39cf6db832a6a2684941c19a9321816e9cbc9dbe8c4a8bb2d7e46487ab3890"
args = ["/install", "/quiet", "/norestart"]
check = "drive_c/Runtime"
check_starts_with = "8."
"#;

    /// A game without slots: every file keeps its game path.
    pub const MIRROR: &str = r#"
schema = 1
id = "mirror"
name = "Mirror Game"
providers = { nexus = { key = "mirror" } }

[detect]
steam_app_ids = [2002]
steam_install_dirs = ["Mirror Game"]
markers = ["bin/x64/game.exe"]

[[executables]]
name = "Game"
path = "bin/x64/game.exe"

[deploy]

[[deploy.mirror]]
dir = "archive/pc/mod"

[[deploy.mirror]]
dir = "r6"

[[deploy.mirror]]
dir = "red4ext"

[[deploy.mirror]]
dir = "engine"

[[deploy.mirror]]
dir = "bin/x64/plugins"

[[deploy.mirror]]
dir = "bin/x64/loader.dll"

# A DLL next to the program, for the static-import overlay test. Single files, so the other mirror rules keep their
# exact meaning (`bin/x64/loader.dll.bak` is not a mirror).
[[deploy.mirror]]
dir = "bin/x64/mod.dll"

[[deploy.mirror]]
dir = "bin/x86/mod.dll"
"#;

    /// A game whose whole deployment root is the mod layout. A loose `.archive` moves to `archive/pc/mod`.
    pub const FULL_ROOT: &str = r#"
schema = 1
id = "full-root"
name = "Full Root Game"
providers = { nexus = { key = "full-root" } }

[detect]
steam_app_ids = [2003]
steam_install_dirs = ["Full Root Game"]
markers = ["bin/x64/game.exe"]

[[executables]]
name = "Game"
path = "bin/x64/game.exe"

[deploy]
full_root = true

[[deploy.move]]
extensions = [".archive", ".xl"]
to = "archive/pc/mod"
"#;

    /// A game whose mods live in a folder outside the game folder.
    pub const EXTERNAL: &str = r#"
schema = 1
id = "external"
name = "External Game"
providers = { nexus = { key = "external" } }

[detect]
steam_app_ids = [3003]
steam_install_dirs = ["External Game"]
markers = ["game.exe"]

[[executables]]
name = "Game"
path = "game.exe"
args = ["-flag"]

[deploy]
root = "{appdata}/External Game"
root_linux = "{local_appdata}/External Game"

[[deploy.slots]]
dir = "Mods"
marker = "ModInfo.xml"
"#;

    /// A game with one user-data set outside its folder, for the move-back and size tests.
    pub const USER_DATA: &str = r#"
schema = 1
id = "user-data"
name = "User Data Game"
providers = { nexus = { key = "user-data" } }

[detect]
steam_app_ids = [7007]
steam_install_dirs = ["User Data Game"]
markers = ["game.exe"]

[[executables]]
name = "Game"
path = "game.exe"

[deploy]

[[deploy.slots]]
dir = "Mods"
marker = "content"

[[user_data]]
id = "documents"
path = "{settings}/user"
mode = "redirect"
default = "isolated"
inherit = true
"#;

    /// The same user-data set, declared as a copy: a platform without a redirect copies it around a launch.
    pub const USER_DATA_COPY: &str = r#"
schema = 1
id = "user-data-copy"
name = "User Data Copy Game"
providers = { nexus = { key = "user-data-copy" } }

[detect]
markers = ["game.exe"]

[[executables]]
name = "Game"
path = "game.exe"

[deploy]

[[deploy.slots]]
dir = "Mods"
marker = "content"

[[user_data]]
id = "documents"
path = "{settings}/user"
mode = "copy"
default = "isolated"
"#;

    /// A game that needs a loader in the game folder and takes bare plugins.
    pub const PLUGIN: &str = r#"
schema = 1
id = "plugin"
name = "Plugin Game"
providers = { nexus = { key = "plugin" } }
requires = ["winhttp.dll", "BepInEx/core/BepInEx.dll"]

[detect]
steam_app_ids = [4004]
steam_install_dirs = ["Plugin Game"]
markers = ["game.exe"]

[[executables]]
name = "Game"
path = "game.exe"

[deploy]
root_files = ["winhttp.dll"]

[[deploy.slots]]
dir = "BepInEx/plugins"
root_extensions = [".dll"]
root_folder_extensions = [".dll"]

[[deploy.mirror]]
dir = "BepInEx/core"

[[deploy.mirror]]
dir = "BepInEx/config"
"#;

    /// A game whose mods are loose files in one flat slot.
    pub const FLAT: &str = r#"
schema = 1
id = "flat"
name = "Flat Game"

[detect]
steam_app_ids = [5005]

[[executables]]
name = "Flat Game"
path = "flat.exe"

[deploy]

[[deploy.slots]]
dir = "ph_ft/source"
flat = true
extensions = [".pak"]
root_extensions = [".pak"]
"#;

    /// The plugin game with archive folder aliases: an archive's `plugins/`, `core/` and `config/` folders stand for the
    /// game's `BepInEx` folders. The same layout as a Thunderstore loader pack, without the aliases, is `PLUGIN`. Its
    /// `BepInEx/patchers` mirror has no alias, so a test can check that a real game folder is not taken as a wrapper.
    pub const ALIASED: &str = r#"
schema = 1
id = "aliased"
name = "Aliased Game"
providers = { thunderstore = { key = "aliased" } }
requires = ["winhttp.dll", "BepInEx/core/BepInEx.dll"]

[detect]
steam_app_ids = [6006]
steam_install_dirs = ["Aliased Game"]
markers = ["game.exe"]

[[executables]]
name = "Game"
path = "game.exe"

[deploy]
root_files = ["winhttp.dll"]

[deploy.aliases]
plugins = "BepInEx/plugins"
core = "BepInEx/core"
config = "BepInEx/config"

[[deploy.slots]]
dir = "BepInEx/plugins"
root_extensions = [".dll"]
root_folder_extensions = [".dll"]

[[deploy.mirror]]
dir = "BepInEx/core"

[[deploy.mirror]]
dir = "BepInEx/config"

# No alias: a pack that keeps the BepInEx root must still reach this mirror.
[[deploy.mirror]]
dir = "BepInEx/patchers"
"#;

    /// A loader preset with a base rule and two variants. The loaded game takes variant `b`.
    pub const LOADER: &str = r#"
schema = 1
id = "fixture-loader"
name = "Fixture Loader"
default_variant = "a"
requires = ["proxy.dll"]

[deploy]
root_files = ["proxy.dll"]

[deploy.aliases]
plugins = "Mods"

[[deploy.slots]]
dir = "Mods"
root_extensions = [".dll"]

[[deploy.mirror]]
dir = "Core"

[variants."a"]
requires = ["Core/a.dll"]

[variants."b"]
requires = ["Core/b.dll"]

[[variants."b".deploy.mirror]]
dir = "Extra"
"#;

    /// A game that takes the fixture loader and adds nothing of its own, so its whole deploy layout comes from it.
    pub const LOADED: &str = r#"
schema = 1
id = "loaded"
name = "Loaded Game"
loader = { id = "fixture-loader", variant = "b" }

[detect]
markers = ["game.exe"]

[[executables]]
name = "Game"
path = "game.exe"
"#;

    /// The same game with the preset's default variant.
    pub const LOADED_DEFAULT: &str = r#"
schema = 1
id = "loaded-default"
name = "Loaded Default Game"
loader = "fixture-loader"

[detect]
markers = ["game.exe"]

[[executables]]
name = "Game"
path = "game.exe"
"#;

    const ALL: &[(&str, &str)] = &[
        (SLOT_ID, SLOT),
        (MIRROR_ID, MIRROR),
        (FULL_ROOT_ID, FULL_ROOT),
        (EXTERNAL_ID, EXTERNAL),
        (USER_DATA_ID, USER_DATA),
        (USER_DATA_COPY_ID, USER_DATA_COPY),
        (PLUGIN_ID, PLUGIN),
        (FLAT_ID, FLAT),
        (ALIASED_ID, ALIASED),
        (LOADED_ID, LOADED),
        (LOADED_DEFAULT_ID, LOADED_DEFAULT),
    ];

    /// Writes the fixture definitions into one folder per process and points `games::load` at it. Called by `load`
    /// and by every helper that reaches a service function; the folder is written once.
    pub fn install() {
        static INSTALLED: OnceLock<()> = OnceLock::new();
        INSTALLED.get_or_init(|| {
            let games =
                std::env::temp_dir().join(format!("gobbo-test-games-{}", std::process::id()));
            let loaders =
                std::env::temp_dir().join(format!("gobbo-test-loaders-{}", std::process::id()));
            let _ = fs::remove_dir_all(&games);
            let _ = fs::remove_dir_all(&loaders);

            for (id, text) in ALL {
                let folder = games.join(id);
                fs::create_dir_all(&folder).unwrap();
                fs::write(folder.join("game.toml"), text).unwrap();
            }

            fs::create_dir_all(&loaders).unwrap();
            fs::write(loaders.join(format!("{LOADER_ID}.toml")), LOADER).unwrap();
            // The shipped definitions and loader presets are copied as they are, so an app test runs a real game
            // through the same flow as a fixture: `test_games::load(id)` is the shipped file for `id`.
            for_each_shipped(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../games"),
                Path::is_dir,
                |name, path| {
                    let folder = games.join(name);
                    fs::create_dir_all(&folder).unwrap();
                    fs::copy(path.join("game.toml"), folder.join("game.toml")).unwrap();
                },
            );
            for_each_shipped(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../loaders"),
                |path| {
                    path.is_file()
                        && path
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
                },
                |name, path| {
                    fs::copy(path, loaders.join(name)).unwrap();
                },
            );
            set_definition_dirs(games, loaders);
        });
    }

    /// Calls `each` with the name and path of every entry of the shipped folder `dir` that `keep` accepts. A name that
    /// starts with `_` marks a template, not a shipped definition, and is skipped.
    fn for_each_shipped(
        dir: &str,
        keep: impl Fn(&Path) -> bool,
        mut each: impl FnMut(&str, &Path),
    ) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();

            if keep(&path) && !name.starts_with('_') {
                each(&name, &path);
            }
        }
    }

    /// A fixture definition read and validated the same way a shipped one is.
    pub fn load(id: &str) -> GameDef {
        install();
        crate::games::load(id).unwrap()
    }

    pub fn slot() -> GameDef {
        load(SLOT_ID)
    }

    pub fn mirror() -> GameDef {
        load(MIRROR_ID)
    }

    pub fn full_root() -> GameDef {
        load(FULL_ROOT_ID)
    }

    pub fn external() -> GameDef {
        load(EXTERNAL_ID)
    }

    pub fn plugin() -> GameDef {
        load(PLUGIN_ID)
    }

    pub fn flat() -> GameDef {
        load(FLAT_ID)
    }

    pub fn aliased() -> GameDef {
        load(ALIASED_ID)
    }

    pub fn loaded() -> GameDef {
        load(LOADED_ID)
    }

    /// The definition with `\n` line endings. A Windows checkout may hold `\r\n`, and the tests search for `\n`.
    pub fn lf(text: &str) -> String {
        text.replace("\r\n", "\n")
    }
}

/// Imports an archive with no known source, as a loose file the user picked.
pub fn import(
    root: &crate::context::Data,
    game_id: &str,
    zip: &Path,
    choices: Option<&[crate::import::fomod::Choice]>,
    progress: &mut dyn FnMut(crate::import::model::ImportStage, u32),
) -> Result<crate::import::ImportView, crate::error::AppError> {
    import_from(root, game_id, zip, choices, None, progress)
}

/// Imports an archive with the provider file it came from.
pub fn import_from(
    root: &crate::context::Data,
    game_id: &str,
    zip: &Path,
    choices: Option<&[crate::import::fomod::Choice]>,
    source: Option<crate::domain::source::SourceRef>,
    progress: &mut dyn FnMut(crate::import::model::ImportStage, u32),
) -> Result<crate::import::ImportView, crate::error::AppError> {
    let request = crate::import::ImportRequest::new(zip)
        .with_choices(choices.map(<[crate::import::fomod::Choice]>::to_vec))
        .with_source(source);
    crate::import::import_archive(root, game_id, request, None, progress)
}
