use std::collections::BTreeSet;

use crate::collections::model::CollectionSessionView;
use crate::collections::status;
use crate::domain::collection::{CollectionOrigin, CollectionPlan};
use crate::domain::library::LibraryView;
use crate::library::waiting::find_version;

/// Every family of the profile list that is a member of the collection: the ones it added and the ones the list
/// already had. Excluded members do not count.
pub(super) fn member_families(
    plan: &CollectionPlan,
    view: &LibraryView<'_>,
    profile: &crate::domain::profiles::Profile,
    origin: &CollectionOrigin,
) -> Vec<String> {
    let listed: BTreeSet<&str> = profile.mods.iter().map(|entry| entry.id.as_str()).collect();
    let mut families: BTreeSet<String> = origin
        .added
        .iter()
        .filter(|family| listed.contains(family.as_str()) && !origin.disabled.contains(*family))
        .cloned()
        .collect();
    for item in &plan.items {
        if origin.excluded.iter().any(|key| key == &item.key) {
            continue;
        }

        if let Some((family, _)) = find_version(view, &item.wanted())
            && listed.contains(family.as_str())
        {
            families.insert(family);
        }
    }

    families.into_iter().collect()
}

pub(super) fn session_view(
    profile: String,
    origin: CollectionOrigin,
    plan: CollectionPlan,
    status_set: status::StatusSet,
) -> CollectionSessionView {
    let next = status_set.next(&plan).map(|item| item.key.clone());
    let update = origin.has_update();
    CollectionSessionView {
        profile,
        origin,
        plan,
        statuses: status_set.statuses,
        counts: status_set.counts,
        next,
        update,
    }
}
