//! Staging the archives of a pack, checking their digests and handing each group to `acquire`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use sha2::Sha256;

use crate::acquire::{Acquired, BatchRequest, Choices, Origin};
use crate::context::Data;
use crate::context::load_library;
use crate::error::AppError;
use crate::error::io_at;
use crate::games;
use crate::import::BATCH_SIZE;
use crate::import::fomod;
use crate::infra::io::hash_and_copy_reader;
use crate::infra::tasks::stop_if;
use crate::packs::PACK_CANCELLED;

use crate::packs::inspect::{Opened, pack_error};
use crate::packs::model::PackImportView;

use super::{MAX_PACK_ARCHIVE_BYTES, MAX_PACK_TOTAL_BYTES, Staging, pack_stop};

/// Imports the archives inside the pack that the library does not have yet, with the FOMOD choices of their mods.
pub(in crate::packs) fn import_bundled(
    root: &Data,
    game_id: &str,
    data: &Data,
    opened: &mut Opened,
    view: &mut PackImportView,
    progress: &mut dyn FnMut(crate::import::model::ImportStage, u32),
    cancel: Option<&AtomicBool>,
) -> Result<BTreeMap<String, Vec<String>>, AppError> {
    let game = games::load(game_id)?;
    let library = load_library(data, &game)?;
    let choices: BTreeMap<
        &str,
        (
            Option<&[fomod::Choice]>,
            Option<crate::domain::source::SourceRef>,
        ),
    > = opened
        .manifest
        .profiles
        .iter()
        .flat_map(|profile| &profile.mods)
        .filter_map(|entry| {
            Some((
                entry.archive.as_ref()?.sha256.as_str(),
                (entry.installer_choices(), entry.archive_source()),
            ))
        })
        .collect();
    let expected: BTreeMap<&str, u64> = opened
        .manifest
        .profiles
        .iter()
        .flat_map(|profile| &profile.mods)
        .filter_map(|entry| {
            let archive = entry.archive.as_ref()?;
            Some((archive.sha256.as_str(), archive.size))
        })
        .collect();
    let total_expected = expected
        .values()
        .fold(0u64, |total, size| total.saturating_add(*size));
    if total_expected > MAX_PACK_TOTAL_BYTES {
        return Err(AppError::user("pack.too_large").with("limit", MAX_PACK_TOTAL_BYTES));
    }
    // A declared archive over the per-archive cap is named before the whole-pack rules can answer for it.
    if let Some(archive) = opened
        .manifest
        .profiles
        .iter()
        .flat_map(|profile| &profile.mods)
        .filter_map(|entry| entry.archive.as_ref())
        .find(|archive| archive.size > MAX_PACK_ARCHIVE_BYTES)
    {
        return Err(AppError::user("pack.archive_too_large").with("name", &archive.name));
    }
    // The digests the library already has, built once: a per-archive lookup would scan every version each time.
    let known_archives: rustc_hash::FxHashSet<&str> = library
        .versions
        .values()
        .filter_map(|version| version.archive.as_deref())
        .collect();
    // The archives this import still has to stage, with the size the manifest promises. The set is only named, not
    // written, so the free-space check happens before the first byte lands on disk.
    let staged: Vec<(&str, &str, u64)> = opened
        .archives
        .iter()
        .filter(|(sha256, _)| !known_archives.contains(sha256.as_str()))
        .filter_map(|(sha256, name)| {
            Path::new(name).file_name()?;
            let expected_size = expected.get(sha256.as_str()).copied().unwrap_or(0);
            Some((sha256.as_str(), name.as_str(), expected_size))
        })
        .collect();
    let needed = staged
        .iter()
        .fold(0u64, |total, (_, _, size)| total.saturating_add(*size));
    if let Some(space) = crate::infra::disk::space_for(&data.dir) {
        let free = space.free_kb.saturating_mul(1024);

        if needed > free {
            return Err(AppError::user("install.no_space")
                .with("needed", needed / 1024)
                .with("free", space.free_kb));
        }
    }

    let staging = Staging::create(data)?;
    // The folders this import just created, by digest. A pack with several versions of one archive resolves them to
    // these folders instead of an older copy the library already had.
    let mut imported: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut copied_total = 0u64;
    // A pack has no download phase; it reports the import phases of its archives and no others.
    let mut acquire_progress = |phase: crate::acquire::Phase, percent: u32| {
        if let crate::acquire::Phase::Importing(stage) = phase {
            progress(stage, percent);
        }
    };
    // The staged archives go to `acquire` one group at a time, so a large pack never keeps every archive on disk
    // before the first one is imported. Staging, the digest check above and the profile placement below stay here;
    // the exact-file check, the FOMOD answers, the import and the duplicate rule run in `acquire`.
    for (chunk_index, chunk) in staged.chunks(BATCH_SIZE).enumerate() {
        // A cancel stops between groups: the group already imported stays in the library.
        stop_if(cancel, PACK_CANCELLED)?;
        let mut labels = Vec::with_capacity(chunk.len());
        let mut targets = Vec::with_capacity(chunk.len());
        let mut choices_of = Vec::with_capacity(chunk.len());
        let mut digests = Vec::with_capacity(chunk.len());
        let mut folders = Vec::with_capacity(chunk.len());

        for (number, &(sha256, name, expected_size)) in chunk.iter().enumerate() {
            let Some(file_name) = Path::new(name).file_name() else {
                continue;
            };
            // The folder is numbered, so a name from the pack can never point outside the staging folder.
            let folder = staging.0.join(format!("{chunk_index}-{number}"));
            fs::create_dir_all(&folder).map_err(io_at(&folder))?;
            let target = folder.join(file_name);
            let entry = opened.zip.by_name(name).map_err(pack_error)?;
            let declared = entry.size();

            if declared > MAX_PACK_ARCHIVE_BYTES {
                return Err(AppError::user("pack.archive_too_large").with("name", name));
            }
            // The copy is hashed in the same pass, so the archive is not read again just to check its sum.
            let (digest, copied) = hash_and_copy_reader::<Sha256, _>(
                &mut entry.take(MAX_PACK_ARCHIVE_BYTES + 1),
                &target,
                declared,
                &mut |_| {},
            )
            .map_err(io_at(&target))?;

            if copied > MAX_PACK_ARCHIVE_BYTES {
                return Err(AppError::user("pack.archive_too_large").with("name", name));
            }
            // The manifest's total is untrusted: the bytes actually copied are capped too.
            copied_total = copied_total.saturating_add(copied);

            if copied_total > MAX_PACK_TOTAL_BYTES {
                return Err(AppError::user("pack.too_large").with("limit", MAX_PACK_TOTAL_BYTES));
            }

            if (expected_size > 0 && copied != expected_size) || digest != sha256 {
                return Err(AppError::user("pack.archive_mismatch").with("name", name));
            }

            let (fomod, source) = choices.get(sha256).cloned().unwrap_or_default();
            labels.push(file_name.to_string_lossy().into_owned());
            targets.push(target);
            choices_of.push((fomod, source));
            digests.push(sha256.to_string());
            folders.push(folder);
        }

        let batch: Vec<BatchRequest<'_>> = targets
            .iter()
            .zip(choices_of)
            .zip(&digests)
            .map(|((target, (fomod, source)), digest)| BatchRequest {
                origin: Origin::File(target),
                source,
                // These are the author's answers kept by the recipe: stale ones ask for the wizard, as in `acquire`.
                choices: fomod.map_or(Choices::Ask, |choices| Choices::Stored(choices.to_vec())),
                staged: Some(digest.clone()),
            })
            .collect();
        let results =
            crate::acquire::acquire_batch(root, game_id, batch, cancel, &mut acquire_progress)
                .map_err(pack_stop)?;
        // The library has its own copies now, so this group's staged bytes leave the disk before the next group.
        for folder in folders {
            let _ = fs::remove_dir_all(folder);
        }

        for ((label, digest), result) in labels.into_iter().zip(digests).zip(results) {
            match result {
                Ok(Acquired::Imported(imported_view)) => {
                    view.imported += 1;

                    for mod_view in imported_view.mods {
                        imported
                            .entry(mod_view.sha256)
                            .or_default()
                            .push(mod_view.folder);
                    }
                }
                // The file id was already in the library: resolve this mod to the folder that holds it.
                Ok(Acquired::Existing { folder }) => {
                    imported.entry(digest).or_default().push(folder);
                }
                Ok(Acquired::NeedsChoice { .. }) | Err(_) => view.failed.push(label),
            }
        }
    }

    Ok(imported)
}
