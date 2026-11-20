use std::collections::BTreeSet;

use super::view::session_view;
use crate::collections::cache;
use crate::collections::model::CollectionSessionView;
use crate::collections::status;
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::context::{Data, load_library, load_profiles};
use crate::domain::collection::CollectionItem;
use crate::error::AppError;
use crate::games;

/// Removes a collection from a profile. Only the families this collection added and no other collection still uses are
/// removed; the library and the archives stay.
pub fn remove(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    collection_key: &str,
) -> Result<CollectionSessionView, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = load_library(&data, &game)?;
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    let plan = {
        let profile = profiles.get(profile_id)?;
        let origin = profile
            .collections
            .iter()
            .find(|origin| origin.key() == collection_key)
            .ok_or_else(|| AppError::user("collection.not_in_profile"))?;
        cache::read_plan_or_missing(&data, origin)?
    };
    let profile = profiles.get_mut(profile_id)?;
    let Some(index) = profile
        .collections
        .iter()
        .position(|origin| origin.key() == collection_key)
    else {
        return Err(AppError::user("collection.not_in_profile"));
    };
    let origin = profile.collections.remove(index);
    let shared: BTreeSet<String> = profile
        .collections
        .iter()
        .flat_map(|other| other.added.iter().chain(&other.families).cloned())
        .collect();
    let removed: BTreeSet<String> = origin
        .added
        .iter()
        .filter(|family| !shared.contains(*family))
        .cloned()
        .collect();
    profile.mods.retain(|entry| !removed.contains(&entry.id));
    // A waiting entry of this collection's member goes away; a different mod that only shares a display name stays.
    // `wanted()` turns the members into the `PackMod`s the waiting rule compares, so the file id decides when both
    // sides name one, instead of a loose name match.
    let wanted: Vec<_> = plan.items.iter().map(CollectionItem::wanted).collect();
    profile.waiting.retain(|waiting| {
        if removed.contains(&waiting.entry.family) {
            return false;
        }

        !wanted.iter().any(|item| item.is_same_mod(&waiting.entry))
    });
    save_profiles(&data, &profiles)?;
    let profile = profiles.get(profile_id)?.clone();
    let current = status::statuses(&plan, &library, &profile, None);
    Ok(session_view(profile_id.to_string(), origin, plan, current))
}
