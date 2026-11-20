//! What a chosen game folder suggests: the programs, the relative folders, the store ids and an installed loader. The
//! folder is only read; the editor decides what to fill in. The loader signals are data-driven: a preset's `requires`,
//! places and root files are checked in the folder, never a loader id in this code.

use std::fs;
use std::path::{Path, PathBuf};

use super::{
    ExecutableSuggestion, FolderInspection, LoaderPresetView, LoaderSuggestion, SteamSuggestion,
    StoreIds,
};
use crate::error::{AppError, GAME_FOLDER_MISSING};

/// Reads a game folder and returns the suggestions the editor fills in. Only the folder itself and its first levels
/// are read; nothing is changed. A folder that does not exist is an error, a folder with no obvious program is not.
pub fn inspect_folder(path: &str) -> Result<FolderInspection, AppError> {
    let folder = PathBuf::from(path.trim());

    if path.trim().is_empty() {
        return Err(AppError::user(GAME_FOLDER_MISSING));
    }

    if !folder.is_dir() {
        return Err(AppError::user("games.not_a_folder").with("path", path));
    }

    let steam = steam_manifest(&folder);
    let name = steam
        .as_ref()
        .and_then(|manifest| manifest.name.clone())
        .unwrap_or_else(|| {
            folder.file_name().map_or_else(
                || path.trim().to_string(),
                |name| name.to_string_lossy().into_owned(),
            )
        });
    let executables = executables(&folder);
    let markers = executables
        .first()
        .map(|executable| vec![executable.path.clone()])
        .unwrap_or_default();
    let presets = super::loader_presets();
    Ok(FolderInspection {
        folder: path.trim().to_string(),
        suggested_id: slugify(&name),
        name,
        executables,
        directories: directories(&folder),
        markers,
        steam,
        loaders: detected_loaders(&folder, &presets),
        stores: store_ids(&folder),
    })
}

fn executables(folder: &Path) -> Vec<ExecutableSuggestion> {
    let mut found = Vec::new();
    let mut budget = 2_000usize;
    walk_executables(folder, folder, 0, &mut budget, &mut found);
    let folder_key = folder
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    found.sort_by(|left, right| {
        left.0
            .matches('/')
            .count()
            .cmp(&right.0.matches('/').count())
            .then_with(|| {
                let left_match = left.0.to_ascii_lowercase().contains(&folder_key);
                let right_match = right.0.to_ascii_lowercase().contains(&folder_key);
                right_match.cmp(&left_match)
            })
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.0.cmp(&right.0))
    });
    found
        .into_iter()
        .take(12)
        .map(|(path, _, _)| ExecutableSuggestion {
            name: path.rsplit('/').next().unwrap_or(&path).to_string(),
            path,
        })
        .collect()
}

fn walk_executables(
    root: &Path,
    dir: &Path,
    depth: usize,
    budget: &mut usize,
    out: &mut Vec<(String, u64, usize)>,
) {
    if depth > 3 || *budget == 0 {
        return;
    }

    for entry in sorted_entries(dir) {
        if *budget == 0 {
            break;
        }

        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        if metadata.file_type().is_symlink() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();

        if metadata.is_dir() {
            if SKIP_DIRS.contains(&lower.as_str()) || lower.starts_with('.') {
                continue;
            }

            walk_executables(root, &entry.path(), depth + 1, budget, out);
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        *budget = budget.saturating_sub(1);
        let is_exe = Path::new(&name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"));
        if !is_exe
            || SKIP_EXE_PREFIXES
                .iter()
                .any(|prefix| lower.starts_with(prefix))
        {
            continue;
        }

        let path = entry.path();
        let Some(relative) = path.strip_prefix(root).ok() else {
            continue;
        };
        out.push((
            relative.to_string_lossy().replace('\\', "/"),
            metadata.len(),
            depth,
        ));
    }
}

/// The entries of a folder, sorted by name. A folder that cannot be read is empty: an inspection never fails a form
/// because one protected subfolder is not readable.
fn sorted_entries(dir: &Path) -> Vec<fs::DirEntry> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    entries
}

/// The installer and runtime folders that never hold the game program.
const SKIP_DIRS: &[&str] = &[
    "redist",
    "_commonredist",
    "dotnet",
    "monobleedingedge",
    "directx",
    "vc_redist",
    "steamworks",
];

/// Installers, crash handlers and setup programs that are not the game.
const SKIP_EXE_PREFIXES: &[&str] = &[
    "unins",
    "unitycrashhandler",
    "vc_redist",
    "vcredist",
    "dxsetup",
    "dotnetfx",
    "ue4prereqsetup",
    "ue5prereqsetup",
];

fn directories(folder: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut budget = 400usize;
    walk_directories(folder, folder, 0, &mut budget, &mut out);
    out.sort();
    out.truncate(200);
    out
}

fn walk_directories(
    root: &Path,
    dir: &Path,
    depth: usize,
    budget: &mut usize,
    out: &mut Vec<String>,
) {
    if depth >= 2 || *budget == 0 {
        return;
    }

    for entry in sorted_entries(dir) {
        if *budget == 0 {
            break;
        }

        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();

        if lower.starts_with('.') || SKIP_DIRS.contains(&lower.as_str()) {
            continue;
        }

        let path = entry.path();

        if let Ok(relative) = path.strip_prefix(root) {
            *budget = budget.saturating_sub(1);
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }

        walk_directories(root, &path, depth + 1, budget, out);
    }
}

fn detected_loaders(folder: &Path, presets: &[LoaderPresetView]) -> Vec<LoaderSuggestion> {
    let mut found = Vec::new();

    for preset in presets {
        if !preset.problems.is_empty() {
            continue;
        }

        let best = if preset.variants.is_empty() {
            score(folder, &preset.requires, &preset.places, &preset.root_files)
                .map(|points| (None, points))
        } else {
            preset
                .variants
                .iter()
                .map(|variant| {
                    (
                        Some(variant.name.clone()),
                        score(
                            folder,
                            &variant.requires,
                            &variant.places,
                            &variant.root_files,
                        )
                        .unwrap_or(0),
                    )
                })
                .max_by_key(|(_, points)| *points)
        };

        if let Some((variant, points)) = best
            && points > 0
        {
            found.push(LoaderSuggestion {
                id: preset.id.clone(),
                name: preset.name.clone(),
                variant,
            });
        }
    }

    found
}

/// How much of a loader's layout is in the folder. A required file is a stronger sign than a folder, because a game
/// may ship an empty `BepInEx` folder after an uninstall.
fn score(
    folder: &Path,
    requires: &[String],
    places: &[String],
    root_files: &[String],
) -> Option<u32> {
    let mut points = 0;

    for path in requires {
        if folder.join(path).exists() {
            points += 2;
        }
    }

    for path in places {
        if folder.join(path).is_dir() {
            points += 1;
        }
    }

    for path in root_files {
        if folder.join(path).exists() {
            points += 1;
        }
    }

    (points > 0).then_some(points)
}

fn steam_manifest(folder: &Path) -> Option<SteamSuggestion> {
    let common = folder.parent()?;

    if common.file_name()? != "common" {
        return None;
    }

    let steamapps = common.parent()?;

    if steamapps.file_name()? != "steamapps" {
        return None;
    }

    let folder_name = folder.file_name()?.to_string_lossy();

    for entry in sorted_entries(steamapps) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_manifest = name.starts_with("appmanifest_")
            && Path::new(&name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("acf"));
        if !is_manifest {
            continue;
        }

        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let install_dir = super::super::steam::vdf_values(&text, "installdir")
            .into_iter()
            .next();
        if install_dir
            .as_deref()
            .is_some_and(|dir| dir.eq_ignore_ascii_case(&folder_name))
        {
            return Some(SteamSuggestion {
                app_id: super::super::steam::vdf_values(&text, "appid")
                    .first()
                    .and_then(|value| value.parse().ok())?,
                name: super::super::steam::vdf_values(&text, "name")
                    .into_iter()
                    .next(),
                install_dir,
            });
        }
    }

    None
}

fn store_ids(folder: &Path) -> StoreIds {
    let mut ids = StoreIds::default();
    let home = super::super::paths::home();
    let program_data = std::env::var_os("PROGRAMDATA").map(PathBuf::from);

    for candidate in super::super::stores::candidates(&home, program_data.as_deref()) {
        if !same_folder(&candidate.root, folder) {
            continue;
        }

        match candidate.store {
            super::super::stores::Store::Gog => ids.gog.push(candidate.id),
            super::super::stores::Store::Epic => ids.epic.push(candidate.id),
            super::super::stores::Store::Heroic => ids.heroic.push(candidate.id),
            super::super::stores::Store::Lutris => ids.lutris.push(candidate.id),
        }
    }

    ids
}

fn same_folder(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

/// A lower-case ASCII slug from a title: the form of a game id. Non-ASCII letters that have a common Latin spelling
/// are folded; other punctuation becomes one dash.
pub(super) fn slugify(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;

    for character in text.chars() {
        let lower = character.to_lowercase().next().unwrap_or(character);
        let mapped = match lower {
            'a'..='z' | '0'..='9' => Some(lower),
            'ą' | 'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => Some('a'),
            'ć' | 'ç' | 'č' => Some('c'),
            'ę' | 'é' | 'è' | 'ê' | 'ë' | 'ē' => Some('e'),
            'í' | 'ì' | 'î' | 'ï' | 'ī' => Some('i'),
            'ł' => Some('l'),
            'ń' | 'ñ' | 'ň' => Some('n'),
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' => Some('o'),
            'ś' | 'š' => Some('s'),
            'ú' | 'ù' | 'û' | 'ü' | 'ū' => Some('u'),
            'ý' | 'ÿ' => Some('y'),
            'ż' | 'ź' | 'ž' => Some('z'),
            _ => None,
        };

        if let Some(character) = mapped {
            out.push(character);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "game".to_string()
    } else {
        out
    }
}
