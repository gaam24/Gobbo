//! Reading one game definition: id validation, loader merging, validation and the cached entry points.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{AppError, ErrorKind, io_at};

use super::cache::read_cached;
use super::definition::GameDef;
use super::dirs::{definition_path, is_valid_id, loaders_dir};
use super::layers::DefinitionLayers;
use super::validate::Section;
use super::{loaders, validate};

/// Loads the definition of one game by id, for example `the-witcher-3-wild-hunt`. Its loader preset is merged first and
/// the result is checked. This is the owned copy for a caller that changes the value; a read-only caller uses
/// `load_shared`. A change to a shipped loader needs a restart.
pub fn load(id: &str) -> Result<GameDef, AppError> {
    load_shared(id).map(|game| (*game).clone())
}

/// The definition of one game as the shared value: a repeated read of an unchanged `game.toml` returns the same `Arc`
/// instead of parsing and cloning the definition again. The hot paths read through here. A loader preset is read at the
/// same time; a change to one needs a restart, as shipped presets do not change while the app runs.
pub fn load_shared(id: &str) -> Result<Arc<GameDef>, AppError> {
    if !is_valid_id(id) {
        return Err(AppError::user("games.unknown").with("id", id));
    }

    let path = definition_path(id);
    read_cached(&path, loaders_dir())
}

/// Loads `game.toml` from every subfolder of `dir` that is not a `_` folder (the template), sorted by id. Kept for the
/// tests that check a whole folder; the app uses `definitions`, which also reads the user folder.
#[cfg(test)]
pub fn load_dir(dir: &Path) -> Result<Vec<GameDef>, AppError> {
    let entries = std::fs::read_dir(dir).map_err(io_at(dir))?;
    let mut games = Vec::new();

    for entry in entries {
        let entry = entry.map_err(io_at(dir))?;

        if entry.file_name().to_string_lossy().starts_with('_') {
            continue;
        }

        let path = entry.path().join("game.toml");

        if path.is_file() {
            match read_cached(&path, loaders_dir()) {
                Ok(game) => games.push((*game).clone()),
                Err(error) => {
                    // One bad definition must not hide every other game. The error is in the log; the catalog keeps the
                    // definitions that load, and a user definition can be fixed without a reinstall.
                    crate::infra::log::error(format!(
                        "game definition {} was skipped: {error}",
                        path.display()
                    ));
                }
            }
        }
    }

    games.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(games)
}

/// Reads one `game.toml`, merges its loader preset and checks the result. Every problem is reported in one error. The
/// loader folders are searched in user, official, bundled order, so a user preset still wins over an official one.
pub(super) fn read_definition(path: &Path, loaders_dir: &Path) -> Result<GameDef, AppError> {
    let dirs = DefinitionLayers::with_bundled_loaders(loaders_dir).loader_paths();
    read_definition_with(path, &dirs)
}

/// Reads one definition with an explicit set of loader folders. The caller owns the precedence, so a test can pass
/// scratch folders without touching the process-wide definition roots.
pub(super) fn read_definition_with(
    path: &Path,
    loader_dirs: &[PathBuf],
) -> Result<GameDef, AppError> {
    let text = std::fs::read_to_string(path).map_err(io_at(path))?;
    let mut game = GameDef::parse(&text).map_err(|e| invalid_definition(path, e))?;
    // A merge replaces an entry with the same key, so it would hide a duplicate that is already in this file.
    let duplicates = validate::duplicate_problems(&game.deploy, Section::Deploy);

    if !duplicates.is_empty() {
        let detail = duplicates
            .into_iter()
            .map(|problem| problem.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(invalid_definition(path, detail));
    }

    loaders::apply(&mut game, loader_dirs).map_err(|detail| invalid_definition(path, detail))?;
    let problems = game.validate();

    if problems.is_empty() {
        Ok(game)
    } else {
        let detail = problems
            .into_iter()
            .map(|problem| problem.to_string())
            .collect::<Vec<_>>()
            .join("; ");
        Err(invalid_definition(path, detail))
    }
}

/// The code of a game definition that is not valid. Its kind says who can fix it.
const INVALID_DEFINITION: &str = "games.invalid_definition";

/// A user's definition or draft that is not valid: the user fixes it in the editor or in the folder.
pub(super) fn invalid_user_definition(detail: impl ToString) -> AppError {
    AppError::user(INVALID_DEFINITION).with("detail", detail)
}

/// A game or loader definition that is shipped with the app is broken. This is a bug in Gobbo, not a user action.
fn invalid_definition(path: &Path, detail: impl ToString) -> AppError {
    AppError::new(INVALID_DEFINITION)
        .with("path", path.display())
        .with("detail", detail)
        .kind(ErrorKind::Bug)
}
