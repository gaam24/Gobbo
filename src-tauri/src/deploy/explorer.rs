//! The global conflict explorer: every shared file of the active profile, its winner, the mods it overrides and the
//! size of the winning copy. It reads the same deployment plan as the status, so the total matches `conflict_count`.

use std::collections::HashMap;
use std::fs;

use serde::Serialize;

use crate::context::{Data, load_game};
use crate::deploy::target::build_plan_with_mode;
use crate::domain::pathkey::path_key;
use crate::domain::resolver::{Conflict, PlanMode};
use crate::error::AppError;
use crate::library::active_mods;

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ConflictEntry {
    /// Relative to the deployment root, with `/`.
    pub path: String,
    /// The family that wins this path.
    pub winner: String,
    /// The families whose copy is overwritten.
    pub losers: Vec<String>,
    /// The size of the winning copy in the library.
    pub bytes: u32,
    /// How many enabled mods write this path.
    pub copies: u32,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ConflictExplorer {
    pub total: u32,
    /// The paths in path order, so the window can group neighbours without sorting 10k rows itself.
    pub entries: Vec<ConflictEntry>,
}

/// One image the window can show, read from a mod in the library. A file whose format the window cannot display has no
/// `data_url`, while `extension` and `bytes` still describe it.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ConflictPreview {
    /// Lower-case extension of the file, without the dot.
    pub extension: String,
    /// Size of the file in the library.
    pub bytes: u32,
    /// A `data:` URL with the file bytes, for the formats the window can render.
    pub data_url: Option<String>,
}

/// The formats the window can render, with the MIME type of the data URL. Textures used by games are rarely among
/// them; the preview says so instead of showing a broken image.
const PREVIEW_TYPES: [(&str, &str); 5] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
];

/// The largest file read into a data URL. A preview is a thumbnail, not a way to load whole archives into the window.
const PREVIEW_MAX_BYTES: u32 = 8 * 1024 * 1024;

/// Every conflict of the active profile. A disabled mod is not part of the plan, so it never appears here.
pub fn conflicts(root: &Data, game_id: &str) -> Result<ConflictExplorer, AppError> {
    let loaded = load_game(root, game_id)?;
    let game = &loaded.game;
    let data = &loaded.data;
    let settings = &loaded.settings;
    let library = &loaded.library;
    let profile = loaded.profiles.active();
    let listed = active_mods(&loaded.profiles, library, game);
    let plan = build_plan_with_mode(
        data,
        game,
        settings,
        profile,
        &profile.winners,
        &library.inputs(&listed),
        PlanMode::Full,
    )?;

    // The winning copy's size is read from the library, not from the target, so a not-yet-deployed profile still
    // shows the size. Only the winner of a path needs a lookup. The paths are keyed the way the plan compares them,
    // because a conflict names one spelling and the winner's file may carry another.
    let conflicts: HashMap<String, &Conflict> = plan
        .conflicts
        .iter()
        .map(|conflict| (path_key(&conflict.path), conflict))
        .collect();
    let mut sizes: HashMap<&str, u32> = HashMap::new();
    let library_root = data.library();

    if !conflicts.is_empty() {
        for file in &plan.files {
            let Some(conflict) = suffix_match(&conflicts, &file.dest) else {
                continue;
            };

            if plan.family(file) != conflict.winner {
                continue;
            }

            let source = library_root
                .join(plan.folder(file))
                .join(plan.rel_path(file));
            if let Ok(meta) = fs::metadata(&source) {
                sizes.insert(
                    conflict.path.as_str(),
                    u32::try_from(meta.len()).unwrap_or(u32::MAX),
                );
            }
        }
    }

    let mut entries: Vec<ConflictEntry> = plan
        .conflicts
        .iter()
        .map(|conflict| ConflictEntry {
            path: conflict.path.clone(),
            winner: conflict.winner.clone(),
            losers: conflict.losers.clone(),
            bytes: sizes.get(conflict.path.as_str()).copied().unwrap_or(0),
            copies: u32::try_from(conflict.losers.len() + 1).unwrap_or(u32::MAX),
        })
        .collect();
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(ConflictExplorer {
        total: u32::try_from(entries.len()).unwrap_or(u32::MAX),
        entries,
    })
}

/// The conflict a planned destination belongs to. A shared file may be planned at its slot path
/// (`Mods/<name>/content/x.ws`) while the conflict names the shared path (`content/x.ws`), so the match falls back to
/// the destination suffixes; the longest suffix wins. Both sides are keyed with `path_key`, because the planned file
/// keeps the spelling of its own mod while the conflict names the spelling of another.
fn suffix_match<'a>(
    conflicts: &'a HashMap<String, &'a Conflict>,
    destination: &str,
) -> Option<&'a Conflict> {
    if let Some(conflict) = conflicts.get(&path_key(destination)) {
        return Some(conflict);
    }

    destination
        .match_indices('/')
        .find_map(|(at, _)| conflicts.get(&path_key(&destination[at + 1..])).copied())
}

/// Reads one copy of a conflicting file from the library. The path and the family come from an explorer row, so
/// neither is joined to the disk directly: both have to match a file of the current deployment plan. This keeps the
/// preview inside the library and away from arbitrary paths the window might send.
pub fn preview(
    root: &Data,
    game_id: &str,
    path: &str,
    family: &str,
) -> Result<ConflictPreview, AppError> {
    use base64::Engine;

    let loaded = load_game(root, game_id)?;
    let data = &loaded.data;
    let settings = &loaded.settings;
    let library = &loaded.library;
    let profile = loaded.profiles.active();
    let listed = active_mods(&loaded.profiles, library, &loaded.game);
    let plan = build_plan_with_mode(
        data,
        &loaded.game,
        settings,
        profile,
        &profile.winners,
        &library.inputs(&listed),
        PlanMode::Full,
    )?;
    let wanted = path_key(path);
    let conflicts: HashMap<String, &Conflict> = plan
        .conflicts
        .iter()
        .map(|conflict| (path_key(&conflict.path), conflict))
        .collect();
    let source = plan.files.iter().find_map(|file| {
        let conflict = suffix_match(&conflicts, &file.dest)?;

        if path_key(&conflict.path) != wanted || plan.family(file) != family {
            return None;
        }

        Some(
            data.library()
                .join(plan.folder(file))
                .join(plan.rel_path(file)),
        )
    });
    let Some(source) = source else {
        return Ok(ConflictPreview {
            extension: String::new(),
            bytes: 0,
            data_url: None,
        });
    };

    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let bytes =
        fs::metadata(&source).map_or(0, |meta| u32::try_from(meta.len()).unwrap_or(u32::MAX));
    let supported = PREVIEW_TYPES
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, mime)| *mime);
    let Some(mime) = supported.filter(|_| bytes <= PREVIEW_MAX_BYTES) else {
        return Ok(ConflictPreview {
            extension,
            bytes,
            data_url: None,
        });
    };
    let Ok(contents) = fs::read(&source) else {
        return Ok(ConflictPreview {
            extension,
            bytes,
            data_url: None,
        });
    };
    Ok(ConflictPreview {
        extension,
        bytes,
        data_url: Some(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(contents)
        )),
    })
}
