//! Proton on Linux: the builds Steam can use and the `proton` file of one. Steam looks in the `compatibilitytools.d`
//! folder of its install, in `steamapps/common` of every library for Valve's own Proton, in the system-wide
//! `/usr/share/steam` and `/usr/local/share/steam`, and in the folders named by `STEAM_EXTRA_COMPAT_TOOLS_PATHS`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::platform::ProtonBuild;

/// Places where Steam is usually installed on Linux, relative to the home folder: natively, in `~/.steam`, the Debian
/// package, Flatpak and Snap. A missing folder is skipped when the libraries are read.
const STEAM_HOMES: [&str; 7] = [
    ".local/share/Steam",
    ".steam/steam",
    ".steam/root",
    ".steam/debian-installation",
    ".var/app/com.valvesoftware.Steam/data/Steam",
    ".var/app/com.valvesoftware.Steam/.local/share/Steam",
    "snap/steam/common/.steam/root",
];

/// The system-wide compatibility tool folders Steam scans, in Steam's own order.
const SYSTEM_COMPAT_DIRS: [&str; 2] = [
    "/usr/share/steam/compatibilitytools.d",
    "/usr/local/share/steam/compatibilitytools.d",
];

/// The folders where Steam may be installed on this computer.
pub fn steam_installs(home: &Path) -> Vec<PathBuf> {
    STEAM_HOMES.iter().map(|dir| home.join(dir)).collect()
}

/// Steam folder: the first standard install that exists. When none exists, we take the game's library.
pub fn steam_root(home: &Path, library: &Path) -> PathBuf {
    steam_installs(home)
        .into_iter()
        .find(|dir| dir.is_dir())
        .unwrap_or_else(|| library.to_path_buf())
}

/// The `proton` file: first from the library, then from `compatibilitytools.d` of the Steam folder. A system-wide tool
/// is not a fallback; Steam's own choice names it through `proton_named`, and the game settings list it.
pub fn find_proton(steam: &Path, library: &Path) -> Option<PathBuf> {
    newest_proton(&library.join("steamapps/common"), |name| {
        name.starts_with("Proton")
    })
    .or_else(|| newest_proton(&steam.join("compatibilitytools.d"), |_| true))
}

/// Every Proton build on this computer: a folder with a `proton` file in any folder Steam scans. Sorted by the name
/// Steam shows, each folder once; a tool installed twice is listed once, under the user's copy. Empty when there is none.
pub fn installed_protons(steams: &[PathBuf], libraries: &[PathBuf]) -> Vec<ProtonBuild> {
    installed_protons_in(&proton_dirs(steams, libraries))
}

/// The folders to scan, most specific first: the user's `compatibilitytools.d` of every Steam install, Valve's Proton in
/// every library, the system-wide folders and `STEAM_EXTRA_COMPAT_TOOLS_PATHS`.
fn proton_dirs(steams: &[PathBuf], libraries: &[PathBuf]) -> Vec<PathBuf> {
    build_dirs(steams, libraries, &extra_compat_dirs())
}

/// `proton_dirs` with the folders from the environment given, so a test can supply its own.
fn build_dirs(steams: &[PathBuf], libraries: &[PathBuf], extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = steams
        .iter()
        .map(|steam| steam.join("compatibilitytools.d"))
        .chain(
            libraries
                .iter()
                .map(|library| library.join("steamapps/common")),
        )
        .collect();
    dirs.extend(system_compat_dirs());
    dirs.extend(extra.iter().cloned());
    dirs
}

/// Every build in `dirs`, with the tool key and the display name from `compatibilitytool.vdf` when the tool has one.
fn installed_protons_in(dirs: &[PathBuf]) -> Vec<ProtonBuild> {
    let mut candidates: Vec<Candidate> = Vec::new();

    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };

        for entry in entries.filter_map(Result::ok) {
            let folder = entry.path();

            if !folder.join("proton").is_file() {
                continue;
            }

            let tool = compat_tool(&folder);
            let folder_name = entry.file_name().to_string_lossy().into_owned();
            candidates.push(Candidate {
                build: ProtonBuild {
                    name: tool.display_name.unwrap_or_else(|| folder_name.clone()),
                    folder: folder.display().to_string(),
                },
                key: tool.key.unwrap_or(folder_name),
            });
        }
    }
    // Two Steam folders that link to one install name the same build: it is listed once, under the first path. A tool
    // installed both for the user and system-wide has one key, and the user's copy is the one found first.
    let mut seen_folders = HashSet::new();
    let mut seen_tools = HashSet::new();
    candidates.retain(|candidate| {
        let real = fs::canonicalize(&candidate.build.folder)
            .unwrap_or_else(|_| PathBuf::from(&candidate.build.folder));
        seen_folders.insert(real) && seen_tools.insert(tool_key(&candidate.key))
    });
    let mut builds: Vec<ProtonBuild> = candidates
        .into_iter()
        .map(|candidate| candidate.build)
        .collect();
    builds.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.folder.cmp(&b.folder)));
    builds
}

/// A build and the tool key Steam uses, from `compatibilitytool.vdf` or else the folder name.
struct Candidate {
    build: ProtonBuild,
    key: String,
}

/// The `proton` file of the installed tool that Steam calls `name`: a folder in `compatibilitytools.d`, in the library's
/// `steamapps/common`, or in any other folder Steam scans, whose folder name or `compatibilitytool.vdf` key matches. `None`
/// when no such folder has a `proton` file.
pub fn proton_named(steam: &Path, library: &Path, name: &str) -> Option<PathBuf> {
    let dirs = build_dirs(
        &[steam.to_path_buf()],
        &[library.to_path_buf()],
        &extra_compat_dirs(),
    );
    proton_named_in(&dirs, name)
}

/// `proton_named` over the given folders, so a test does not read the whole computer.
fn proton_named_in(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    let wanted = tool_key(name);
    dirs.iter()
        .filter_map(|dir| fs::read_dir(dir).ok())
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("proton").is_file())
        .find(|entry| {
            let folder = entry.file_name().to_string_lossy().into_owned();
            tool_key(&folder) == wanted
                || compat_tool(&entry.path())
                    .key
                    .is_some_and(|key| tool_key(&key) == wanted)
        })
        .map(|entry| entry.path().join("proton"))
}

/// The names a tool's `compatibilitytool.vdf` gives it: the key Steam writes in `CompatToolMapping` and the name Steam
/// shows. A tool without that file has neither, and its folder name is used.
#[derive(Default)]
struct CompatTool {
    key: Option<String>,
    display_name: Option<String>,
}

/// The names in the tool's `compatibilitytool.vdf`, when the file is there and readable.
fn compat_tool(dir: &Path) -> CompatTool {
    fs::read_to_string(dir.join("compatibilitytool.vdf"))
        .ok()
        .map(|text| parse_compat_tool(&text))
        .unwrap_or_default()
}

/// The first `compat_tools` entry of a `compatibilitytool.vdf`: its key and its display name. The key is the entry in
/// the block, and the name is the `display_name` below it.
fn parse_compat_tool(text: &str) -> CompatTool {
    let mut tool = CompatTool::default();
    let mut in_tools = false;
    let mut depth = 0usize;

    for line in text.lines().map(str::trim) {
        if !in_tools {
            if line == "\"compat_tools\"" {
                in_tools = true;
            }

            continue;
        }

        match line {
            "{" => depth += 1,
            "}" => {
                if depth == 0 {
                    break;
                }

                depth -= 1;

                if depth == 0 {
                    break;
                }
            }
            _ if depth == 1 => {
                if let ["", key, ""] = line.split('"').collect::<Vec<_>>().as_slice()
                    && tool.key.is_none()
                {
                    tool.key = Some((*key).to_string());
                }
            }
            _ if depth == 2 => {
                if let ["", "display_name", _, value, ""] =
                    line.split('"').collect::<Vec<_>>().as_slice()
                    && tool.display_name.is_none()
                {
                    tool.display_name = Some((*value).to_string());
                }
            }
            _ => {}
        }
    }

    tool
}

/// A tool's name the way Steam writes it: lowercase words joined by `_`, without a trailing `_0`. So `Proton 11.0`
/// and `proton_11` are the same tool, and `Proton Hotfix` and `proton_hotfix` are.
fn tool_key(name: &str) -> String {
    let key = name
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if let Some(stripped) = key.strip_suffix("_0") {
        return stripped.to_string();
    }

    key
}

/// The `proton` file in the folder given in the settings. Without this file, `None`.
pub fn proton_at(dir: &Path) -> Option<PathBuf> {
    let proton = dir.join("proton");
    proton.is_file().then_some(proton)
}

/// The last folder alphabetically that matches `wanted` and has a `proton` file. A folder without one is skipped.
fn newest_proton(dir: &Path, wanted: impl Fn(&str) -> bool) -> Option<PathBuf> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .ok()?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| wanted(name))
        .collect();
    names.sort();
    names
        .iter()
        .rev()
        .map(|name| dir.join(name).join("proton"))
        .find(|path| path.is_file())
}

/// The system-wide compatibility tool folders Steam scans.
fn system_compat_dirs() -> Vec<PathBuf> {
    SYSTEM_COMPAT_DIRS.iter().map(PathBuf::from).collect()
}

/// The compatibility tool folders named by `STEAM_EXTRA_COMPAT_TOOLS_PATHS`, a colon-separated list.
fn extra_compat_dirs() -> Vec<PathBuf> {
    std::env::var_os("STEAM_EXTRA_COMPAT_TOOLS_PATHS")
        .map(|paths| std::env::split_paths(&paths).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    /// The folder `<dir>/<name>` with the `proton` file.
    fn proton(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("proton"), "").unwrap();
        path
    }

    /// A `compatibilitytool.vdf` with one tool, as a packaged or manually added tool writes it.
    fn compatibilitytool(dir: &Path, key: &str, display_name: &str) {
        fs::write(
            dir.join("compatibilitytool.vdf"),
            format!(
                "\"compatibilitytools\"\n{{\n  \"compat_tools\"\n  {{\n    \"{key}\"\n    {{\n      \"install_path\" \".\"\n      \"display_name\" \"{display_name}\"\n      \"from_oslist\" \"windows\"\n      \"to_oslist\" \"linux\"\n    }}\n  }}\n}}\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn installed_protons_lists_each_build_once_by_name() {
        let base = scratch!("installed-protons");
        let steam = base.join("steam");
        let library = base.join("library");
        proton(&steam.join("compatibilitytools.d"), "GE-Proton9-20");
        proton(&library.join("steamapps/common"), "Proton 9.0");
        // A game folder without a `proton` file is not a build.
        fs::create_dir_all(library.join("steamapps/common/Some Game")).unwrap();

        // Two paths to one Steam folder (a link, as Flatpak and older installs make) still name each build once.
        let link = base.join("steam-link");
        std::os::unix::fs::symlink(&steam, &link).unwrap();
        let builds = installed_protons_in(&[
            steam.join("compatibilitytools.d"),
            link.join("compatibilitytools.d"),
            library.join("steamapps/common"),
        ]);

        let names: Vec<&str> = builds.iter().map(|build| build.name.as_str()).collect();
        assert_eq!(names, ["GE-Proton9-20", "Proton 9.0"]);
        assert!(builds[1].folder.ends_with("steamapps/common/Proton 9.0"));
    }

    #[test]
    fn a_tool_is_listed_under_the_name_steam_shows() {
        let base = scratch!("display-name");
        let tools = base.join("compatibilitytools.d");
        let folder = proton(&tools, "proton-cachyos-slr");
        compatibilitytool(
            &folder,
            "proton-cachyos-slr",
            "proton-cachyos-11.0-20261015 (steam linux runtime)",
        );

        let builds = installed_protons_in(&[tools]);

        assert_eq!(
            builds[0].name,
            "proton-cachyos-11.0-20261015 (steam linux runtime)"
        );
        assert_eq!(builds[0].folder, folder.display().to_string());
    }

    #[test]
    fn a_user_copy_of_a_tool_wins_over_the_system_one() {
        let base = scratch!("tool-order");
        let user = base.join("user/compatibilitytools.d");
        let system = base.join("system/compatibilitytools.d");
        let user_tool = proton(&user, "Proton-CachyOS Latest");
        let system_tool = proton(&system, "proton-cachyos-slr");
        compatibilitytool(&user_tool, "proton-cachyos", "Proton-CachyOS Latest");
        compatibilitytool(
            &system_tool,
            "proton-cachyos",
            "proton-cachyos-11.0-20261015",
        );

        let builds = installed_protons_in(&[user, system]);

        assert_eq!(builds.len(), 1);
        assert_eq!(builds[0].name, "Proton-CachyOS Latest");
        assert_eq!(builds[0].folder, user_tool.display().to_string());
    }

    #[test]
    fn the_system_folders_and_the_environment_follow_the_steam_folders() {
        let base = scratch!("dirs");
        let steam = base.join("steam");
        let library = base.join("library");
        let extra = base.join("extra");

        let dirs = build_dirs(
            std::slice::from_ref(&steam),
            std::slice::from_ref(&library),
            std::slice::from_ref(&extra),
        );

        assert_eq!(dirs[0], steam.join("compatibilitytools.d"));
        assert_eq!(dirs[1], library.join("steamapps/common"));
        assert!(dirs.contains(&PathBuf::from("/usr/share/steam/compatibilitytools.d")));
        assert!(dirs.contains(&PathBuf::from(
            "/usr/local/share/steam/compatibilitytools.d"
        )));
        assert_eq!(dirs.last(), Some(&extra));
    }

    #[test]
    fn steam_installs_cover_native_flatpak_and_snap() {
        let home = Path::new("/home/user");

        let installs = steam_installs(home);

        assert!(installs.contains(&home.join(".local/share/Steam")));
        assert!(installs.contains(&home.join(".steam/debian-installation")));
        assert!(installs.contains(&home.join(".var/app/com.valvesoftware.Steam/data/Steam")));
        assert!(installs.contains(&home.join("snap/steam/common/.steam/root")));
    }

    #[test]
    fn a_proton_from_the_steam_library_comes_first() {
        let base = scratch!("library-first");
        let library = base.join("library");
        let steam = base.join("steam");
        proton(&library.join("steamapps/common"), "Proton 9.0");
        proton(&steam.join("compatibilitytools.d"), "GE-Proton9-20");

        assert_eq!(
            find_proton(&steam, &library),
            Some(library.join("steamapps/common/Proton 9.0/proton"))
        );
    }

    #[test]
    fn compatibilitytools_are_the_fallback() {
        let base = scratch!("fallback");
        let library = base.join("library");
        let steam = base.join("steam");
        fs::create_dir_all(library.join("steamapps/common")).unwrap();
        proton(&steam.join("compatibilitytools.d"), "GE-Proton9-20");

        assert_eq!(
            find_proton(&steam, &library),
            Some(steam.join("compatibilitytools.d/GE-Proton9-20/proton"))
        );
    }

    #[test]
    fn folders_without_a_proton_script_are_skipped() {
        let base = scratch!("skip");
        let library = base.join("library");
        let common = library.join("steamapps/common");
        fs::create_dir_all(common.join("Proton Hotfix")).unwrap();
        proton(&common, "Proton 8.0");
        fs::create_dir_all(common.join("Example Game")).unwrap();

        assert_eq!(
            find_proton(&base.join("steam"), &library),
            Some(common.join("Proton 8.0/proton"))
        );
    }

    #[test]
    fn a_chosen_folder_must_hold_the_proton_script() {
        let base = scratch!("chosen");
        proton(&base, "Proton Custom");

        assert_eq!(
            proton_at(&base.join("Proton Custom")),
            Some(base.join("Proton Custom/proton"))
        );
        assert_eq!(proton_at(&base), None);
    }

    #[test]
    fn nothing_is_found_without_proton() {
        let base = scratch!("none");
        assert_eq!(
            find_proton(&base.join("steam"), &base.join("library")),
            None
        );
    }

    #[test]
    fn proton_named_finds_the_folder_of_the_tool_that_steam_names() {
        let base = scratch!("proton-named");
        let steam = base.join("Steam");
        let library = base.join("library");

        for folder in [
            steam.join("compatibilitytools.d/Proton-CachyOS Latest"),
            library.join("steamapps/common/Proton Hotfix"),
            library.join("steamapps/common/Proton 11.0"),
        ] {
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("proton"), "").unwrap();
        }

        let dirs = [
            steam.join("compatibilitytools.d"),
            library.join("steamapps/common"),
        ];
        // Steam writes Valve's tools in lowercase words, and the custom one as its folder name.
        let hotfix = library.join("steamapps/common/Proton Hotfix/proton");
        assert_eq!(proton_named_in(&dirs, "proton_hotfix"), Some(hotfix));
        let eleven = library.join("steamapps/common/Proton 11.0/proton");
        assert_eq!(proton_named_in(&dirs, "proton_11"), Some(eleven));
        let cachyos = steam.join("compatibilitytools.d/Proton-CachyOS Latest/proton");
        assert_eq!(
            proton_named_in(&dirs, "Proton-CachyOS Latest"),
            Some(cachyos)
        );
        assert_eq!(proton_named_in(&dirs, "proton-cachyos-slr"), None);
    }

    #[test]
    fn proton_named_uses_the_key_of_a_packaged_tool() {
        let base = scratch!("proton-key");
        let system = base.join("system");
        let folder = proton(&system, "proton-cachyos-slr");
        compatibilitytool(&folder, "proton-cachyos-slr", "proton-cachyos-20261015");

        assert_eq!(
            proton_named_in(&[system], "proton-cachyos-slr"),
            Some(folder.join("proton"))
        );
    }

    #[test]
    fn a_tool_without_its_own_vdf_uses_the_folder_name() {
        let base = scratch!("plain-tool");
        let tools = base.join("compatibilitytools.d");
        proton(&tools, "GE-Proton9-20");

        let builds = installed_protons_in(&[tools]);

        assert_eq!(builds[0].name, "GE-Proton9-20");
    }
}
