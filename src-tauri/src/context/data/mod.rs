//! The game as the features read it: the data folder of each game and the loads that every view starts with.

use super::library::{self, FolderCheck, Library};
use super::profile::history::Index;
use super::settings::{CONFIG_FILE, Settings};
use crate::domain::profiles::{self, Profiles};
use crate::error::AppError;
use crate::games::{self, GameDef};
use crate::infra::deployment;
use crate::infra::file_cache::FileCache;
use crate::infra::memo::Memo;
use crate::infra::store;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod edit;
mod load;

pub use edit::{edit_family, edit_library, edit_library_clean, edit_profiles, idle_data};
pub use load::{
    Loaded, load_game, load_library, load_library_shared, load_profiles, load_profiles_shared,
    load_stopped, open,
};

/// The journal of a deployment, in the state folder of a profile. It records what to undo; it does not by itself mean
/// the deployment finished. The completion marker below is the deployed flag.
pub const JOURNAL_FILE: &str = "journal.jsonl";
/// Written after the files and the settings are in place. A journal without this marker is an interrupted apply, not a
/// deployment the game may start from.
pub const DEPLOYMENT_FILE: &str = "deployment.json";

/// The Gobbo data folder. Each game has its own subfolder in it.
#[derive(Clone)]
pub struct Data {
    pub dir: PathBuf,
    /// The app data root this handle was opened from. A game handle keeps it so paths stored at the root, such as the
    /// Proton prefix Gobbo owns, resolve the same way from a game handle and from the root handle.
    root: Option<PathBuf>,
    /// The app's read-through caches; a test or a direct `Data::new` has none. Every game handle shares them.
    caches: Option<Arc<Caches>>,
}

/// The two deployment states of one game, from one scan of the state folder.
#[derive(Debug, Default)]
pub struct DeploymentStates {
    pub prepared: BTreeMap<String, PathBuf>,
    pub incomplete: BTreeMap<String, PathBuf>,
}

/// Whether `state` holds a completion marker this build can trust, and so an apply that reached its end. A missing,
/// empty, damaged or newer marker is not trusted; the journal alone is an interrupted apply.
fn marker_applied(state: &Path) -> bool {
    deployment::read_complete(&state.join(DEPLOYMENT_FILE)).is_some()
}

/// The files a status reads on every call, each parsed again only when it changed on disk.
#[derive(Default)]
struct Caches {
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
    /// A data folder handle with no read-through caches, for a test or a direct caller.
    pub const fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            root: None,
            caches: None,
        }
    }

    /// The app's data folder. Reading the library reuses the parsed index until the file changes.
    pub fn with_cache(dir: PathBuf) -> Self {
        Self {
            caches: Some(Arc::default()),
            ..Self::new(dir)
        }
    }

    /// The app data root, whether this is the root handle or one of its game handles.
    pub fn root(&self) -> &Path {
        self.root.as_deref().unwrap_or(&self.dir)
    }

    fn library_cache(&self) -> Option<&FileCache<Library>> {
        self.caches.as_deref().map(|caches| &caches.library)
    }

    fn profiles_cache(&self) -> Option<&FileCache<Profiles>> {
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

    /// The definition of game `id`, read and checked again only when its `game.toml` changed. A loader preset is read
    /// at the same time; a change to a preset needs a restart, as shipped presets do not change while the app runs. The
    /// value is owned, for a caller that changes it; a read-only caller uses `definition_shared`.
    pub fn definition(&self, id: &str) -> Result<GameDef, AppError> {
        self.definition_shared(id).map(|game| (*game).clone())
    }

    /// The definition of game `id` as the shared handle: the process cache gives every reader the same value until
    /// `game.toml` changes.
    #[expect(
        clippy::unused_self,
        reason = "the definition cache is process-wide, not per handle, but the handle is the API the callers use"
    )]
    pub fn definition_shared(&self, id: &str) -> Result<Arc<GameDef>, AppError> {
        games::load_shared(id)
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

    /// The data folder as text, for the settings page: the app settings and every game's folder live in it.
    pub fn folder_text(&self) -> String {
        self.dir.display().to_string()
    }

    /// The data folder of one game: library, archives, profiles, deployment state and settings. The index cache is
    /// shared with the handle it came from, so every game uses the same one-entry cache.
    pub fn game(&self, id: &str) -> Self {
        Self {
            dir: self.dir.join(crate::migrations::GAMES_DIR).join(id),
            root: Some(self.root().to_path_buf()),
            caches: self.caches.clone(),
        }
    }

    /// The library folder, which holds the installed mod folders.
    pub fn library(&self) -> PathBuf {
        self.dir.join("library")
    }

    /// The visible folders in `library/`, from the read-through cache when the folder did not change.
    pub fn library_folders(&self) -> Result<Arc<Vec<String>>, AppError> {
        let dir = self.library();
        let cache = self.caches.as_deref().map(|caches| &caches.folders);
        cached(cache, &dir, || library::mod_folders(&dir))
    }

    /// The archive copies of the installed versions.
    pub fn archives(&self) -> PathBuf {
        self.dir.join(store::ARCHIVES_DIR)
    }

    /// The downloads waiting for their import, next to the game's data.
    pub fn downloads(&self) -> PathBuf {
        self.dir.join("downloads")
    }

    /// The cached collection recipes and their archives.
    pub fn collections(&self) -> PathBuf {
        self.dir.join("collections")
    }

    /// The cached pictures of mod pages.
    pub fn images(&self) -> PathBuf {
        self.dir.join("images")
    }

    /// The library index file, `library.json`.
    pub fn index(&self) -> PathBuf {
        self.dir.join(library::INDEX_FILE)
    }

    /// The profile list file, `profiles.json`.
    pub fn profiles_file(&self) -> PathBuf {
        self.dir.join(profiles::PROFILES_FILE)
    }

    /// The deployment state of one profile: the journal and the overlay layer. Each profile has its own folder, so
    /// switching profiles does not delete another profile's layer.
    pub fn profile_state(&self, profile: &str) -> PathBuf {
        self.dir.join("state").join(profile)
    }

    /// Every profile with a prepared deployment, by profile id. Only a finished apply counts: the journal alone is an
    /// interrupted apply, not a deployment the game may use.
    pub fn prepared_states(&self) -> BTreeMap<String, PathBuf> {
        self.deployment_states().prepared
    }

    /// Both state maps from one directory scan. A caller that needs both must not read the state folder twice: a
    /// deployment could finish between the scans and be missed.
    pub fn deployment_states(&self) -> DeploymentStates {
        let mut states = DeploymentStates::default();
        let Ok(entries) = fs::read_dir(self.dir.join("state")) else {
            return states;
        };

        for entry in entries.filter_map(std::result::Result::ok) {
            let path = entry.path();

            if !path.is_dir() || !path.join(JOURNAL_FILE).is_file() {
                continue;
            }

            let id = entry.file_name().to_string_lossy().into_owned();

            if marker_applied(&path) {
                states.prepared.insert(id, path);
            } else {
                states.incomplete.insert(id, path);
            }
        }

        states
    }

    /// The state of the active profile, if it is prepared. A marker this build cannot trust is not prepared.
    pub fn deployed_state(&self) -> Option<PathBuf> {
        let state = self.profile_state(&self.active_profile_id());
        marker_applied(&state).then_some(state)
    }

    /// The state of the active profile when its journal exists but its completion marker does not, or the marker is
    /// empty, damaged or from a newer build. This is an apply that stopped in the middle; it may be undone, but it is
    /// never reported as applied.
    pub fn incomplete_state(&self) -> Option<PathBuf> {
        let state = self.profile_state(&self.active_profile_id());
        (state.join(JOURNAL_FILE).is_file() && !marker_applied(&state)).then_some(state)
    }

    /// Id of the profile that is active in `profiles.json`, from the shared cache when the file did not change.
    pub fn active_profile_id(&self) -> String {
        load_profiles_shared(self)
            .map_or_else(|_| "p1".to_string(), |profiles| profiles.active.clone())
    }

    /// The game's settings file, `config.json`.
    pub fn config(&self) -> PathBuf {
        self.dir.join(CONFIG_FILE)
    }

    /// Saves `profiles.json` and keeps the read-through cache warm with what was written. Every save goes through here:
    /// the cache knows a file by its length and modification time, and two quick saves of the same length could share
    /// both, so the value written must replace the cached one.
    pub fn save_profiles(&self, profiles: &Profiles) -> Result<(), AppError> {
        let path = self.profiles_file();
        profiles.save(&path)?;

        if let Some(caches) = &self.caches {
            caches.profiles.store(&path, Arc::new(profiles.clone()));
        }

        Ok(())
    }

    /// Saves the index and keeps the read-through cache warm, so the status after an import, an edit or a deployment
    /// does not parse the file that was just written. The library moves into the cache instead of being cloned.
    pub fn save_library(&self, mut library: Library) -> Result<Arc<Library>, AppError> {
        let index = self.index();
        library.prune_layouts();
        library.save(&index)?;
        let library = Arc::new(library);

        if let Some(cache) = self.library_cache() {
            cache.store(&index, Arc::clone(&library));
        }

        Ok(library)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_game_handle_shares_one_index_cache() {
        let root = Data::with_cache(PathBuf::from("data"));
        let game = root.game("slot");
        assert!(Arc::ptr_eq(
            root.caches.as_ref().expect("the root cache"),
            game.caches.as_ref().expect("the game cache")
        ));
    }

    #[test]
    fn an_owned_definition_is_a_copy_of_the_shared_value() {
        use crate::test_support::games as test_games;

        test_games::install();
        let data = Data::with_cache(PathBuf::from("data"));
        let mut owned = data.definition(test_games::SLOT_ID).unwrap();
        owned.name = "Changed Game".into();
        let shared = data.definition_shared(test_games::SLOT_ID).unwrap();
        assert_eq!(owned.name, "Changed Game");
        assert_ne!(shared.name, "Changed Game");
        assert_eq!(shared.id, test_games::SLOT_ID);
    }

    #[test]
    fn a_saved_index_is_read_again_but_an_unchanged_one_is_reused() {
        use crate::test_support::games as test_games;
        use crate::test_support::scratch;

        let base = scratch!("data-cache-load");
        let root = Data::with_cache(base);
        let data = root.game(test_games::SLOT_ID);
        let game = test_games::slot();
        let folder = data.library().join("modA");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("content.esp"), "x").unwrap();
        let mut library = Library::default();
        library.versions.insert(
            "modA".into(),
            library::scan_mod(&data.library(), "modA", &game).unwrap(),
        );
        library.save(&data.index()).unwrap();

        let first = load_library_shared(&data, &game).unwrap();
        let second = load_library_shared(&data, &game).unwrap();
        assert!(Arc::ptr_eq(&first, &second));

        library.versions.clear();
        library.save(&data.index()).unwrap();
        let third = load_library_shared(&data, &game).unwrap();
        assert!(!Arc::ptr_eq(&first, &third));
        assert!(third.versions.is_empty());
    }

    #[test]
    fn a_shared_profiles_read_reuses_the_value_and_sees_a_save() {
        use crate::test_support::games as test_games;
        use crate::test_support::scratch;

        let base = scratch!("data-profiles-shared");
        let root = Data::with_cache(base);
        let data = root.game(test_games::SLOT_ID);
        Profiles::with_default(Vec::new())
            .save(&data.profiles_file())
            .unwrap();

        let first = load_profiles_shared(&data).unwrap();
        let second = load_profiles_shared(&data).unwrap();
        assert!(Arc::ptr_eq(&first, &second));

        let mut changed = (*first).clone();
        changed.profiles[0].name = "Changed".into();
        data.save_profiles(&changed).unwrap();
        let third = load_profiles_shared(&data).unwrap();
        assert!(
            !Arc::ptr_eq(&first, &third),
            "a save replaces the cached value"
        );
        assert_eq!(third.profiles[0].name, "Changed");
    }

    #[test]
    fn a_profiles_and_library_pair_is_remembered_in_step_until_either_changes() {
        use crate::test_support::games as test_games;
        use crate::test_support::scratch;

        let base = scratch!("data-profiles-in-step");
        let root = Data::with_cache(base);
        let data = root.game(test_games::SLOT_ID);
        let game = test_games::slot();
        Library::default().save(&data.index()).unwrap();
        Profiles::with_default(Vec::new())
            .save(&data.profiles_file())
            .unwrap();

        let library = load_library_shared(&data, &game).unwrap();
        let profiles = load_profiles_shared(&data).unwrap();
        assert!(!data.profiles_in_step(&profiles, &library));
        data.remember_profiles_in_step(&profiles, &library);
        assert!(data.profiles_in_step(&profiles, &library));

        // A save is a new library value, so the pair the next status reads is a new one.
        let library = data.save_library(Library::default()).unwrap();
        assert!(!data.profiles_in_step(&profiles, &library));
    }

    /// A game's data with a journal for profile `p1` and `marker` as its completion marker.
    fn state_with_marker(name: &str, marker: &[u8]) -> (Data, PathBuf) {
        use crate::test_support::scratch;

        let data = Data::new(scratch!(name));
        let state = data.profile_state("p1");
        fs::create_dir_all(&state).unwrap();
        fs::write(state.join(JOURNAL_FILE), b"").unwrap();
        fs::write(state.join(DEPLOYMENT_FILE), marker).unwrap();
        (data, state)
    }

    #[test]
    fn a_marker_this_build_cannot_trust_is_an_interrupted_apply() {
        let cases: [(&str, &[u8]); 4] = [
            ("data-marker-empty", b""),
            ("data-marker-corrupt", b"{ not json"),
            ("data-marker-no-fields", b"{}"),
            (
                "data-marker-newer",
                br#"{"schema_version":99,"method":"hardlink","files":1,"plan_hash":"abc"}"#,
            ),
        ];

        for (name, marker) in cases {
            let (data, state) = state_with_marker(name, marker);

            assert!(data.prepared_states().is_empty(), "{name}");
            assert_eq!(
                data.deployment_states().incomplete.get("p1"),
                Some(&state),
                "{name}"
            );
            assert!(data.deployed_state().is_none(), "{name}");
            assert_eq!(data.incomplete_state(), Some(state.clone()), "{name}");
            assert!(
                state.join(DEPLOYMENT_FILE).is_file(),
                "the untrusted marker is kept for diagnosis: {name}"
            );
        }
    }

    #[test]
    fn a_marker_of_this_build_or_an_older_one_is_applied() {
        let cases: [(&str, &[u8]); 2] = [
            (
                "data-marker-current",
                br#"{"schema_version":1,"method":"hardlink","files":1,"plan_hash":"abc"}"#,
            ),
            (
                "data-marker-old",
                br#"{"method":"hardlink","files":1,"plan_hash":"abc"}"#,
            ),
        ];

        for (name, marker) in cases {
            let (data, state) = state_with_marker(name, marker);

            assert_eq!(data.prepared_states().get("p1"), Some(&state), "{name}");
            assert!(data.deployment_states().incomplete.is_empty(), "{name}");
            assert_eq!(data.deployed_state(), Some(state), "{name}");
            assert!(data.incomplete_state().is_none(), "{name}");
        }
    }
}
