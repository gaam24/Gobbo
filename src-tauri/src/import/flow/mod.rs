//! The import flow: copy and hash the archive, reserve its name, extract it, place the mods and save the index.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use rustc_hash::{FxHashMap, FxHashSet};

use super::model::{ImportStage, ImportView};
use crate::context::library::Library;
use crate::context::{Data, load_library, load_library_shared, load_settings};
use crate::domain::source::SourceRef;
use crate::error::AppError;
use crate::games::{self, GameDef};
use crate::import::IMPORT_CANCELLED;
use crate::import::fomod;
use crate::import::install;
use crate::infra::tasks::stop_if;
use crate::library;

use commit::{commit_prepared, forget_download};
use prepare::{copy_and_hash, extract_copy, prepare_archives, reserve_name};

mod commit;
mod prepare;
mod working;

pub use commit::{already_in_library, remove_folders};

/// The top-level folders of the game's deployment root, for the wrapper rules. A missing game folder gives an empty
/// list; the version and same-name rules still recognise the common archive wrappers.
fn base_roots(data: &Data, game: &GameDef) -> Vec<String> {
    let settings = load_settings(data);
    crate::context::target::deployment_root(data, game, &settings)
        .ok()
        .map(|root| install::top_level_folders(&root))
        .unwrap_or_default()
}

/// One archive to import, with what the caller knows about it. Every import goes through this type: a loose file, a
/// download, a pack, a collection member and an update.
#[derive(Debug, Clone, Default)]
pub struct ImportRequest {
    /// The archive file to import.
    pub zip: PathBuf,
    /// The FOMOD choices picked for it. `Some` (even empty) makes the installer choose the files.
    pub choices: Option<Vec<fomod::Choice>>,
    /// The provider file the archive came from. Without it, the sidecar or a provider that recognises the archive name
    /// supplies the source.
    pub source: Option<SourceRef>,
    /// The SHA-256 of a temporary copy the caller made and hashed itself, such as an archive staged out of a pack. The
    /// import then moves the file into its working folder instead of copying and hashing it a second time.
    pub staged: Option<String>,
}

impl ImportRequest {
    /// A request for one archive with nothing else known.
    pub fn new(zip: impl Into<PathBuf>) -> Self {
        Self {
            zip: zip.into(),
            ..Self::default()
        }
    }

    /// The request with the provider file the archive came from.
    #[must_use]
    pub fn with_source(mut self, source: Option<SourceRef>) -> Self {
        self.source = source;
        self
    }

    /// The request with the FOMOD choices to apply.
    #[must_use]
    pub fn with_choices(mut self, choices: Option<Vec<fomod::Choice>>) -> Self {
        self.choices = choices;
        self
    }

    /// The request for a temporary copy the caller already hashed: the import takes the file over by moving it.
    #[must_use]
    pub fn staged(mut self, sha256: impl Into<String>) -> Self {
        self.staged = Some(sha256.into());
        self
    }
}

/// Imports one archive into the library: the single-archive wrapper over [`import_batch`] that tests and benches call.
/// Production goes through `import_batch` directly. It works also while a deployment is
/// active: the mods land in the library, and the profiles adopt them disabled at the next status read. `source` keeps
/// the mod id and the file id whatever the archive's name says. Imports may run at the same time: they only wait for
/// each other while they name and write. A flag stops the copy, the extraction or the commit before it starts; a
/// stopped import commits nothing.
#[allow(
    dead_code,
    reason = "the documented single-archive import entry point, used by test and bench callers"
)]
pub fn import_archive(
    root: &Data,
    game_id: &str,
    request: ImportRequest,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(ImportStage, u32),
) -> Result<ImportView, AppError> {
    // Imports are safe to overlap: the library index, the archive reservation and the folder repair already serialize
    // the two writers. The state lock is for operations that move the game folder or the profiles, not for this one.
    // A single import is a batch of one, so the two can never drift apart.
    let mut results = run_batch(
        root,
        game_id,
        vec![request],
        progress,
        cancel,
        Stops::Anywhere,
    )?;
    results
        .pop()
        .unwrap_or_else(|| Err(AppError::unexpected("an import produced no result")))
}

/// How many archives of a batch are on disk at once. The batch saves the library once per group; a whole large pack at
/// once would also keep every archive copy and every extracted mod on disk together, which a pack of tens of gigabytes
/// cannot afford.
pub const BATCH_SIZE: usize = 4;

/// Imports a batch of archives, one group at a time, with one index read and one save per group. Returns one result per
/// request, in order. A failed archive is reported in its own slot while the rest still import. A flag stops the batch
/// with `import.cancelled` between groups; the groups already committed stay.
pub fn import_batch(
    root: &Data,
    game_id: &str,
    requests: Vec<ImportRequest>,
    progress: &mut dyn FnMut(ImportStage, u32),
    cancel: Option<&AtomicBool>,
) -> Result<Vec<Result<ImportView, AppError>>, AppError> {
    run_batch(
        root,
        game_id,
        requests,
        progress,
        cancel,
        Stops::BetweenGroups,
    )
}

/// Where a flag may stop an import. A single import stops at once; a batch lets the group already on disk finish, so
/// the archives it copied and extracted are not thrown away.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Stops {
    Anywhere,
    BetweenGroups,
}

/// The body of every import: groups of `BATCH_SIZE`, each reserved, extracted and committed with one index save.
fn run_batch(
    root: &Data,
    game_id: &str,
    requests: Vec<ImportRequest>,
    progress: &mut dyn FnMut(ImportStage, u32),
    cancel: Option<&AtomicBool>,
    stops: Stops,
) -> Result<Vec<Result<ImportView, AppError>>, AppError> {
    let inside = (stops == Stops::Anywhere).then_some(cancel).flatten();
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    // The cleanup and the archive set serve the whole batch. Rebuilding them for every group made a 40-archive import
    // list the library folder and its names ten times.
    let mut taken = {
        let _index = library::lock_index();
        let library = load_library_shared(&data, &game)?;
        prepare_archives(&data, &library)?
    };
    let mut results = Vec::with_capacity(requests.len());
    let mut pending = requests;

    while !pending.is_empty() {
        // A stop keeps the groups already committed; the running group commits nothing.
        stop_if(cancel, IMPORT_CANCELLED)?;
        let take = pending.len().min(BATCH_SIZE);
        let group: Vec<ImportRequest> = pending.drain(..take).collect();
        results.extend(import_group(
            &data, &game, group, &mut taken, inside, progress,
        )?);
    }

    Ok(results)
}

/// One group of `import_batch`: reserve, extract and commit its archives, then save the index once. `taken` is the
/// archive set prepared once for the whole batch.
fn import_group(
    data: &Data,
    game: &GameDef,
    requests: Vec<ImportRequest>,
    taken: &mut std::collections::BTreeSet<String>,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(ImportStage, u32),
) -> Result<Vec<Result<ImportView, AppError>>, AppError> {
    let mut results: Vec<Option<Result<ImportView, AppError>>> =
        (0..requests.len()).map(|_| None).collect();
    let roots = base_roots(data, game);

    // Copy and hash every archive outside the index lock. A large pack spends most of its time here.
    let mut copies = Vec::with_capacity(requests.len());

    for (index, request) in requests.into_iter().enumerate() {
        let ImportRequest {
            zip,
            choices,
            source,
            staged,
        } = request;

        match copy_and_hash(data, &zip, source, staged, cancel, progress) {
            Ok(copy) => copies.push((index, choices, copy)),
            Err(error) => results[index] = Some(Err(error)),
        }
    }

    stop_if(cancel, IMPORT_CANCELLED)?;

    // Reserve the archive names under one lock and one library read.
    let mut reserved = Vec::with_capacity(copies.len());
    {
        let _index = library::lock_index();
        let library = load_library_shared(data, game)?;
        let wanted: FxHashSet<&str> = copies
            .iter()
            .map(|(_, _, copy)| copy.digest.as_str())
            .collect();
        let by_archive = archive_folders(&library, &wanted);

        for (index, choices, copy) in copies {
            if let Some(folder) = by_archive.get(copy.digest.as_str()) {
                results[index] = Some(Err(already_in_library(folder)));
                continue;
            }

            match reserve_name(data, &copy, taken) {
                Ok((file, claim)) => reserved.push((index, choices, copy, claim, file)),
                Err(error) => results[index] = Some(Err(error)),
            }
        }
    }

    // Extract outside the lock, as a single import does.
    let mut ready = Vec::with_capacity(reserved.len());

    for (index, choices, copy, claim, file) in reserved {
        match extract_copy(
            copy,
            claim,
            file,
            choices,
            prepare::Rules {
                game,
                base_roots: &roots,
            },
            cancel,
            progress,
        ) {
            Ok(prepared) => ready.push((index, prepared)),
            Err(error) => results[index] = Some(Err(error)),
        }
    }

    stop_if(cancel, IMPORT_CANCELLED)?;
    // Commit everything under one lock and one library read, then save once.
    let mut committed = Vec::with_capacity(ready.len());
    let mut folders = Vec::new();
    {
        let _index = library::lock_index();
        let mut library = load_library(data, game)?;
        let wanted: FxHashSet<&str> = ready
            .iter()
            .map(|(_, prepared)| prepared.copy.digest.as_str())
            .collect();
        let by_archive = archive_folders(&library, &wanted);

        for (index, mut prepared) in ready {
            if let Some(folder) = by_archive.get(prepared.copy.digest.as_str()) {
                results[index] = Some(Err(already_in_library(folder)));
                continue;
            }

            match commit_prepared(data, game, &mut library, &mut prepared) {
                Ok(done) => {
                    folders.extend(done.folders.iter().cloned());
                    committed.push((index, prepared, done));
                }
                Err(error) => results[index] = Some(Err(error)),
            }
        }

        if let Err(error) = data.save_library(library) {
            remove_folders(&data.library(), &folders);
            return Err(error);
        }
    }

    for (index, mut prepared, done) in committed {
        prepared.claim.committed = true;
        forget_download(data, &prepared.copy);
        results[index] = Some(Ok(ImportView {
            folders: done.folders,
            files: done.files,
            skipped: done.skipped,
            skipped_files: done.skipped_files,
            mods: done.mods,
        }));
    }

    Ok(results
        .into_iter()
        .map(|result| {
            result.unwrap_or_else(|| Err(AppError::unexpected("an import produced no result")))
        })
        .collect())
}

/// The folder each wanted archive already owns, by digest. A batch checks its archives against one map instead of
/// scanning the whole library once per archive.
fn archive_folders(library: &Library, wanted: &FxHashSet<&str>) -> FxHashMap<String, String> {
    library
        .versions
        .values()
        .filter_map(|version| {
            let digest = version.archive.as_deref()?;
            wanted
                .contains(digest)
                .then(|| (digest.to_string(), version.folder.clone()))
        })
        .collect()
}
