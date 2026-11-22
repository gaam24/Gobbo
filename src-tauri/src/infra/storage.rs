//! Where Gobbo keeps its data: next to the program or in the global app data folder. The first start asks; the answer
//! is a small JSON file. A portable copy only writes next to its own executable, so moving the folder moves the data.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, ErrorKind, io_at};

/// The app identifier from `tauri.conf.json`. The global data folder is `dirs::data_dir()` with this name.
pub const APP_ID: &str = "dev.gobbo.modsorganizer";
/// The file that records the choice, next to the program or in the global data folder.
pub const DECISION_FILE: &str = "gobbo.storage.json";
/// The marker shipped only in the portable archive. It makes the first question recommend the portable folder.
pub const PORTABLE_MARKER: &str = "gobbo.portable";
/// The data folder inside the portable folder. It is not the program folder itself: the bundled `games/` definitions
/// would collide with the per-game data folder `games/<id>`.
pub const PORTABLE_DATA_DIR: &str = "data";

/// Whether a folder name is the portable data folder. An update never reads or writes it: it holds the user's library
/// and settings, not the program files the update replaces.
pub fn is_portable_data_dir(name: &str) -> bool {
    name == PORTABLE_DATA_DIR
}

/// The version of the decision file this build understands.
pub const DECISION_VERSION: u32 = 1;

pub use crate::domain::storage::StorageMode;

/// A resolved data folder, with the file that records the choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageLocation {
    /// The mode this copy uses.
    pub mode: StorageMode,
    /// The data folder. Each game keeps its own folder below it.
    pub root: PathBuf,
    /// Empty for [`StorageMode::Custom`], which has no decision file.
    pub decision_file: PathBuf,
    /// Whether the portable archive's marker is next to the program.
    pub portable_marker: bool,
}

impl StorageLocation {
    /// The root to hand to Tauri's `appDirectoriesOverride`. The global mode keeps Tauri's own layout, so it needs no
    /// override; portable and custom put every app directory in one folder.
    pub fn tauri_root(&self) -> Option<PathBuf> {
        match self.mode {
            StorageMode::Portable | StorageMode::Custom => Some(self.root.clone()),
            StorageMode::Global => None,
        }
    }
}

/// What the start needs: a decision, or the facts for the first-start question.
#[derive(Debug, Clone)]
pub enum Resolved {
    /// A recorded choice or the environment override.
    Known(StorageLocation),
    /// No choice yet; the caller asks the first-start question.
    Choice(PromptContext),
}

/// The facts shown by the first-start question. Nothing is written until the user chooses.
#[derive(Debug, Clone)]
pub struct PromptContext {
    /// The mode the question recommends first.
    pub default_mode: StorageMode,
    /// The folder of the running program.
    pub exe_dir: PathBuf,
    /// `<exe_dir>/data`, the portable data folder.
    pub portable_root: PathBuf,
    /// The global app data folder, or `None` when the system does not provide one.
    pub global_root: Option<PathBuf>,
    /// Whether the global folder already holds an installation.
    pub global_data_exists: bool,
    /// Whether a new file can be written next to the program.
    pub portable_available: bool,
    /// Whether the portable archive's marker is next to the program.
    pub portable_marker: bool,
    /// The decision file next to the program.
    pub local_decision_file: PathBuf,
    /// The decision file in the global folder, when that folder exists.
    pub global_decision_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum DecisionMode {
    Portable,
    Global,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Decision {
    #[serde(default)]
    version: u32,
    mode: DecisionMode,
}

/// The folder of the running executable.
pub fn exe_dir() -> Result<PathBuf, AppError> {
    let exe = std::env::current_exe().map_err(AppError::unexpected)?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| AppError::unexpected("the executable has no folder"))
}

/// The global data folder, the same one Tauri's `app_data_dir` uses when no override is set.
pub fn global_root() -> Result<PathBuf, AppError> {
    dirs::data_dir()
        .map(|dir| dir.join(APP_ID))
        .ok_or_else(|| AppError::user("settings.appdata_missing"))
}

/// The portable data folder next to the executable.
pub fn portable_root(exe_dir: &Path) -> PathBuf {
    exe_dir.join(PORTABLE_DATA_DIR)
}

/// The decision file next to the executable.
pub fn local_decision_file(exe_dir: &Path) -> PathBuf {
    exe_dir.join(DECISION_FILE)
}

/// The decision file in the global data folder.
pub fn global_decision_file(global_root: &Path) -> PathBuf {
    global_root.join(DECISION_FILE)
}

/// The marker shipped in the portable archive.
pub fn portable_marker(exe_dir: &Path) -> PathBuf {
    exe_dir.join(PORTABLE_MARKER)
}

/// `GOBBO_DATA_DIR`, made absolute so the GUI and the command line agree on the same folder.
pub fn env_override() -> Option<PathBuf> {
    std::env::var_os("GOBBO_DATA_DIR")
        .filter(|dir| !dir.is_empty())
        .map(|dir| {
            let path = PathBuf::from(dir);
            std::path::absolute(&path).unwrap_or(path)
        })
}

/// Whether the global folder holds an installation from a version before the choice existed. The per-game data lives
/// under `games/`; `settings.json` and the old `games.json` are the app-level files. The global folder itself is not a
/// decision: an empty folder is not data.
pub fn has_existing_data(root: &Path) -> bool {
    ["settings.json", "games.json", "games"]
        .iter()
        .any(|name| root.join(name).exists())
}

/// Whether a new file can be created in `dir`. It writes and removes a tiny probe file, so the answer does not depend
/// on platform-specific permission APIs. The probe is only asked when the first start needs it.
pub fn can_write(dir: &Path) -> bool {
    let probe = dir.join(format!(
        ".gobbo-write-{}.tmp",
        super::store::unique_suffix()
    ));
    let Ok(file) = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    else {
        return false;
    };
    drop(file);
    let _ = fs::remove_file(&probe);
    true
}

/// Reads the choice. A missing file is no choice; a damaged one is kept aside like every other state file.
fn read_decision(path: &Path) -> Option<DecisionMode> {
    match super::json::read_json::<Decision>(path) {
        Ok(Some(decision)) if decision.version <= DECISION_VERSION => Some(decision.mode),
        Ok(Some(decision)) => {
            eprintln!(
                "gobbo: {} asks for storage version {}; this Gobbo understands up to {}",
                path.display(),
                decision.version,
                DECISION_VERSION
            );
            None
        }
        Ok(None) => None,
        Err(super::json::ReadError::Parse(error)) => {
            super::json::keep_broken(path, &error);
            None
        }
        Err(super::json::ReadError::Io(error)) => {
            eprintln!(
                "gobbo: {} could not be read ({error}); choosing the data folder again",
                path.display()
            );
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
    super::json::write_json(path, &decision).map_err(|error| {
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

            eprintln!(
                "gobbo: {} asks for the portable folder; only the decision next to the program can do that",
                global_file.display()
            );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    fn dirs(name: &str) -> (PathBuf, PathBuf) {
        let root = scratch!(name);
        let exe = root.join("program");
        let global = root.join("global");
        fs::create_dir_all(&exe).unwrap();
        (exe, global)
    }

    fn write_decision_file(path: &Path, version: u32, mode: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!(r#"{{"version":{version},"mode":"{mode}"}}"#)).unwrap();
    }

    fn known(resolved: Resolved) -> StorageLocation {
        match resolved {
            Resolved::Known(location) => location,
            Resolved::Choice(_) => panic!("expected a known location"),
        }
    }

    fn choice(resolved: Resolved) -> PromptContext {
        match resolved {
            Resolved::Choice(context) => context,
            Resolved::Known(_) => panic!("expected a question"),
        }
    }

    #[test]
    fn a_custom_folder_wins_and_has_no_decision_file() {
        let (exe, global) = dirs("storage-custom");
        let custom = exe.join("elsewhere");
        let resolved = resolve(&exe, Some(&global), Some(custom.clone()));
        let location = known(resolved);
        assert_eq!(location.mode, StorageMode::Custom);
        assert_eq!(location.root, custom);
        assert!(location.decision_file.as_os_str().is_empty());
        assert!(location.tauri_root().is_some());
    }

    #[test]
    fn a_local_portable_decision_wins() {
        let (exe, global) = dirs("storage-portable");
        write_decision_file(&local_decision_file(&exe), 1, "portable");
        let resolved = resolve(&exe, Some(&global), None);
        let location = known(resolved);
        assert_eq!(location.mode, StorageMode::Portable);
        assert_eq!(location.root, portable_root(&exe));
        assert_eq!(location.decision_file, local_decision_file(&exe));
        assert_eq!(location.tauri_root(), Some(portable_root(&exe)));
    }

    #[test]
    fn a_global_decision_in_the_data_folder_is_used() {
        let (exe, global) = dirs("storage-global-decision");
        write_decision_file(&global_decision_file(&global), 1, "global");
        let resolved = resolve(&exe, Some(&global), None);
        let location = known(resolved);
        assert_eq!(location.mode, StorageMode::Global);
        assert_eq!(location.root, global);
        assert!(location.tauri_root().is_none());
    }

    #[test]
    fn a_local_global_decision_wins_over_the_global_file() {
        let (exe, global) = dirs("storage-local-global");
        write_decision_file(&local_decision_file(&exe), 1, "global");
        write_decision_file(&global_decision_file(&global), 1, "portable");
        let resolved = resolve(&exe, Some(&global), None);
        let location = known(resolved);
        assert_eq!(location.mode, StorageMode::Global);
        assert_eq!(location.decision_file, local_decision_file(&exe));
    }

    #[test]
    fn a_portable_mode_in_the_global_file_is_ignored() {
        let (exe, global) = dirs("storage-global-portable");
        write_decision_file(&global_decision_file(&global), 1, "portable");
        assert!(matches!(
            resolve(&exe, Some(&global), None),
            Resolved::Choice(_)
        ));
    }

    #[test]
    fn a_damaged_decision_is_kept_aside_and_the_question_returns() {
        let (exe, global) = dirs("storage-broken");
        let path = local_decision_file(&exe);
        fs::write(&path, "{ not json").unwrap();
        assert!(matches!(
            resolve(&exe, Some(&global), None),
            Resolved::Choice(_)
        ));
        assert!(path.with_extension("json.broken").is_file());
    }

    #[test]
    fn a_marked_portable_copy_defaults_to_portable_without_global_data() {
        let (exe, global) = dirs("storage-default-portable");
        fs::write(portable_marker(&exe), "").unwrap();
        let context = choice(resolve(&exe, Some(&global), None));
        assert!(context.portable_marker);
        assert!(context.portable_available);
        assert!(!context.global_data_exists);
        assert_eq!(context.default_mode, StorageMode::Portable);
    }

    #[test]
    fn an_installed_copy_defaults_to_global() {
        let (exe, global) = dirs("storage-default-global");
        let context = choice(resolve(&exe, Some(&global), None));
        assert!(!context.portable_marker);
        assert_eq!(context.default_mode, StorageMode::Global);
    }

    #[test]
    fn a_games_folder_counts_as_existing_data() {
        let root = scratch!("storage-existing-games");
        fs::create_dir_all(root.join("games").join("slot")).unwrap();
        assert!(has_existing_data(&root));
    }

    #[test]
    fn existing_global_data_keeps_global_as_the_default() {
        let (exe, global) = dirs("storage-default-existing");
        fs::write(portable_marker(&exe), "").unwrap();
        fs::create_dir_all(&global).unwrap();
        fs::write(global.join("settings.json"), "{}").unwrap();
        let context = choice(resolve(&exe, Some(&global), None));
        assert!(context.global_data_exists);
        assert_eq!(context.default_mode, StorageMode::Global);
    }

    #[test]
    fn a_writable_program_folder_is_detected() {
        let (exe, _global) = dirs("storage-writable");
        assert!(can_write(&exe));
        assert!(fs::read_dir(&exe).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("gobbo-write")
        }));
    }

    #[test]
    fn a_file_cannot_be_used_as_a_program_folder() {
        let root = scratch!("storage-not-a-dir");
        let file = root.join("program");
        fs::write(&file, "x").unwrap();
        assert!(!can_write(&file));
    }

    #[test]
    fn committing_portable_creates_the_data_folder_and_the_local_decision() {
        let (exe, global) = dirs("storage-commit-portable");
        let context = choice(resolve(&exe, Some(&global), None));
        let location = commit(&context, StorageMode::Portable).unwrap();
        assert_eq!(location.mode, StorageMode::Portable);
        assert!(portable_root(&exe).is_dir());
        assert!(local_decision_file(&exe).is_file());
        let saved: Decision =
            serde_json::from_str(&fs::read_to_string(local_decision_file(&exe)).unwrap()).unwrap();
        assert_eq!(saved.version, 1);
        assert_eq!(saved.mode, DecisionMode::Portable);
    }

    #[test]
    fn committing_global_prefers_the_local_decision() {
        let (exe, global) = dirs("storage-commit-global");
        let context = choice(resolve(&exe, Some(&global), None));
        let location = commit(&context, StorageMode::Global).unwrap();
        assert_eq!(location.mode, StorageMode::Global);
        assert_eq!(location.root, global);
        assert_eq!(location.decision_file, local_decision_file(&exe));
        assert!(global.is_dir());
        assert!(local_decision_file(&exe).is_file());
    }

    #[test]
    fn committing_global_falls_back_to_the_global_decision() {
        let (exe, global) = dirs("storage-commit-global-fallback");
        let mut context = choice(resolve(&exe, Some(&global), None));
        // Make the local write fail by putting a file where its parent folder should be.
        context.local_decision_file = exe.join("blocked").join(DECISION_FILE);
        fs::write(exe.join("blocked"), "x").unwrap();
        let location = commit(&context, StorageMode::Global).unwrap();
        assert_eq!(location.decision_file, global_decision_file(&global));
        assert!(global_decision_file(&global).is_file());
    }

    #[test]
    fn committing_portable_refuses_a_read_only_folder() {
        let (exe, global) = dirs("storage-commit-read-only");
        let mut context = choice(resolve(&exe, Some(&global), None));
        context.portable_available = false;
        assert_eq!(
            commit(&context, StorageMode::Portable).unwrap_err().code,
            "storage.portable_unavailable"
        );
    }

    #[test]
    fn the_cli_default_matches_the_first_question() {
        let (exe, global) = dirs("storage-cli-default");
        fs::write(portable_marker(&exe), "").unwrap();
        let context = choice(resolve(&exe, Some(&global), None));
        assert_eq!(default_root(&context).unwrap(), portable_root(&exe));
        // The default does not write a decision; the first window start still asks.
        assert!(!local_decision_file(&exe).exists());
    }
}
