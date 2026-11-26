//! The placement of a loader release: a repack that wraps the release in more folders still lands where the original
//! release does.

use crate::domain::library::{SkipReason, SkippedFile};
use crate::domain::pathkey::path_key;

use super::model::Placed;

/// The placement of a loader release: the folder that holds the first program among the package's check files is the
/// release's root, and every file below it goes to the same path under the package's `to` folder. A repack that wraps
/// the release in more folders therefore lands where the original release does. Files outside that root are
/// skipped. `None` when the archive does not hold the program after all.
pub(super) fn loader_layout(
    install: &crate::games::LoaderPackage,
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
pub(super) fn places_every_check(install: &crate::games::LoaderPackage, placed: &[Placed]) -> bool {
    install.check.iter().all(|check| {
        let check = path_key(check);
        placed
            .iter()
            .any(|item| item.slot.is_empty() && path_key(&item.rel) == check)
    })
}
