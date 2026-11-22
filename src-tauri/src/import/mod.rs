//! Importing mods into the library: the archive is read, its layout is recognised and each file is placed by the rules
//! of the game (`install`), and FOMOD installers are read for their choices (`fomod`). The module also owns the
//! commands, the wizard data and the reinstall flow.

pub mod commands;
pub mod fomod;
pub mod install;
pub mod preview;
pub mod tool;

// ---------------------------------------------------------------------------------------------
// The use cases and commands of this module.
// ---------------------------------------------------------------------------------------------

mod flow;
mod inspect;
pub mod model;
mod reinstall;
mod report;
mod skipped;
mod wizard;

use std::path::Path;

pub use flow::BATCH_SIZE;
pub use flow::ImportRequest;
#[allow(
    unused_imports,
    reason = "the one-archive import entry point is used by test and bench callers, and kept here as the documented single-archive API"
)]
pub use flow::import_archive;
#[cfg(test)]
pub use flow::remove_folders;
pub use flow::{already_in_library, import_batch};
pub use inspect::{inspect_at, state_at};
#[cfg(test)]
pub use install::fail_after;
pub use model::{
    ArchiveReport, FomodGroupView, FomodOptionView, FomodStepView, ImportProgress, ImportView,
    ZipInspect,
};
#[cfg(test)]
pub use reinstall::reinstall_mod;
pub use reinstall::reinstall_mod_with;
pub use reinstall::{reinstall_fomod_state, reinstall_inspect};
pub use report::report as archive_report;

/// The stable error code of an import the user stopped. A check treats it as a stop, not as a failure. The siblings
/// `download.cancelled`, `pack.cancelled` and `collection.cancelled` name the other contexts of the same underlying
/// stop; all four are kept distinct so the frontend can answer in the right context.
pub const IMPORT_CANCELLED: &str = "import.cancelled";

/// The stable error code of an archive the library already holds under another folder.
pub const ALREADY_IN_LIBRARY: &str = "install.already_in_library";

/// The name a mod's folder starts from: the archive's file name without its extension.
fn archive_base_name(file: &str) -> String {
    Path::new(file)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("mod")
        .to_string()
}
