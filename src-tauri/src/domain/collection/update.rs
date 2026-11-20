use std::collections::BTreeSet;

use super::DisabledChange;

/// Reconciles recorded disabled families with the members of a revision.
///
/// `added` holds the families the collection put into the list before the update, `recorded` the ones an earlier
/// update switched off, `members` every family the new revision names and `shared` the families another collection of
/// the same profile still uses. A mod the user switched off by hand is not in `recorded`, so a later update leaves it
/// alone; a mod the curator brings back is switched on again.
pub fn reconcile_disabled<'a>(
    added: &BTreeSet<&'a str>,
    recorded: &BTreeSet<&'a str>,
    members: &BTreeSet<&'a str>,
    shared: &BTreeSet<&'a str>,
) -> DisabledChange {
    let removed: BTreeSet<&'a str> = added.difference(members).copied().collect();
    let candidates: BTreeSet<&'a str> = removed.difference(shared).copied().collect();
    let disable: BTreeSet<String> = candidates
        .difference(recorded)
        .copied()
        .map(ToString::to_string)
        .collect();
    let enable: BTreeSet<String> = recorded
        .intersection(members)
        .copied()
        .map(ToString::to_string)
        .collect();
    let mut next: BTreeSet<String> = recorded.iter().copied().map(ToString::to_string).collect();

    for family in &disable {
        next.insert(family.clone());
    }

    for family in &enable {
        next.remove(family);
    }

    DisabledChange {
        disable,
        enable,
        next,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn families<'a>(items: &'a [&'a str]) -> BTreeSet<&'a str> {
        items.iter().copied().collect()
    }

    #[test]
    fn an_update_switches_off_only_removed_families_no_other_collection_uses() {
        let change = reconcile_disabled(
            &families(&["removed", "kept", "shared"]),
            &families(&[]),
            &families(&["kept", "shared", "fresh"]),
            &families(&["shared"]),
        );
        assert_eq!(change.disable, BTreeSet::from(["removed".to_string()]));
        assert!(change.enable.is_empty());
        assert_eq!(change.next, BTreeSet::from(["removed".to_string()]));
    }

    #[test]
    fn a_re_enabled_mod_is_not_switched_off_by_a_later_update() {
        let change = reconcile_disabled(
            &families(&["removed"]),
            &families(&["removed"]),
            &families(&[]),
            &families(&[]),
        );
        assert!(change.disable.is_empty());
        assert_eq!(change.next, BTreeSet::from(["removed".to_string()]));
    }

    #[test]
    fn a_family_the_new_revision_lists_again_is_switched_back_on() {
        let change = reconcile_disabled(
            &families(&["back"]),
            &families(&["back"]),
            &families(&["back"]),
            &families(&[]),
        );
        assert!(change.disable.is_empty());
        assert_eq!(change.enable, BTreeSet::from(["back".to_string()]));
        assert!(change.next.is_empty());
    }
}
