//! Preparing an import: copy and hash the archive, reserve its name and extract the mods into a working folder.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sha2::Sha256;

use super::super::archive_base_name;
use super::super::model::ImportStage;
use super::super::wizard::fomod_allowed;
use super::working::{Claim, WORKING_PREFIX, Working, remove_stale_working, working_copy_name};
use crate::context::Data;
use crate::domain::source::SourceRef;
use crate::error::{AppError, io_at};
use crate::games::GameDef;
use crate::import::IMPORT_CANCELLED;
use crate::import::fomod;
use crate::import::install;
use crate::infra::archive;
use crate::infra::io::{hash_and_copy, hash_and_copy_cancellable};
use crate::infra::store;
use crate::infra::tasks::cancelled;
use crate::library::Library;

/// An archive copied to a working folder and hashed, before its archive name is reserved.
pub(super) struct Copy {
    pub(super) original: String,
    pub(super) digest: String,
    pub(super) copy: PathBuf,
    pub(super) working: Arc<Working>,
    pub(super) source: Option<SourceRef>,
    pub(super) download: Option<PathBuf>,
}

/// A copied archive with its name reserved and its files extracted: ready for the index lock that commits it.
pub(super) struct Prepared {
    pub(super) copy: Copy,
    pub(super) claim: Claim,
    pub(super) file: String,
    pub(super) base_name: String,
    pub(super) staged: install::Staged,
    pub(super) choices: Vec<fomod::Choice>,
    pub(super) installer: bool,
}

/// Copies the archive into a working folder and hashes the copy in the same pass, so a large archive is not read twice
/// just to be stored and named.
pub(super) fn copy_and_hash(
    data: &Data,
    zip: &Path,
    source: Option<SourceRef>,
    staged: Option<String>,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(ImportStage, u32),
) -> Result<Copy, AppError> {
    let original = zip.file_name().map_or_else(
        || "archive.zip".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let source = source
        .or_else(|| crate::infra::sidecar::read_source(zip))
        .or_else(|| crate::providers::source_from_archive_name(&original));
    let working_path = data
        .library()
        .join(format!("{WORKING_PREFIX}{}", store::unique_suffix()));
    let working = Arc::new(Working::create(working_path.clone()).map_err(io_at(&working_path))?);
    // The copy keeps the source extension, because the reader picks the format from it.
    let copy = working.path.join(working_copy_name(zip));
    // A staged copy is the caller's own temporary file with a known sum: it moves in, and only a move across disks
    // falls back to the copy below.
    if let Some(digest) = staged
        && std::fs::rename(zip, &copy).is_ok()
    {
        progress(ImportStage::Hashing, 100);
        return Ok(Copy {
            original,
            digest,
            copy,
            working,
            source,
            download: None,
        });
    }

    let mut report = |percent| progress(ImportStage::Hashing, percent);
    let digest = match cancel {
        Some(flag) => hash_and_copy_cancellable::<Sha256>(zip, &copy, flag, &mut report),
        None => hash_and_copy::<Sha256>(zip, &copy, &mut report),
    }
    .map_err(|error| {
        if error.kind() == io::ErrorKind::Interrupted && cancelled(cancel) {
            AppError::user(IMPORT_CANCELLED)
        } else {
            io_at(zip)(error)
        }
    })?;
    Ok(Copy {
        original,
        digest,
        copy,
        working,
        source,
        download: Some(zip.to_path_buf()),
    })
}

/// Loads the archive names the library holds and clears stale working folders. The caller holds the index lock.
pub(super) fn prepare_archives(
    data: &Data,
    library: &Library,
) -> Result<std::collections::BTreeSet<String>, AppError> {
    let archives = data.archives();
    remove_stale_working(&data.library());
    std::fs::create_dir_all(&archives).map_err(io_at(&archives))?;
    Ok(library.archive_files())
}

/// Reserves the archive's name before extracting: an empty file holds it, no other import takes it, and a cleanup does
/// not remove it. The caller holds the index lock.
pub(super) fn reserve_name(
    data: &Data,
    copy: &Copy,
    taken: &mut std::collections::BTreeSet<String>,
) -> Result<(String, Claim), AppError> {
    let archives = data.archives();
    let (file, reserved) = loop {
        let file = store::free_archive_name(&archives, &copy.original, &copy.digest, taken);
        let reserved = archives.join(&file);

        match std::fs::File::create_new(&reserved) {
            Ok(_file) => break (file, reserved),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                // Another process took the name between the check and the create. The next name carries the suffix;
                // `free_archive_name` checks that one against `taken` too.
                if !taken.insert(file) {
                    return Err(io_at(&reserved)(error));
                }
            }
            Err(error) => return Err(io_at(&reserved)(error)),
        }
    };
    store::reserve_archive(&reserved);
    Ok((
        file,
        Claim {
            archive: reserved,
            working: Arc::clone(&copy.working),
            committed: false,
        },
    ))
}

/// The game rules of one import: the definition and the game's own top-level folders, which tell a wrapper from a
/// folder the game has.
pub(super) struct Rules<'a> {
    pub(super) game: &'a GameDef,
    pub(super) base_roots: &'a [String],
}

/// Reads the copied archive, applies the FOMOD choices and extracts the planned files. The claim removes the reserved
/// archive and the working folder when this fails.
#[allow(
    clippy::too_many_arguments,
    reason = "the copy, its claim, choices, rules, cancellation and progress are one import"
)]
pub(super) fn extract_copy(
    copy: Copy,
    claim: Claim,
    file: String,
    choices: Option<Vec<fomod::Choice>>,
    rules: Rules<'_>,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(ImportStage, u32),
) -> Result<Prepared, AppError> {
    let reader = archive::Archive::open(&copy.copy)?;
    // We resolve the FOMOD wizard choices before extracting: files outside the choice count neither toward the library
    // nor toward the skipped ones. The same open archive is used by the plan below.
    let installer = choices.is_some();
    // A named installer always filters, even with no option picked: required options and the installer itself still
    // choose the files, and the optional ones that were not picked stay out. The same selection carries the FOMOD
    // destinations so the plan places files where the installer asks, not where they sit inside the archive.
    let selection = match &choices {
        Some(choices) => Some(fomod_allowed(&reader, choices)?),
        None => None,
    };
    let base_name = archive_base_name(&file);
    let plan = install::plan_with_roots(
        &reader,
        rules.game,
        &base_name,
        selection.as_ref().map(|selection| &selection.allowed),
        selection.as_ref().map(|selection| &selection.virtual_paths),
        rules.base_roots,
    )?;
    let staged = install::extract(
        &reader,
        &plan,
        &claim.working.path,
        cancel,
        &mut |percent| {
            progress(ImportStage::Extracting, percent);
        },
    )?;
    Ok(Prepared {
        copy,
        claim,
        file,
        base_name,
        staged,
        choices: choices.unwrap_or_default(),
        installer,
    })
}
