//! Names and folders of the library: the family id of a mod, its slug, the folders in the library and their scans.

use crate::error::{AppError, FileError, io_at};
use std::fs;
use std::io;
use std::path::Path;

use crate::domain::library::{ModInfo, ModInfoEdit, ModVersion};
use crate::domain::source::SourceRef;
use crate::games::GameDef;

/// A stable id for a mod family: the provider, mod id and part when the archive has them, otherwise the name.
pub fn family_id_from(source: Option<&SourceRef>, name: &str) -> String {
    source.map_or_else(
        || format!("local:{}", crate::domain::naming::slug(name)),
        |source| {
            source.part_id.as_ref().map_or_else(
                || format!("{}:{}", source.provider, source.mod_id),
                |part| format!("{}:{}#{}", source.provider, source.mod_id, part),
            )
        },
    )
}

/// The page id of a source, shared by every part of that page: `<provider>:<mod_id>`.
pub fn page_id_of(source: &SourceRef) -> String {
    format!("{}:{}", source.provider, source.mod_id)
}

/// Mod folders in the library: every visible folder. Hidden ones, for example `.importing`, are skipped.
pub fn mod_folders(library_dir: &Path) -> Result<Vec<String>, AppError> {
    let entries = match fs::read_dir(library_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(AppError::from(FileError::new(library_dir, e))),
    };
    let mut folders = Vec::new();

    for entry in entries {
        let entry = entry.map_err(io_at(library_dir))?;
        // The directory listing already says whether an entry is a folder, so this does not stat every mod again.
        // A symlink is followed, as `Path::is_dir` did, because a user may link a folder into the library.
        let file_type = entry.file_type().map_err(io_at(library_dir))?;
        let is_dir = file_type.is_dir() || (file_type.is_symlink() && entry.path().is_dir());
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if is_dir && !name.starts_with('.') {
            folders.push(name.into_owned());
        }
    }

    folders.sort_unstable();
    Ok(folders)
}

/// Reads one version's folder: files, size and `info.json`. It is the only place that walks the disk.
pub fn scan_mod(library_dir: &Path, folder: &str, game: &GameDef) -> Result<ModVersion, AppError> {
    let (files, size_bytes) = scan_files(library_dir, folder)?;
    Ok(version_from_import(
        &library_dir.join(folder),
        folder,
        &game.slot_for_folder(folder),
        files,
        size_bytes,
    ))
}

/// Reads the files and their sizes of one folder, without reading `info.json`. A full rescan and a deployment refresh
/// both walk the folder here; the refresh puts the result on the record the index already has.
pub fn scan_files(library_dir: &Path, folder: &str) -> Result<(Vec<String>, u64), AppError> {
    let dir = library_dir.join(folder);
    let (files, size_bytes) = crate::infra::fs::list_files_with_sizes(&dir)
        .map_err(|error| FileError::new(&dir, error))?;
    Ok((files, size_bytes))
}

/// Builds the index record of a freshly installed folder. The import already knows the files and their uncompressed
/// sizes, so it does not walk the folder again; only `info.json` is read.
pub fn version_from_import(
    dir: &Path,
    folder: &str,
    slot: &str,
    files: Vec<String>,
    size_bytes: u64,
) -> ModVersion {
    // The caller passes a sorted list: the scanner sorts while walking, and the import sorts the paths it already
    // has. Sorting here again walked the same list twice.
    debug_assert!(files.is_sorted());
    let info = mod_info(dir, folder);
    let family = family_id_from(None, &info.name);
    ModVersion {
        folder: folder.to_string(),
        family,
        slot: slot.to_string(),
        files: files.into(),
        size_bytes,
        info,
        edit: ModInfoEdit::default(),
        archive: None,
        parsed_sources: std::collections::BTreeMap::new(),
        parsed_parts: std::collections::BTreeMap::new(),
        installed_at: crate::infra::clock::now_seconds(),
        fomod: Vec::new(),
        fomod_installer: false,
        integrity: None,
    }
}

/// The info of a version folder as its `info.json` says it, without the user's edits.
pub fn mod_info(dir: &Path, folder: &str) -> ModInfo {
    let json = [dir.join("content/info.json"), dir.join("info.json")]
        .into_iter()
        .find_map(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let field = |key: &str| {
        json.as_ref()
            .and_then(|value| value.get(key))
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    ModInfo {
        name: field("modName").unwrap_or_else(|| folder.to_string()),
        version: field("version"),
        author: field("author"),
        description: field("description").map(|text| {
            text.chars()
                .take(crate::domain::DESCRIPTION_MAX)
                .collect::<String>()
        }),
    }
}
