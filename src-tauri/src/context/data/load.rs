use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::domain::profiles::Profiles;
use crate::error::AppError;
use crate::games::GameDef;

use crate::context::library::{ArchiveLabels, Library, lock_index};
use crate::context::settings::{Settings, load_settings};

use super::{Data, cached};

/// The number of tries a startup migration gets when another writer holds the state lock.
const MIGRATE_ATTEMPTS: u32 = 20;
/// The pause between two tries: a short window a second process holds, not a long apply.
const MIGRATE_RETRY_DELAY: Duration = Duration::from_millis(100);

/// Opens the data folder for this process: the shared caches, the file-layout migrations, the archive names of the
/// libraries and the stale temp files. `archive_names` is the providers' reader (`providers::archive_name_from`).
pub fn open(dir: PathBuf, archive_names: ArchiveLabels<'_>) -> Result<Data, AppError> {
    open_with_budget(dir, archive_names, MIGRATE_ATTEMPTS, MIGRATE_RETRY_DELAY)
}

/// `open` with the retry budget passed in, so a test can walk the busy paths quickly.
fn open_with_budget(
    dir: PathBuf,
    archive_names: ArchiveLabels<'_>,
    attempts: u32,
    delay: Duration,
) -> Result<Data, AppError> {
    let data = Data::with_cache(dir);
    let migration = migrate_with_retries(&data, attempts, delay);
    // The caller builds a fallback Data from the error, so it must already be labelled and cleaned on both paths.
    name_archives(&data, archive_names);
    data.clean_stale_temps();
    migration?;
    Ok(data)
}

/// Retries only `state.busy`, up to `attempts` tries. Logs once that a writer was in the way and once if it still
/// fails, rather than one line per retry, and returns the migration's own error unchanged.
fn migrate_with_retries(data: &Data, attempts: u32, delay: Duration) -> Result<(), AppError> {
    let mut tried = 0;
    loop {
        match crate::context::managed::migrate(data) {
            Ok(()) => return Ok(()),
            Err(error) if error.code == "state.busy" && tried + 1 < attempts => {
                if tried == 0 {
                    crate::infra::log::info("the data folders are busy; retrying the migration");
                }
                tried += 1;
                std::thread::sleep(delay);
            }
            Err(error) => {
                if tried > 0 {
                    crate::infra::log::warn(format!(
                        "the data folders stayed busy after {attempts} tries: {error}"
                    ));
                }
                return Err(error);
            }
        }
    }
}

/// Names the archives of every game's library that have no label yet. It runs on every start, like the file-layout
/// migrations: an archive that already has its label is skipped, and a library is saved only when a label was added.
/// A library that cannot be read is logged and skipped, so it never stops the app from starting.
fn name_archives(root: &Data, archive_names: ArchiveLabels<'_>) {
    let ids = match crate::context::managed::managed_ids(root) {
        Ok(ids) => ids,
        Err(error) => {
            crate::infra::log::warn(format!("archive names: {error}"));
            return;
        }
    };

    for id in ids {
        let named = root
            .definition(&id)
            .and_then(|game| name_game_archives(&root.game(&id), &game, archive_names));
        if let Err(error) = named {
            crate::infra::log::warn(format!("archive names of {id}: {error}"));
        }
    }
}

/// Names the archives of one game's library that have no label, and saves the index only when a label was added.
/// Returns whether it saved. A game without an index has no archives to name.
pub fn name_game_archives(
    data: &Data,
    game: &GameDef,
    archive_names: ArchiveLabels<'_>,
) -> Result<bool, AppError> {
    if !data.index().is_file() {
        return Ok(false);
    }

    let _index = lock_index();
    let mut library = load_library(data, game)?;

    if !library.name_archives(archive_names) {
        return Ok(false);
    }

    data.save_library(library)?;
    Ok(true)
}

/// A game as the service reads it: its definition, its settings, its library and its profiles.
pub struct Loaded {
    pub game: GameDef,
    pub data: Data,
    pub settings: Settings,
    /// Read-only for these views; the app shares the parsed index between commands until the file changes.
    pub library: Arc<Library>,
    pub profiles: Profiles,
}

/// Reads what most views need about a game: its definition, its settings, its library and its profiles.
pub fn load_game(root: &Data, game_id: &str) -> Result<Loaded, AppError> {
    let game = root.definition(game_id)?;
    let data = root.game(game_id);
    let settings = load_settings(&data);
    let library = load_library_shared(&data, &game)?;
    let profiles = load_profiles(&data)?;
    Ok(Loaded {
        game,
        data,
        settings,
        library,
        profiles,
    })
}

/// The game, its data and its settings, when the game is not running. Its files are not touched while it runs.
pub fn load_stopped(root: &Data, game_id: &str) -> Result<(GameDef, Data, Settings), AppError> {
    let game = root.definition(game_id)?;
    let data = root.game(game_id);

    if crate::context::running::game_running(root, game_id)? {
        return Err(crate::error::game_running());
    }

    let settings = load_settings(&data);
    Ok((game, data, settings))
}

/// The library index for a caller that may change it. The read-through cache is used when it still matches, so an edit
/// after a status does not parse the whole file again; the caller saves through `Data::save_library`, which puts the
/// changed value back. Callers hold `lock_index` while they change the index.
pub fn load_library(data: &Data, game: &GameDef) -> Result<Library, AppError> {
    let index = data.index();
    let parse = || Library::load(&index, &data.library(), game);
    data.library_cache()
        .map_or_else(parse, |cache| cache.load_cloned(&index, parse))
}

/// The library index for read-only views. With the app's `Data`, an unchanged index comes from the cache.
pub fn load_library_shared(data: &Data, game: &GameDef) -> Result<Arc<Library>, AppError> {
    let index = data.index();
    cached(data.library_cache(), &index, || {
        Library::load(&index, &data.library(), game)
    })
}

/// The game's profiles for a caller that will change them. A missing file starts the game with one empty profile.
pub fn load_profiles(data: &Data) -> Result<Profiles, AppError> {
    let path = data.profiles_file();
    let parse = || Profiles::read_or_default(&path);
    data.profiles_cache()
        .map_or_else(parse, |cache| cache.load_cloned(&path, parse))
}

/// The game's profiles for read-only views. With the app's `Data`, an unchanged file comes from the cache and the
/// parsed list is shared instead of copied.
pub fn load_profiles_shared(data: &Data) -> Result<Arc<Profiles>, AppError> {
    let path = data.profiles_file();
    cached(data.profiles_cache(), &path, || {
        Profiles::read_or_default(&path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::library::scan_mod;
    use crate::domain::library::{Archive, ArchiveLabel};
    use crate::test_support::{games as test_games, scratch};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::SystemTime;

    #[test]
    fn a_library_without_labels_is_named_once_and_saved_with_them() {
        let base = scratch!("archive-labels-backfill");
        let root = Data::with_cache(base);
        let data = root.game(test_games::SLOT_ID);
        let game = test_games::slot();
        let folder = data.library().join("modA");
        fs::create_dir_all(folder.join("content")).unwrap();
        fs::write(folder.join("content/x.ws"), "a").unwrap();

        // An index as an older Gobbo wrote it: the archive has no label, so the file has no `named` key.
        let mut library = Library::default();
        let mut version = scan_mod(&data.library(), "modA", &game).unwrap();
        version.archive = Some("sha-a".into());
        version.parsed_sources.insert(
            crate::domain::source::ProviderId::new("nexus"),
            "12345".into(),
        );
        library.versions.insert("modA".into(), version);
        library.archives.insert(
            "sha-a".into(),
            Archive {
                original_name: "Example Mod 12345 1.2.0 2026-01-01T00-00Z example-token.zip".into(),
                file: "sha-a.zip".into(),
                named: None,
                source_files: BTreeMap::new(),
                source_parts: BTreeMap::new(),
                skipped: Vec::new(),
                skipped_total: 0,
            },
        );
        library.save(&data.index()).unwrap();
        assert!(!fs::read_to_string(data.index()).unwrap().contains("named"));

        let named = name_game_archives(&data, &game, &crate::providers::archive_name_from).unwrap();
        assert!(named, "the first start names the archive and saves");
        assert!(
            fs::read_to_string(data.index())
                .unwrap()
                .contains("\"named\"")
        );

        let loaded = Library::load(&data.index(), &data.library(), &game).unwrap();
        assert_eq!(
            loaded.archives["sha-a"].named,
            Some(ArchiveLabel {
                name: Some("Example Mod".into()),
                version: "1.2.0".into(),
            })
        );
        assert_eq!(
            loaded.version_string(&loaded.versions["modA"]).as_deref(),
            Some("1.2.0")
        );

        // The next start finds the label, saves nothing and leaves the file as it is.
        let before = fs::read(data.index()).unwrap();
        assert!(!name_game_archives(&data, &game, &crate::providers::archive_name_from).unwrap());
        assert_eq!(fs::read(data.index()).unwrap(), before);
    }

    #[test]
    fn a_thunderstore_archive_gets_its_version_on_the_next_start_and_keeps_its_name() {
        let base = scratch!("archive-labels-thunderstore");
        let root = Data::with_cache(base);
        let data = root.game(test_games::SLOT_ID);
        let game = test_games::slot();
        let folder = data.library().join("modA");
        fs::create_dir_all(folder.join("content")).unwrap();
        fs::write(folder.join("content/x.ws"), "a").unwrap();
        fs::write(folder.join("info.json"), r#"{"modName":"Package Name"}"#).unwrap();

        // The file name has the package slug and the version; the label must not rename the mod.
        let mut library = Library::default();
        let mut version = scan_mod(&data.library(), "modA", &game).unwrap();
        version.archive = Some("sha-t".into());
        version.parsed_sources.insert(
            crate::domain::source::ProviderId::new("thunderstore"),
            "example_owner-example_mod".into(),
        );
        library.versions.insert("modA".into(), version);
        library.archives.insert(
            "sha-t".into(),
            Archive {
                original_name: "example_owner-example_mod-1.0.0.zip".into(),
                file: "sha-t.zip".into(),
                named: None,
                source_files: BTreeMap::new(),
                source_parts: BTreeMap::new(),
                skipped: Vec::new(),
                skipped_total: 0,
            },
        );
        library.save(&data.index()).unwrap();

        let named = name_game_archives(&data, &game, &crate::providers::archive_name_from).unwrap();
        assert!(
            named,
            "the first start gives the archive its version and saves"
        );
        let mut loaded = Library::load(&data.index(), &data.library(), &game).unwrap();
        assert_eq!(
            loaded.archives["sha-t"].named,
            Some(ArchiveLabel {
                name: None,
                version: "1.0.0".into(),
            })
        );
        assert_eq!(loaded.versions["modA"].info.name, "Package Name");
        assert_eq!(
            loaded.version_string(&loaded.versions["modA"]).as_deref(),
            Some("1.0.0")
        );

        loaded.rescan(&data.library(), &game).unwrap();
        assert_eq!(loaded.versions["modA"].info.name, "Package Name");
        assert_eq!(
            loaded.versions["modA"].info.version.as_deref(),
            Some("1.0.0")
        );
    }

    /// An index as an older Gobbo wrote it: the archive has no label yet.
    fn unnamed_archive(root: &Data) {
        let data = root.game(test_games::SLOT_ID);
        let game = test_games::slot();
        let folder = data.library().join("modA");
        fs::create_dir_all(folder.join("content")).unwrap();
        fs::write(folder.join("content/x.ws"), "a").unwrap();
        let mut library = Library::default();
        let mut version = scan_mod(&data.library(), "modA", &game).unwrap();
        version.archive = Some("sha-a".into());
        version.parsed_sources.insert(
            crate::domain::source::ProviderId::new("nexus"),
            "12345".into(),
        );
        library.versions.insert("modA".into(), version);
        library.archives.insert(
            "sha-a".into(),
            Archive {
                original_name: "Example Mod 12345 1.2.0 2026-01-01T00-00Z example-token.zip".into(),
                file: "sha-a.zip".into(),
                named: None,
                source_files: BTreeMap::new(),
                source_parts: BTreeMap::new(),
                skipped: Vec::new(),
                skipped_total: 0,
            },
        );
        library.save(&data.index()).unwrap();
    }

    /// A temp file old enough that the startup cleanup removes it.
    fn stale_temp(root: &Path) -> PathBuf {
        let path = root.join("library.json.1-0.tmp");
        fs::write(&path, "old").unwrap();
        let two_hours_ago = SystemTime::now() - Duration::from_hours(2);
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(two_hours_ago)
            .unwrap();
        path
    }

    #[test]
    fn a_busy_migration_is_retried_and_the_opened_data_is_labelled_and_clean() {
        test_games::install();
        let base = scratch!("open-busy-retry");
        unnamed_archive(&Data::with_cache(base.clone()));
        let stale = stale_temp(&base);

        let (locked, locked_rx) = mpsc::channel();
        let (start, start_rx) = mpsc::channel();
        std::thread::scope(|scope| {
            let base_path = &base;
            scope.spawn(move || {
                let _lock = crate::context::lock_state(base_path).unwrap();
                locked.send(()).unwrap();
                start_rx.recv().unwrap();
                std::thread::sleep(Duration::from_millis(60));
            });
            locked_rx.recv().unwrap();
            start.send(()).unwrap();

            let data = open_with_budget(
                base.clone(),
                &crate::providers::archive_name_from,
                10,
                Duration::from_millis(10),
            )
            .unwrap();
            let index = data.game(test_games::SLOT_ID).index();
            assert!(
                fs::read_to_string(index).unwrap().contains("\"named\""),
                "the index is labelled after the retry"
            );
        });
        assert!(!stale.exists(), "the stale temp is removed");
    }

    #[test]
    fn a_migration_that_stays_busy_returns_its_error_after_cleaning() {
        test_games::install();
        let base = scratch!("open-busy-failure");
        let stale = stale_temp(&base);

        let (locked, locked_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let error = std::thread::scope(|scope| {
            let base_path = &base;
            scope.spawn(move || {
                let _lock = crate::context::lock_state(base_path).unwrap();
                locked.send(()).unwrap();
                release_rx.recv().unwrap();
            });
            locked_rx.recv().unwrap();
            let result = open_with_budget(
                base.clone(),
                &crate::providers::archive_name_from,
                2,
                Duration::from_millis(1),
            );
            release.send(()).unwrap();
            match result {
                Err(error) => error,
                Ok(_) => panic!("a held state lock must fail the migration"),
            }
        });
        assert_eq!(error.code, "state.busy");
        assert!(
            !stale.exists(),
            "cleanup runs even when the migration failed"
        );
    }
}
