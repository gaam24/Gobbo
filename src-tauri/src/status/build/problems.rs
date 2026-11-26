//! Problems the disk and the index disagree about.

use crate::context::library::FolderCheck;
use crate::domain::library::Library;
use crate::domain::profiles::entry::ProfileMod;
use crate::error::Message;

/// Problems the disk and the index disagree about: a folder the index lost or never saw, and a mod of the profile that
/// is in neither place.
pub(super) fn index_problems(
    library: &Library,
    listed: &[ProfileMod],
    check: &FolderCheck,
) -> Vec<Message> {
    let mut problems: Vec<Message> = check
        .missing
        .iter()
        .map(|folder| Message::new("status.index_folder_missing").with("folder", folder))
        .collect();
    problems.extend(
        check
            .unindexed
            .iter()
            .map(|folder| Message::new("status.folder_not_indexed").with("folder", folder)),
    );

    for entry in listed {
        if !library.versions.contains_key(&entry.folder) && !check.on_disk.contains(&entry.folder) {
            problems.push(Message::new(crate::error::MOD_MISSING).with("folder", &entry.folder));
        }
    }

    problems
}
