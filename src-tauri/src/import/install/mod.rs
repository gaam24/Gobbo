//! Imports a mod archive (for example a provider download) into the library. The game definition says where its mods go
//! (`games::Deploy`), and each file of the archive is placed by the first rule that takes it:
//! 1. a file under a mirrored folder keeps its game path (`archive/pc/mod/...`, `DLC/<dlc>/...`);
//! 2. a file under a mod root: a folder a slot's marker finds (`<modlet>/ModInfo.xml`, `<mod>/content/...`), or a
//!    folder at the archive root that holds a file the slot's `root_folder_extensions` names (`<Plugin>/<plugin>.dll`);
//! 3. a file under a slot folder (`Mods/<mod>/...`, or `BepInEx/plugins/<file>` directly);
//! 4. a file in a protected game folder that no rule took refuses the whole archive;
//! 5. a bare file at the archive root that a slot takes by its extension (`<plugin>.dll`).
//!
//! Anything else is skipped. Nothing reaches the library until the whole archive passes the checks.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::library::{Integrity, SkipReason, SkippedFile};
use crate::domain::pathkey::path_key;
use crate::error::FileError;
use crate::games::GameDef;
use crate::infra::archive::Archive;

/// Working folder in the library. A name with a dot is never a mod folder, so it does not clash with one.
const STAGING: &str = ".importing";

/// An archive may not write more than this in one import. A mod is not a disk image; a larger file is a mistake or a
/// decompression bomb, and the user gets a clear error instead of a full disk.
const MAX_EXTRACT_BYTES: u64 = 16 * 1024 * 1024 * 1024;
/// The most files one import may write.
const MAX_EXTRACT_FILES: usize = 100_000;

/// The shared expansion limit: a mod is not a disk image, and a very large one is a mistake or a bomb.
const fn check_limits(files: usize, total: u64) -> Result<(), InstallError> {
    if files > MAX_EXTRACT_FILES {
        return Err(InstallError::TooManyFiles {
            limit: MAX_EXTRACT_FILES,
        });
    }

    if total > MAX_EXTRACT_BYTES {
        return Err(InstallError::TooLarge {
            limit: MAX_EXTRACT_BYTES,
        });
    }

    Ok(())
}

/// A mod saved in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The folder in the library, for example `modSortEverything`.
    pub folder: String,
    /// The slot (its folder), empty for a mod that only mirrors game paths.
    pub slot: String,
    pub files: usize,
    /// Files relative to the library folder, in the order they were extracted. The caller can build the index
    /// record from these instead of walking the new folder again.
    pub paths: Vec<String>,
    /// Sum of the uncompressed sizes of the files.
    pub size_bytes: u64,
    /// Checksum of the extracted files, taken before the folder moved into the library.
    pub integrity: Option<Integrity>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Import {
    pub mods: Vec<Installed>,
    /// Entries no rule takes, for example a readme in the root.
    pub skipped: usize,
    /// Every entry that was not unpacked, with its reason.
    pub skipped_files: Vec<SkippedFile>,
}

/// An import failed. Nothing is saved when this is returned.
pub(super) struct Placed {
    pub(super) index: usize,
    /// The mod inside the archive the entry belongs to: the name of its folder in the archive, or the archive's own
    /// name for files that belong to the archive as a whole. Each group becomes one library folder.
    pub(super) group: String,
    /// The slot (its folder). Empty for a file that mirrors a game path.
    pub(super) slot: String,
    /// Path relative to the library folder: inside the mod for a slot, the game path for a mirror.
    pub(super) rel: String,
}

/// What the library folders of an archive's mods are called.
#[derive(Debug, Clone, Copy)]
pub struct Naming<'a> {
    /// The archive copy's stem. A mod that is alone in the archive takes it.
    pub base_name: &'a str,
    /// SHA-256 sum of the archive. When the name is taken, the start of the digest goes into it.
    pub digest: &'a str,
    /// A reinstall: only the mod that is the library folder `only` is unpacked, under that name.
    pub only: Option<&'a str>,
}

/// The placement of a loader release: the folder that holds the first program among the package's check files is the
/// release's root, and every file below it goes to the same path under the package's `to` folder. A repack that wraps
/// the release in more folders therefore lands where the original release does. Files outside that root are
/// skipped. `None` when the archive does not hold the program after all.
fn loader_layout(
    install: &crate::games::Install,
    files: &[(usize, Vec<&str>)],
    base_name: &str,
    skipped_files: &mut Vec<SkippedFile>,
) -> Option<Vec<Placed>> {
    let to = path_key(install.to.trim_end_matches('/'));
    let program = install.check.iter().find_map(|check| {
        let key = path_key(check);
        let rel = if to.is_empty() {
            key
        } else {
            key.strip_prefix(&format!("{to}/"))?.to_string()
        };
        std::path::Path::new(&rel)
            .extension()
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("dll") || extension.eq_ignore_ascii_case("exe")
            })
            .then_some(rel)
    })?;
    let program_depth = program.split('/').count();
    let root = files.iter().find_map(|(_, parts)| {
        let key = path_key(&parts.join("/"));
        let at = parts.len().checked_sub(program_depth)?;
        (key == program || key.ends_with(&format!("/{program}"))).then(|| parts[..at].to_vec())
    })?;
    let folder = install.to.trim_end_matches('/');
    let mut placed = Vec::new();

    for (index, parts) in files {
        let inside = parts.len() > root.len()
            && parts
                .iter()
                .zip(&root)
                .all(|(part, root)| part.eq_ignore_ascii_case(root));

        if inside {
            let rest = parts[root.len()..].join("/");
            placed.push(Placed {
                index: *index,
                group: base_name.to_string(),
                slot: String::new(),
                rel: if folder.is_empty() {
                    rest
                } else {
                    format!("{folder}/{rest}")
                },
            });
        } else {
            skipped_files.push(SkippedFile {
                path: parts.join("/"),
                reason: SkipReason::NoRule,
            });
        }
    }

    Some(placed)
}

/// Whether the game's own rules put every check file of the loader package at its game path.
fn places_every_check(install: &crate::games::Install, placed: &[Placed]) -> bool {
    install.check.iter().all(|check| {
        let check = path_key(check);
        placed
            .iter()
            .any(|item| item.slot.is_empty() && path_key(&item.rel) == check)
    })
}

/// The entries of an archive and the mod each one belongs to. Nothing is written yet.
pub struct Plan {
    pub(super) placed: Vec<Placed>,
    /// A file to write into a mod folder that a slot's `enable_file` names, by group. It is not an archive entry.
    pub(super) enable_files: BTreeMap<String, String>,
    /// Entries no rule takes, for example a readme in the root.
    pub(super) skipped: usize,
    /// Every entry that was not unpacked, with its reason.
    pub(super) skipped_files: Vec<SkippedFile>,
}

/// The mods of an archive, extracted into a working folder: mod `i` is in `dir/<i>` and waits for its library folder.
pub struct Staged {
    pub(super) dir: PathBuf,
    pub(super) mods: Vec<StagedMod>,
    pub(super) skipped: usize,
    pub(super) skipped_files: Vec<SkippedFile>,
}

/// A mod in the working folder.
pub(super) struct StagedMod {
    /// The mod inside the archive: the name of its folder in the archive, or the archive's own name for the files that
    /// belong to it as a whole.
    pub(super) group: String,
    pub(super) slot: String,
    pub(super) files: usize,
    pub(super) paths: Vec<String>,
    pub(super) size_bytes: u64,
    pub(super) integrity: Option<Integrity>,
}

/// Archive folders that are documentation or variants, not game files. They are skipped before the layout rules, so a
/// full-root game does not put a README folder into the game.
fn is_archive_noise(name: &str) -> bool {
    let first = name.split('/').next().unwrap_or(name).trim_end_matches('/');
    [
        "__macosx",
        ".git",
        ".svn",
        "docs",
        "documentation",
        "readme",
        "source",
        "src",
        "optional",
    ]
    .iter()
    .any(|folder| first.eq_ignore_ascii_case(folder))
}

/// The top-level folders of the game folder, sorted. A wrapper around one of them is stripped from an archive path.
/// A missing or unreadable folder gives an empty list; the version and same-name wrapper rules still apply.
pub fn top_level_folders(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort();
    names
}

/// Decides where each entry of the archive goes. Nothing is written. `base_name` names the files that belong to the
/// archive as a whole, and `allowed` limits the entries to the ones the FOMOD wizard chose.
pub fn plan(
    archive: &Archive,
    game: &GameDef,
    base_name: &str,
    allowed: Option<&BTreeSet<usize>>,
) -> Result<Plan, InstallError> {
    plan_with_roots(archive, game, base_name, allowed, None, &[])
}

/// `plan` with the game's own top-level folders, so a wrapper around `bin/` or `r6/` is stripped and a folder with a
/// game name is not a wrapper. Empty when the game folder is not known, for example in a unit test.
///
/// `virtual_paths` maps a selected archive entry to the game path its FOMOD pattern's `destination` asks for. It is
/// used for every layout decision in place of the archive name; the entry's real index is kept, so extraction still
/// reads the right bytes. `None` is a plain archive, where the archive path is the game path.
pub fn plan_with_roots<'a>(
    archive: &Archive,
    game: &'a GameDef,
    base_name: &str,
    allowed: Option<&BTreeSet<usize>>,
    virtual_paths: Option<&BTreeMap<usize, String>>,
    base_roots: &'a [String],
) -> Result<Plan, InstallError> {
    let deploy = &game.deploy;

    // The file entries this import considers, split into parts once.
    let mut files: Vec<(usize, Vec<&str>)> = Vec::new();
    let mut skipped_files = Vec::new();

    for (index, entry) in archive.entries().iter().enumerate() {
        if entry.is_dir {
            continue;
        }

        let name = entry.name.as_str();
        // A root `info.xml` is always installer metadata. The whole `fomod/` folder is metadata only for a plain
        // archive: with a FOMOD selection the `allowed` set already holds just the files whose source pattern matched,
        // and a real mod can keep its source files under `fomod/`. Compare without allocating a lowercased copy of
        // every entry name.
        let metadata = name.eq_ignore_ascii_case("info.xml")
            || (allowed.is_none()
                && name
                    .get(.."fomod/".len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("fomod/")));
        if metadata {
            skipped_files.push(SkippedFile {
                path: entry.name.clone(),
                reason: SkipReason::Metadata,
            });
            continue;
        }

        if is_archive_noise(name) {
            skipped_files.push(SkippedFile {
                path: entry.name.clone(),
                reason: SkipReason::Noise,
            });
            continue;
        }

        if allowed.is_some_and(|allowed| !allowed.contains(&index)) {
            skipped_files.push(SkippedFile {
                path: entry.name.clone(),
                reason: SkipReason::NotSelected,
            });
            continue;
        }
        // Layout decisions use the FOMOD destination when the wizard supplied one. The archive name is still the
        // source of truth for the metadata/noise/choice checks above and for `SkippedFile` paths below.
        let layout_name = virtual_paths
            .and_then(|paths| paths.get(&index))
            .map_or(name, String::as_str);
        files.push((index, split_name(layout_name)?));
    }

    // A loader release keeps the game's own rules when they already put every check file at its game path, as a
    // `BepInEx` pack with plugins does: its plugins still go to their slot. Otherwise the release is one mod whose files
    // keep their game paths, wherever the archive keeps the release (UE4SS below the game's program folder).
    let loader = game.install.as_ref().filter(|_| {
        game.is_loader_archive(archive.entries().iter().map(|entry| entry.name.as_str()))
    });
    let loader_files = loader.map(|_| files.clone());
    let before = skipped_files.len();

    let layout = LayoutPlan::new(deploy, base_roots);
    let wrapper = layout.wrapper_depth(&files);

    for (_, parts) in &mut files {
        let strip = wrapper.min(parts.len().saturating_sub(1));
        parts.drain(..strip);
        layout.rewrite_aliases(parts);
    }

    let units = layout.unit_roots(&files);
    let unit_index = UnitIndex::new(&units);

    let mut placed = Vec::new();
    let mut skipped = 0;

    for (index, parts) in &files {
        if let Some((group, slot, rel)) = layout.classify(parts, &unit_index, &units, base_name)? {
            placed.push(Placed {
                index: *index,
                group,
                slot,
                rel,
            });
        } else {
            skipped += 1;
            skipped_files.push(SkippedFile {
                path: archive.entries()[*index].name.clone(),
                reason: SkipReason::NoRule,
            });
        }
    }

    if let (Some(install), Some(loader_files)) = (loader, loader_files)
        && !places_every_check(install, &placed)
    {
        let mut loader_skipped = skipped_files[..before].to_vec();

        if let Some(placed) = loader_layout(install, &loader_files, base_name, &mut loader_skipped)
        {
            refuse_unwritable(&placed, archive)?;
            let skipped = loader_skipped.len() - before;
            return Ok(Plan {
                placed,
                enable_files: BTreeMap::new(),
                skipped,
                skipped_files: loader_skipped,
            });
        }
    }

    if placed.is_empty() {
        return Err(InstallError::UnknownLayout);
    }
    refuse_unwritable(&placed, archive)?;

    attach_mirrored(&mut placed);
    // A slot's `enable_file` is written once per mod, unless the archive already ships it.
    let mut enable_files: BTreeMap<String, String> = BTreeMap::new();

    for item in &placed {
        if enable_files.contains_key(&item.group) {
            continue;
        }

        if let Some(file) = deploy
            .slots
            .iter()
            .find(|slot| slot.dir == item.slot)
            .and_then(|slot| slot.enable_file.as_ref())
        {
            enable_files.insert(item.group.clone(), file.clone());
        }
    }

    for item in &placed {
        if enable_files
            .get(&item.group)
            .is_some_and(|file| path_key(file) == path_key(&item.rel))
        {
            enable_files.remove(&item.group);
        }
    }

    Ok(Plan {
        placed,
        enable_files,
        skipped,
        skipped_files,
    })
}

/// Imports the archive into `library`, with the folders named as `naming` says. `allowed` limits the import to the
/// entries chosen in the FOMOD wizard (indexes of the entries). `progress` receives the percentage of extracted bytes.
#[cfg(test)]
pub fn install_zip_named(
    zip_path: &Path,
    library: &Path,
    game: &GameDef,
    naming: Naming<'_>,
    allowed: Option<&BTreeSet<usize>>,
    progress: &mut dyn FnMut(u32),
) -> Result<Import, InstallError> {
    install_zip_named_with_roots(
        zip_path,
        library,
        game,
        naming,
        allowed,
        None,
        &[],
        progress,
    )
}

/// `install_zip_named` with the game's own top-level folders, so a wrapper around `bin/` or `r6/` is stripped.
/// `virtual_paths` carries the FOMOD destinations, if any, through to `plan_with_roots`.
#[allow(
    clippy::too_many_arguments,
    reason = "the archive, library, game, naming, choice filter, FOMOD paths, game roots and progress are one import"
)]
pub fn install_zip_named_with_roots(
    zip_path: &Path,
    library: &Path,
    game: &GameDef,
    naming: Naming<'_>,
    allowed: Option<&BTreeSet<usize>>,
    virtual_paths: Option<&BTreeMap<usize, String>>,
    base_roots: &[String],
    progress: &mut dyn FnMut(u32),
) -> Result<Import, InstallError> {
    let archive = Archive::open(zip_path)?;
    let mut planned = plan_with_roots(
        &archive,
        game,
        naming.base_name,
        allowed,
        virtual_paths,
        base_roots,
    )?;
    let total: u64 = planned
        .placed
        .iter()
        .map(|item| archive.entries()[item.index].size)
        .sum();
    check_limits(planned.placed.len() + planned.enable_files.len(), total)?;

    if let Some(space) = crate::infra::disk::space_for(library) {
        let free = space.free_kb.saturating_mul(1024);

        if total > free {
            return Err(InstallError::NoSpace {
                needed_kb: total / 1024,
                free_kb: space.free_kb,
            });
        }
    }

    if let Some(folder) = naming.only {
        // A reinstall keeps the mod that had the folder `folder` and leaves out the other mods of the archive.
        let groups: BTreeSet<String> = planned
            .placed
            .iter()
            .map(|item| item.group.clone())
            .collect();
        let keep: BTreeSet<String> = groups
            .iter()
            .filter(|group| {
                let wanted = wanted_name(naming.base_name, groups.len(), group);
                is_folder_for(folder, &wanted, naming.digest)
            })
            .cloned()
            .collect();
        // A single-mod archive has one candidate. Its folder may carry a recipe's display name instead of the archive's
        // name (a collection member), so the name cannot decide; the archive is already the one this folder came from.
        let keep = if keep.is_empty() && groups.len() == 1 {
            groups
        } else {
            keep
        };
        planned.placed.retain(|item| keep.contains(&item.group));
        planned.enable_files.retain(|group, _| keep.contains(group));

        if planned.placed.is_empty() {
            return Err(InstallError::UnknownLayout);
        }
    }

    fs::create_dir_all(library).map_err(|e| FileError::new(library, e))?;
    let staging = library.join(STAGING);
    let _ = fs::remove_dir_all(&staging);
    let result = extract(&archive, &planned, &staging, None, progress)
        .and_then(|mut staged| place(&mut staged, library, naming));
    let _ = fs::remove_dir_all(&staging);
    result
}

/// Imports an archive under its own file name. The application uses `install_zip_named`, because the archive copy may
/// have received a collision suffix; tests use this convenience wrapper.
#[cfg(test)]
pub fn install_zip(
    zip_path: &Path,
    library: &Path,
    game: &GameDef,
    digest: &str,
    allowed: Option<&BTreeSet<usize>>,
    progress: &mut dyn FnMut(u32),
) -> Result<Import, InstallError> {
    let base_name = zip_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("mod")
        .to_string();
    let naming = Naming {
        base_name: &base_name,
        digest,
        only: None,
    };
    install_zip_named(zip_path, library, game, naming, allowed, progress)
}

/// Mirrored files (for example `DLC/` or `bin/config/`) belong to the archive's main mod, so they join its group.
/// Without a main mod they stay together. With several main mods, the first one in name order takes them.
mod apply;
mod errors;
mod layout;
mod naming;
mod roots;

#[cfg(test)]
pub use apply::fail_after;
pub use apply::{extract, place};
pub use errors::InstallError;
use layout::{LayoutPlan, UnitIndex, attach_mirrored};
use naming::{free_folder_name, is_folder_for, refuse_unwritable, split_name, wanted_name};
pub use roots::base_roots;

#[cfg(test)]
mod tests;
