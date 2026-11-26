//! The caches shared by a data handle and its game handles, and the reads and writes that use them.

use super::Data;
use crate::context::library::{self, FolderCheck, Library};
use crate::context::profile::history::Index;
use crate::context::settings::Settings;
use crate::domain::profiles::Profiles;
use crate::error::AppError;
use crate::infra::file_cache::FileCache;
use crate::infra::memo::Memo;
use crate::infra::store;
use std::fs;
use std::path::Path;
use std::sync::Arc;

/// The files a status reads on every call, each parsed again only when it changed on disk.
#[derive(Default)]
pub(super) struct Caches {
    /// `library.json`: parsing a large index is the largest single cost of a status.
    library: FileCache<Library>,
    /// The library folder listing, so a status does not list thousands of names again.
    folders: FileCache<Vec<String>>,
    /// `profiles.json`: parsing it was about a twelfth of every status.
    profiles: FileCache<Profiles>,
    /// `config.json`: the settings are read by every status and every launch path.
    settings: FileCache<Settings>,
    /// `history/index.json`: the status asks whether the active profile can undo or redo on every call.
    history: FileCache<Index>,
    /// The launch report's read-modify-write. The wait thread and Stop can finish in either order; one lock keeps the
    /// stop flag and the exit code from overwriting each other.
    report: std::sync::Mutex<()>,
    /// The profiles and the library they were last brought in step with. A status that sees the same pair again skips
    /// the copy and the sync; both caches give a new `Arc` when the file behind them changed.
    profiles_in_step: Memo<Profiles, Library, ()>,
    /// How the index and the library folder disagree, while both are the ones in the caches above.
    folder_check: Memo<Library, Vec<String>, FolderCheck>,
}

/// The value at `path` through `cache`, or parsed directly without one.
pub(super) fn cached<T>(
    cache: Option<&FileCache<T>>,
    path: &Path,
    parse: impl FnOnce() -> Result<T, AppError>,
) -> Result<Arc<T>, AppError> {
    match cache {
        Some(cache) => cache.load(path, parse),
        None => parse().map(Arc::new),
    }
}

impl Data {
    /// The library index cache, when this handle keeps the app's caches.
    pub(super) fn library_cache(&self) -> Option<&FileCache<Library>> {
        self.caches.as_deref().map(|caches| &caches.library)
    }

    /// The profile list cache, when this handle keeps the app's caches.
    pub(super) fn profiles_cache(&self) -> Option<&FileCache<Profiles>> {
        self.caches.as_deref().map(|caches| &caches.profiles)
    }

    /// The settings file cache, when this handle keeps the app's caches.
    pub(crate) fn settings_cache(&self) -> Option<&FileCache<Settings>> {
        self.caches.as_deref().map(|caches| &caches.settings)
    }

    /// The history index cache, when this handle keeps the app's caches.
    pub(crate) fn history_cache(&self) -> Option<&FileCache<Index>> {
        self.caches.as_deref().map(|caches| &caches.history)
    }

    /// The launch report lock, when this handle keeps the app's caches.
    pub(crate) fn report_lock(&self) -> Option<&std::sync::Mutex<()>> {
        self.caches.as_deref().map(|caches| &caches.report)
    }

    /// Keeps the history index just written, so the next status does not parse it again.
    pub(crate) fn remember_history(&self, path: &Path, index: &Index) {
        if let Some(caches) = &self.caches {
            caches.history.store(path, Arc::new(index.clone()));
        }
    }

    /// Keeps the settings just written, so the next read does not parse the file again.
    pub(crate) fn remember_settings(&self, settings: &Settings) {
        if let Some(caches) = &self.caches {
            caches
                .settings
                .store(&self.config(), Arc::new(settings.clone()));
        }
    }

    /// The visible folders in `library/`, from the read-through cache when the folder did not change.
    pub fn library_folders(&self) -> Result<Arc<Vec<String>>, AppError> {
        let dir = self.library();
        let cache = self.caches.as_deref().map(|caches| &caches.folders);
        cached(cache, &dir, || library::mod_folders(&dir))
    }

    /// How `library` and the folder listing `folders` disagree. With the app's caches the answer is kept while both are
    /// the same, so a status after a list edit does not compare thousands of folders again.
    pub fn folder_check(
        &self,
        library: &Arc<Library>,
        folders: &Arc<Vec<String>>,
    ) -> Arc<FolderCheck> {
        let check = || library.check_folders(folders);
        self.caches.as_ref().map_or_else(
            || Arc::new(check()),
            |caches| caches.folder_check.get(library, folders, check),
        )
    }

    /// Whether `profiles` and `library` are the pair a status last brought in step. Both caches hand out a new `Arc`
    /// when the file behind them changed, so identity is enough and a stale pair never matches.
    pub fn profiles_in_step(&self, profiles: &Arc<Profiles>, library: &Arc<Library>) -> bool {
        self.caches
            .as_ref()
            .is_some_and(|caches| caches.profiles_in_step.matches(profiles, library))
    }

    /// Remembers that `profiles` and `library` are in step, so the next status read does not copy the list and sync it
    /// again. Without the app's caches there is nothing to keep.
    pub fn remember_profiles_in_step(&self, profiles: &Arc<Profiles>, library: &Arc<Library>) {
        if let Some(caches) = &self.caches {
            caches.profiles_in_step.get(profiles, library, || ());
        }
    }

    /// Removes the temporary files that an interrupted save left next to the JSON files. The library folder and the
    /// archive copies are not walked: they hold millions of files and none of them is written atomically.
    pub fn clean_stale_temps(&self) {
        store::remove_temp_files(&self.dir);
        let games = self.dir.join(crate::migrations::GAMES_DIR);
        let Ok(entries) = fs::read_dir(&games) else {
            return;
        };

        for entry in entries.flatten() {
            let game = entry.path();

            if !game.is_dir() {
                continue;
            }

            store::remove_temp_files(&game);

            for folder in ["history", "state"] {
                let dir = game.join(folder);
                store::remove_temp_files(&dir);

                if folder == "state"
                    && let Ok(states) = fs::read_dir(&dir)
                {
                    for state in states.flatten() {
                        if state.path().is_dir() {
                            store::remove_temp_files(&state.path());
                        }
                    }
                }
            }
        }
    }
}
