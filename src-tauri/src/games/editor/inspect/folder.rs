//! The folder inspection and the bounded walk that feeds every suggestion.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{AppError, GAME_FOLDER_MISSING};

use super::super::FolderInspection;
use super::super::engine;
use super::super::loader_presets;
use super::executables::executables;
use super::loaders::detected_loaders;
use super::slugify;
use super::stores::{steam_manifest, store_ids};

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
    let presets = loader_presets();
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

/// How many folders the folder list keeps. It is a list of suggestions, shallow folders first.
const DIRECTORY_LIMIT: usize = 500;

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

/// A file or folder that `walk` returned.
pub(super) struct Entry {
    /// Relative to the walked folder, with `/` separators.
    pub(super) path: String,
    /// The name as the folder spells it.
    pub(super) name: String,
    pub(super) size: u64,
    /// How many folders lie between the walked folder and the folder that holds the entry: 0 for the walked folder's
    /// own entries.
    pub(super) depth: usize,
}

/// The files and folders below `folder`, `max_depth` folders deep, in breadth-first order: every folder of one depth is
/// read before any folder below it. A large folder that sorts early therefore never hides a shallow file, and the
/// budgets count folders, not files. Installer and runtime folders and hidden folders are not read, a symbolic link is
/// not followed, and a folder that cannot be read is empty. `keep` names the entries to return, by name and kind.
pub(super) fn walk(
    folder: &Path,
    max_depth: usize,
    keep: impl Fn(&str, bool) -> bool,
) -> Vec<Entry> {
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

/// The entries of a folder, sorted by name. A folder that cannot be read is empty: an inspection never fails a form
/// because one protected subfolder is not readable.
pub(in crate::games::editor) fn sorted_entries(dir: &Path) -> Vec<fs::DirEntry> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    entries
}
