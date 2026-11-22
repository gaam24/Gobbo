use std::collections::{BTreeMap, BTreeSet};

use super::view::{member_families, session_view};
use crate::collections::cache;
use crate::collections::model::CollectionSessionView;
use crate::collections::status;
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::context::{Data, load_game, load_library, load_profiles};
use crate::domain::collection::CollectionItem;
use crate::domain::library::LibraryView;
use crate::domain::profiles::{Entry as ProfileEntry, Profiles};
use crate::error::AppError;
use crate::games;
use crate::library::waiting::find_version;

/// The live status of one collection in one profile.
pub fn status(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    collection_key: &str,
) -> Result<CollectionSessionView, AppError> {
    let loaded = load_game(root, game_id)?;
    let profile = loaded
        .profiles
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| Profiles::not_found(profile_id))?;
    let origin = profile
        .collections
        .iter()
        .find(|origin| origin.key() == collection_key)
        .ok_or_else(|| {
            AppError::user("collection.not_in_profile").with("collection", collection_key)
        })?;
    let plan = cache::read_plan_or_missing(&loaded.data, origin)
        .map_err(|error| error.with("slug", &origin.slug))?;
    let current = status::statuses(&plan, &loaded.library, profile, Some(origin));
    Ok(session_view(
        profile.id.clone(),
        origin.clone(),
        plan,
        current,
    ))
}

/// Permanently skips a member. A temporary skip is the window's queue order, not a profile change.
pub fn skip(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    collection_key: &str,
    item_key: &str,
    permanent: bool,
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
        let plan = cache::read_plan_or_missing(&data, origin)?;

        if !plan.has_item(item_key) {
            return Err(AppError::user("collection.item_missing").with("item", item_key));
        }

        plan
    };
    let item = plan
        .item(item_key)
        .ok_or_else(|| AppError::user("collection.item_missing").with("item", item_key))?
        .clone();
    let profile = profiles.get_mut(profile_id)?;
    let origin = profile
        .collections
        .iter_mut()
        .find(|origin| origin.key() == collection_key)
        .ok_or_else(|| AppError::user("collection.not_in_profile"))?;
    if permanent {
        if !origin.excluded.iter().any(|key| key == item_key) {
            origin.excluded.push(item_key.to_string());
        }

        let wanted = item.wanted();
        profile
            .waiting
            .retain(|waiting| !waiting.entry.is_same_mod(&wanted));
    } else {
        origin.excluded.retain(|key| key != item_key);
    }

    let origin = origin.clone();
    save_profiles(&data, &profiles)?;
    let profile = profiles.get(profile_id)?.clone();
    let current = status::statuses(&plan, &library, &profile, Some(&origin));
    Ok(session_view(profile_id.to_string(), origin, plan, current))
}

/// Closes an install: settles waiting members that are now in the library, records the families the collection added,
/// and puts the collection block in plan order. One profile edit, one history point.
pub fn finish(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    collection_key: &str,
) -> Result<CollectionSessionView, AppError> {
    finish_with_manual(root, game_id, profile_id, collection_key, &[])
}

/// `finish` with the members that were completed without a Nexus source. The keys are stored in the origin so the
/// panel keeps them done after a restart.
pub fn finish_with_manual(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    collection_key: &str,
    manual: &[String],
) -> Result<CollectionSessionView, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = load_library(&data, &game)?;
    let view = LibraryView::new(&library);
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
    let mut origin = profile
        .collections
        .iter()
        .find(|origin| origin.key() == collection_key)
        .ok_or_else(|| AppError::user("collection.not_in_profile"))?
        .clone();
    for key in manual {
        if plan.has_item(key) && !origin.manual.iter().any(|known| known == key) {
            origin.manual.push(key.clone());
        }
    }

    origin.manual.sort();
    origin.manual.dedup();
    let mut added: BTreeSet<String> = origin.added.iter().cloned().collect();
    // Waiting entries that are now in the library join the list. This is the only place the batch and the guided
    // flow share: neither writes the list per member.
    added.extend(crate::library::waiting::settle_waiting(profile, &view));
    // A member already in the library (for example imported by a previous attempt) joins the list without a download.
    for item in plan.ordered() {
        if origin.excluded.iter().any(|key| key == &item.key) {
            continue;
        }

        let Some((family, folder)) = find_version(&view, &item.wanted()) else {
            continue;
        };

        if !collection_file_is_exact(item, &view, &folder) {
            continue;
        }

        if let Some(entry) = profile.mods.iter_mut().find(|entry| entry.id == family) {
            if item.update_policy == crate::domain::collection::CollectionUpdatePolicy::Exact
                && entry.folder != folder
            {
                entry.folder = folder;
            }
        } else {
            // Only a family this collection puts into the list is its own. A mod the list already had stays when the
            // collection is removed.
            added.insert(family.clone());
            profile.add_mod(&family, &folder);
        }
    }
    // Keep the block in plan order. Non-collection entries keep their places.
    let mut index: BTreeMap<String, usize> = BTreeMap::new();

    for (position, item) in plan.ordered().iter().enumerate() {
        if let Some((family, folder)) = find_version(&view, &item.wanted())
            && collection_file_is_exact(item, &view, &folder)
        {
            index.entry(family).or_insert(position);
        }
    }

    let mut collection_entries: Vec<ProfileEntry> = profile
        .mods
        .iter()
        .filter(|entry| added.contains(&entry.id) && index.contains_key(&entry.id))
        .cloned()
        .collect();
    if !collection_entries.is_empty() {
        let first = profile
            .mods
            .iter()
            .position(|entry| added.contains(&entry.id) && index.contains_key(&entry.id))
            .unwrap_or(profile.mods.len());
        profile
            .mods
            .retain(|entry| !(added.contains(&entry.id) && index.contains_key(&entry.id)));
        collection_entries.sort_by_key(|entry| index.get(&entry.id).copied().unwrap_or(usize::MAX));
        let at = first.min(profile.mods.len());

        for (offset, entry) in collection_entries.into_iter().enumerate() {
            profile.mods.insert(at + offset, entry);
        }
    }

    origin.added = added.into_iter().collect();
    origin.families = member_families(&plan, &view, profile, &origin);
    origin.installed_at = crate::infra::clock::now_seconds();

    if let Some(index) = profile
        .collections
        .iter()
        .position(|known| known.key() == collection_key)
    {
        profile.collections[index] = origin.clone();
    }

    save_profiles(&data, &profiles)?;
    let profile = profiles.get(profile_id)?.clone();
    let current = status::statuses_with_view(&plan, &library, &view, &profile, Some(&origin));
    Ok(session_view(profile_id.to_string(), origin, plan, current))
}
/// Marks a member done without a library source, for a manual or browse resource the user installed by hand.
pub fn mark_done(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    collection_key: &str,
    item: &str,
) -> Result<CollectionSessionView, AppError> {
    finish_with_manual(
        root,
        game_id,
        profile_id,
        collection_key,
        &[item.to_string()],
    )
}

fn collection_file_is_exact(item: &CollectionItem, view: &LibraryView<'_>, folder: &str) -> bool {
    if item.update_policy != crate::domain::collection::CollectionUpdatePolicy::Exact {
        return true;
    }

    let Some((provider, _, file_id)) = item.provider_file() else {
        return true;
    };
    view.version(folder)
        .is_some_and(|version| view.file_id_of(version, provider) == Some(file_id))
}
