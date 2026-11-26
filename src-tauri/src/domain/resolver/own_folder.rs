//! Which mods get a folder of their own in a slot. The game reads a slot's mods from those folders, so the load order
//! number of a mod reaches the game only when the mod has one. The planner writes the order rows, and the load order
//! window offers only the mods `is_orderable` accepts; both call the rules here, so the window never offers a number
//! that the planner would drop.

use super::model::ModInput;
use crate::domain::deploy::{Deploy, Slot};
use crate::domain::layout::Materialized;

/// Whether a mod in `slot` gets a folder of its own there: the slot is not flat, and the mod does not only keep game
/// paths, since those files go to their game paths and not into a folder of the mod.
pub(super) const fn has_own_folder(slot: &Slot, mirrored_only: bool) -> bool {
    !slot.flat && !mirrored_only
}

/// Whether every file of the mod keeps its game path, so the mod has no folder of its own. The library's layout answers
/// it once the library has one; a library built in memory scans the file list.
pub(super) fn mirrored_only_of(deploy: &Deploy, input: &ModInput<'_>) -> bool {
    match input.layout {
        Some(profile) if profile.materialized != Materialized::Unknown => profile.mirrored_only,
        _ => deploy.mirrored_only(&input.files),
    }
}

/// Whether the game reads the mod's number from its order file, so the load order window may give the mod one. The
/// planner writes an order row for a mod with a priority only when this holds. A mod without a slot, a mod in a flat
/// slot and a mod whose files all keep their game paths have no folder for the game to read.
pub fn is_orderable(deploy: &Deploy, input: &ModInput<'_>) -> bool {
    deploy
        .slot(&input.slot)
        .is_some_and(|slot| has_own_folder(slot, mirrored_only_of(deploy, input)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::games as test_games;
    use std::borrow::Cow;
    use std::sync::Arc;

    /// A mod in `slot` (empty for a mod without one) whose files are `files`.
    fn mod_in<'a>(slot: &'a str, files: &[&str]) -> ModInput<'a> {
        ModInput {
            id: Cow::Borrowed("local:mod-a"),
            folder: Cow::Borrowed("mod-a"),
            slot: Cow::Borrowed(slot),
            display_name: Cow::Borrowed("mod-a"),
            enabled: true,
            priority: None,
            files: Arc::from(files.iter().map(ToString::to_string).collect::<Vec<_>>()),
            layout: None,
        }
    }

    #[test]
    fn a_mod_in_a_folder_slot_has_a_folder_of_its_own_and_is_orderable() {
        let deploy = test_games::slot().deploy;
        assert!(is_orderable(&deploy, &mod_in("Mods", &["content/a.ws"])));
    }

    #[test]
    fn a_mod_with_some_files_in_its_own_folder_is_orderable() {
        let deploy = test_games::slot().deploy;
        let files = ["DLC/dlc-a/blob0.bundle", "content/a.ws"];
        assert!(is_orderable(&deploy, &mod_in("Mods", &files)));
    }

    #[test]
    fn a_mod_in_a_flat_slot_has_no_folder_and_is_not_orderable() {
        let mut deploy = test_games::slot().deploy;
        deploy.slots[0].flat = true;
        assert!(!is_orderable(&deploy, &mod_in("Mods", &["content/a.ws"])));
    }

    #[test]
    fn a_mod_of_mirrored_files_only_has_no_folder_and_is_not_orderable() {
        let deploy = test_games::slot().deploy;
        assert!(!is_orderable(
            &deploy,
            &mod_in("Mods", &["DLC/dlc-a/blob0.bundle"])
        ));
    }

    #[test]
    fn a_mod_without_a_slot_is_not_orderable() {
        let deploy = test_games::slot().deploy;
        assert!(!is_orderable(&deploy, &mod_in("", &["content/a.ws"])));
    }
}
