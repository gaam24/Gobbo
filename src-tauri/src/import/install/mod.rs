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
use std::path::Path;

use crate::error::FileError;
use crate::games::GameDef;
use crate::infra::archive::Archive;

/// Working folder in the library. A name with a dot is never a mod folder, so it does not clash with one.
const STAGING: &str = ".importing";

mod apply;
mod errors;
mod layout;
mod limits;
mod loader;
mod model;
mod naming;
mod plan;
mod roots;

#[cfg(test)]
pub use apply::fail_after;
pub use apply::{extract, place};
pub use errors::InstallError;
pub use model::{Import, Installed, Naming, Plan, Staged};
pub(in crate::import) use model::{Placed, StagedMod};
pub use plan::{plan, plan_with_roots};
pub use roots::base_roots;

use limits::check_limits;
#[cfg(test)]
use loader::loader_layout;
use naming::{free_folder_name, is_folder_for, wanted_name};

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
#[expect(
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
    let base_name = crate::import::archive_base_name(&zip_path.to_string_lossy());
    let naming = Naming {
        base_name: &base_name,
        digest,
        only: None,
    };
    install_zip_named(zip_path, library, game, naming, allowed, progress)
}

#[cfg(test)]
mod tests;
