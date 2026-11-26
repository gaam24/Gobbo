//! Loader presets from `loaders/<id>.toml`. A preset carries the `requires` and `[deploy]` rules shared by every game
//! that uses the same loader. A game names one with `loader = "id"` or `loader = { id = "id", variant = "6" }`; the
//! preset's rules are merged under the game's own, so a game can add to or override a preset entry. The code never
//! branches on a loader id: a new loader is a new file.

mod merge;
mod preset;

use std::path::PathBuf;

#[cfg(test)]
use std::path::Path;

pub(super) use merge::{apply, merge_lists, with_requirement_paths};
pub(super) use preset::{EngineDef, LoaderDef, LoaderRules, LoaderVariant, marker_alternatives};

#[cfg(test)]
pub(super) fn load(dir: &Path, id: &str) -> Result<LoaderDef, String> {
    load_any(&[dir.to_path_buf()], id)
}

/// Reads and checks `loaders/<id>.toml` from the first folder that has it.
pub(super) fn load_any(dirs: &[PathBuf], id: &str) -> Result<LoaderDef, String> {
    if !super::is_valid_id(id) {
        return Err(format!("loader id is not a slug: {id:?}"));
    }

    let path = dirs
        .iter()
        .map(|dir| dir.join(format!("{id}.toml")))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            let first = dirs.first().map_or_else(String::new, |dir| {
                dir.join(format!("{id}.toml")).display().to_string()
            });
            format!("{first}: loader {id:?} was not found")
        })?;
    let text =
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let loader = LoaderDef::parse(&text).map_err(|error| format!("{}: {error}", path.display()))?;

    if loader.id != id {
        return Err(format!(
            "{}: id {:?} does not match the file name",
            path.display(),
            loader.id
        ));
    }

    let problems = loader.validate();

    if problems.is_empty() {
        Ok(loader)
    } else {
        Err(format!("{}: {}", path.display(), problems.join("; ")))
    }
}
