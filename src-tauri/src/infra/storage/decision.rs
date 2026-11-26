//! The first-start decision and the small JSON file that records it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::storage::StorageMode;
use crate::error::{AppError, ErrorKind, io_at};
use crate::infra::json;

use super::DECISION_VERSION;
use super::model::{PromptContext, Resolved, StorageLocation};
use super::paths::{
    can_write, env_override, exe_dir, global_decision_file, global_root, has_existing_data,
    local_decision_file, portable_marker, portable_root,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum DecisionMode {
    Portable,
    Global,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Decision {
    #[serde(default)]
    pub(super) version: u32,
    pub(super) mode: DecisionMode,
}

/// Reads the choice. A missing file is no choice; a damaged one is kept aside like every other state file.
fn read_decision(path: &Path) -> Option<DecisionMode> {
    match json::read_json::<Decision>(path) {
        Ok(Some(decision)) if decision.version <= DECISION_VERSION => Some(decision.mode),
        Ok(Some(decision)) => {
            crate::infra::log::warn(format!(
                "{} asks for storage version {}; this Gobbo understands up to {}",
                path.display(),
                decision.version,
                DECISION_VERSION
            ));
            None
        }
        Ok(None) => None,
        Err(json::ReadError::Parse(error)) => {
            json::keep_broken(path, &error);
            None
        }
        Err(json::ReadError::Io(error)) => {
            crate::infra::log::warn(format!(
                "{} could not be read ({error}); choosing the data folder again",
                path.display()
            ));
            None
        }
    }
}

/// Writes the choice through the app's atomic JSON writer.
fn write_decision(path: &Path, mode: DecisionMode) -> Result<(), AppError> {
    let decision = Decision {
        version: DECISION_VERSION,
        mode,
    };
    json::write_json(path, &decision).map_err(|error| {
        AppError::user("storage.decision_write_failed")
            .with("path", path.display())
            .with("detail", error)
    })
}

fn portable_location(exe_dir: &Path, decision_file: &Path, marker: bool) -> StorageLocation {
    StorageLocation {
        mode: StorageMode::Portable,
        root: portable_root(exe_dir),
        decision_file: decision_file.to_path_buf(),
        portable_marker: marker,
    }
}

fn global_location(global_root: &Path, decision_file: &Path, marker: bool) -> StorageLocation {
    StorageLocation {
        mode: StorageMode::Global,
        root: global_root.to_path_buf(),
        decision_file: decision_file.to_path_buf(),
        portable_marker: marker,
    }
}

fn prompt_context(
    exe_dir: &Path,
    global_root: Option<&Path>,
    marker: bool,
    local_decision_file: PathBuf,
    global_decision_file: Option<PathBuf>,
) -> PromptContext {
    let global_data_exists = global_root.is_some_and(has_existing_data);
    let portable_available = can_write(exe_dir);
    // The marker only recommends the portable folder when nothing is stored globally yet. An existing global
    // installation stays the safe default.
    let default_mode = if marker && portable_available && !global_data_exists {
        StorageMode::Portable
    } else if global_root.is_some() {
        StorageMode::Global
    } else if portable_available {
        StorageMode::Portable
    } else {
        StorageMode::Global
    };
    PromptContext {
        default_mode,
        exe_dir: exe_dir.to_path_buf(),
        portable_root: portable_root(exe_dir),
        global_root: global_root.map(Path::to_path_buf),
        global_data_exists,
        portable_available,
        portable_marker: marker,
        local_decision_file,
        global_decision_file,
    }
}

/// Resolves the data folder without writing anything. The caller shows the question when this returns
/// [`Resolved::Choice`]. `env_override` is separate from the process environment so tests can call this directly.
pub fn resolve(
    exe_dir: &Path,
    global_root: Option<&Path>,
    env_override: Option<PathBuf>,
) -> Resolved {
    let marker = portable_marker(exe_dir).is_file();

    if let Some(root) = env_override {
        return Resolved::Known(StorageLocation {
            mode: StorageMode::Custom,
            root: std::path::absolute(&root).unwrap_or(root),
            decision_file: PathBuf::new(),
            portable_marker: marker,
        });
    }

    let local_file = local_decision_file(exe_dir);

    if let Some(mode) = read_decision(&local_file) {
        match mode {
            DecisionMode::Portable => {
                return Resolved::Known(portable_location(exe_dir, &local_file, marker));
            }
            DecisionMode::Global => {
                if let Some(global) = global_root {
                    return Resolved::Known(global_location(global, &local_file, marker));
                }
                // The copy asked for the global folder, but the system has none right now. Offer the choice again.
            }
        }
    }

    if let Some(global) = global_root {
        let global_file = global_decision_file(global);

        if let Some(mode) = read_decision(&global_file) {
            if mode == DecisionMode::Global {
                return Resolved::Known(global_location(global, &global_file, marker));
            }

            crate::infra::log::warn(format!(
                "{} asks for the portable folder; only the decision next to the program can do that",
                global_file.display()
            ));
        }
    }

    Resolved::Choice(prompt_context(
        exe_dir,
        global_root,
        marker,
        local_file,
        global_root.map(global_decision_file),
    ))
}

/// Writes the chosen mode and prepares its folder. Portable requires a writable program folder; global falls back to
/// the decision file in the global data folder when the program folder is read-only.
pub fn commit(context: &PromptContext, mode: StorageMode) -> Result<StorageLocation, AppError> {
    match mode {
        StorageMode::Portable => commit_portable(context),
        StorageMode::Global => commit_global(context),
        StorageMode::Custom => Err(AppError::user("storage.custom_choice").kind(ErrorKind::Bug)),
    }
}

fn commit_portable(context: &PromptContext) -> Result<StorageLocation, AppError> {
    if !context.portable_available {
        return Err(AppError::new("storage.portable_unavailable")
            .with("path", context.portable_root.display()));
    }

    fs::create_dir_all(&context.portable_root).map_err(io_at(&context.portable_root))?;
    write_decision(&context.local_decision_file, DecisionMode::Portable)?;
    Ok(portable_location(
        &context.exe_dir,
        &context.local_decision_file,
        context.portable_marker,
    ))
}

fn commit_global(context: &PromptContext) -> Result<StorageLocation, AppError> {
    let Some(global_root) = &context.global_root else {
        return Err(AppError::user("storage.global_unavailable"));
    };
    fs::create_dir_all(global_root).map_err(io_at(global_root))?;
    // A writable program folder keeps the choice next to the program, so an installed copy does not need the
    // global decision file. A read-only one uses the file in the data folder instead.
    if write_decision(&context.local_decision_file, DecisionMode::Global).is_ok() {
        return Ok(global_location(
            global_root,
            &context.local_decision_file,
            context.portable_marker,
        ));
    }

    let Some(global_file) = &context.global_decision_file else {
        return Err(AppError::user("storage.global_unavailable"));
    };
    write_decision(global_file, DecisionMode::Global)?;
    Ok(global_location(
        global_root,
        global_file,
        context.portable_marker,
    ))
}

/// The folder the command line uses when no choice exists yet. It never asks: a marked portable copy uses its own
/// folder, everything else uses the global folder. The first window start decides and records the choice.
pub fn default_root(context: &PromptContext) -> Result<PathBuf, AppError> {
    if context.default_mode == StorageMode::Portable && context.portable_available {
        return Ok(context.portable_root.clone());
    }

    if let Some(global) = &context.global_root {
        return Ok(global.clone());
    }

    if context.portable_available {
        return Ok(context.portable_root.clone());
    }

    Err(AppError::user("settings.appdata_missing"))
}

/// The data folder for `gobbo run <game> -- <program>`.
pub fn root_for_cli() -> Result<PathBuf, AppError> {
    let exe_dir = exe_dir()?;
    let global = global_root().ok();

    match resolve(&exe_dir, global.as_deref(), env_override()) {
        Resolved::Known(location) => Ok(location.root),
        Resolved::Choice(context) => default_root(&context),
    }
}
