//! What a chosen game folder suggests: the programs, the relative folders, the store ids, an installed loader and the
//! engine. The folder is only read; the editor decides what to fill in. The loader and engine signals are data-driven: a
//! preset's `requires`, places, root files and `[engine]` markers are checked in the folder, never a loader id in this
//! code.

use std::cmp::Ordering;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use super::engine;
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
    let executables = executables(&folder, &name);
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
        engine: engine::detect(&folder, &presets),
        stores: store_ids(&folder),
    })
}

/// The most folders a walk reads. Each folder costs one listing, so this bounds the work, not the number of files.
const FOLDER_BUDGET: usize = 2_000;

/// The most entries read from one folder, in name order. A game folder never comes near it; the cap only bounds a
/// pathological folder.
const ENTRY_CAP: usize = 20_000;

/// How many folders below the game folder the program search reads: `Engine/Binaries/Win64` is three.
const PROGRAM_DEPTH: usize = 3;

/// How many folders the folder list keeps. It is a list of suggestions, shallow folders first.
const DIRECTORY_LIMIT: usize = 500;

/// The shorter of two names must have this many characters before one containing the other counts as a match. A short
/// word such as `game` is part of many names and would match too much.
const PARTIAL_NAME_MIN: usize = 6;

/// Words that mark a helper program rather than the game: a crash reporter, a checksum or verify tool, an updater,
/// patcher, setup, configuration, benchmark or editor. A helper is offered after the other programs, never hidden;
/// `check` also covers `checksum` and `checker`.
const HELPER_WORDS: &[&str] = &[
    "crash",
    "report",
    "helper",
    "check",
    "verify",
    "update",
    "patch",
    "setup",
    "config",
    "benchmark",
    "editor",
];

/// The programs of a game folder, best first. The folder name and the game's display name are the names a program may
/// match.
fn executables(folder: &Path, name: &str) -> Vec<ExecutableSuggestion> {
    let folder_name = folder
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let names = [folder_name.as_str(), name];
    let mut programs = walk(folder, PROGRAM_DEPTH, |entry, is_dir| {
        !is_dir && is_program(entry)
    });
    programs.sort_by(|left, right| program_order(left, right, &names));
    programs
        .into_iter()
        .map(|program| ExecutableSuggestion {
            name: program.name,
            path: program.path,
        })
        .collect()
}

/// A file or folder that `walk` returned.
struct Entry {
    /// Relative to the walked folder, with `/` separators.
    path: String,
    /// The name as the folder spells it.
    name: String,
    size: u64,
    /// How many folders lie between the walked folder and the folder that holds the entry: 0 for the walked folder's
    /// own entries.
    depth: usize,
}

/// The files and folders below `folder`, `max_depth` folders deep, in breadth-first order: every folder of one depth is
/// read before any folder below it. A large folder that sorts early therefore never hides a shallow file, and the
/// budgets count folders, not files. Installer and runtime folders and hidden folders are not read, a symbolic link is
/// not followed, and a folder that cannot be read is empty. `keep` names the entries to return, by name and kind.
fn walk(folder: &Path, max_depth: usize, keep: impl Fn(&str, bool) -> bool) -> Vec<Entry> {
    let mut found = Vec::new();
    let mut queue = VecDeque::from([(folder.to_path_buf(), 0usize)]);
    let mut listed = 0usize;

    while let Some((dir, depth)) = queue.pop_front() {
        if listed == FOLDER_BUDGET {
            break;
        }

        listed += 1;

        for entry in sorted_entries(&dir).into_iter().take(ENTRY_CAP) {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };

            if metadata.file_type().is_symlink() {
                continue;
            }

            let is_dir = metadata.is_dir();

            if !is_dir && !metadata.is_file() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();

            if is_dir && (lower.starts_with('.') || SKIP_DIRS.contains(&lower.as_str())) {
                continue;
            }

            let path = entry.path();

            if is_dir && depth < max_depth {
                queue.push_back((path.clone(), depth + 1));
            }

            if keep(&name, is_dir)
                && let Ok(relative) = path.strip_prefix(folder)
            {
                found.push(Entry {
                    path: relative.to_string_lossy().replace('\\', "/"),
                    name,
                    size: metadata.len(),
                    depth,
                });
            }
        }
    }

    found
}

/// Whether a file name is a program the game could start: an `.exe` that is not an installer or a crash handler.
fn is_program(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    Path::new(&lower)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        && !SKIP_EXE_PREFIXES
            .iter()
            .any(|prefix| lower.starts_with(prefix))
}

/// The order the programs are offered in, best first: a program whose name is the game's name comes before one whose
/// name contains it, and both before the rest; a helper program comes after the others; then the shallower folder, the
/// larger file and the path.
fn program_order(left: &Entry, right: &Entry, names: &[&str]) -> Ordering {
    name_rank(&left.name, names)
        .cmp(&name_rank(&right.name, names))
        .then_with(|| is_helper(&left.name).cmp(&is_helper(&right.name)))
        .then_with(|| left.depth.cmp(&right.depth))
        .then_with(|| right.size.cmp(&left.size))
        .then_with(|| left.path.cmp(&right.path))
}

/// How well a program's name matches the game's names: 0 when the name is one of them, 1 when one contains the other
/// (and the shorter has at least `PARTIAL_NAME_MIN` characters), 2 otherwise.
fn name_rank(file_name: &str, names: &[&str]) -> u8 {
    let program = comparable(file_stem(file_name));
    let mut rank = 2;

    for name in names.iter().map(|name| comparable(name)) {
        if program.is_empty() || name.is_empty() {
            continue;
        }

        if program == name {
            return 0;
        }

        // The longer name must contain the shorter one, and the shorter one must be long enough to mean something.
        let (longer, shorter) = if program.chars().count() >= name.chars().count() {
            (program.as_str(), name.as_str())
        } else {
            (name.as_str(), program.as_str())
        };

        if shorter.chars().count() >= PARTIAL_NAME_MIN && longer.contains(shorter) {
            rank = 1;
        }
    }

    rank
}

/// Whether a program is a helper, by the words in its name (see `HELPER_WORDS`).
fn is_helper(file_name: &str) -> bool {
    let key = comparable(file_stem(file_name));
    HELPER_WORDS.iter().any(|word| key.contains(word))
}

/// A file name without its last extension: `Game Setup.exe` gives `Game Setup`.
fn file_stem(file_name: &str) -> &str {
    file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem)
}

/// A name as the program search compares it: its letters and digits in lower case. Case, spaces, apostrophes and other
/// punctuation do not count, so `Example's Game` and `ExamplesGame` are the same name.
fn comparable(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The entries of a folder, sorted by name. A folder that cannot be read is empty: an inspection never fails a form
/// because one protected subfolder is not readable.
pub(super) fn sorted_entries(dir: &Path) -> Vec<fs::DirEntry> {
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

/// The folders at the first two levels, shallow first, for the pickers of the mod folders.
fn directories(folder: &Path) -> Vec<String> {
    let mut out: Vec<(usize, String)> = walk(folder, 1, |_, is_dir| is_dir)
        .into_iter()
        .map(|entry| (entry.depth, entry.path))
        .collect();
    out.sort();
    out.truncate(DIRECTORY_LIMIT);
    out.into_iter().map(|(_, path)| path).collect()
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
        let install_dir = super::super::stores::steam::vdf_values(&text, "installdir")
            .into_iter()
            .next();
        if install_dir
            .as_deref()
            .is_some_and(|dir| dir.eq_ignore_ascii_case(&folder_name))
        {
            return Some(SteamSuggestion {
                app_id: super::super::stores::steam::vdf_values(&text, "appid")
                    .first()
                    .and_then(|value| value.parse().ok())?,
                name: super::super::stores::steam::vdf_values(&text, "name")
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

    for candidate in super::super::stores::candidates(&home) {
        if !super::super::detect::same_folder(&candidate.root, folder) {
            continue;
        }

        // The id goes to the list it belongs to: a GOG game installed through Heroic or Lutris is a GOG id.
        let list = match candidate.kind {
            super::super::stores::IdKind::Gog => &mut ids.gog,
            super::super::stores::IdKind::Epic => &mut ids.epic,
            #[cfg(not(windows))]
            super::super::stores::IdKind::Lutris => &mut ids.lutris,
        };

        if !list.contains(&candidate.id) {
            list.push(candidate.id);
        }
    }

    ids
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
