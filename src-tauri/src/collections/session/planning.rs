use std::collections::BTreeSet;

use super::view::{member_families, session_view};
use crate::collections::inspect;
use crate::collections::model::{
    CollectionInstallRequest, CollectionPosition, CollectionSessionView,
};
use crate::collections::status;
use crate::context::profile::history::save_profiles;
use crate::context::profile::lock_profiles;
use crate::context::{Data, load_library, load_profiles};
use crate::domain::collection::{
    CollectionItem, CollectionItemStatus, CollectionOrigin, CollectionPlan, CollectionSource,
};
use crate::domain::pack::WaitingMod;
use crate::error::AppError;
use crate::games;
use crate::library::LibraryView;
use crate::library::waiting::find_version;

/// Creates or updates the profile of one collection, then returns its live session.
pub fn start(
    root: &Data,
    game_id: &str,
    request: &CollectionInstallRequest,
) -> Result<CollectionSessionView, AppError> {
    let opened = inspect::open(root, game_id, &request.link, request.revision)?;
    start_with_plan(root, game_id, request, opened.plan)
}

/// The profile part of `start`, after the collection plan was opened. Tests call it with a plan and a cache.
pub(in crate::collections) fn start_with_plan(
    root: &Data,
    game_id: &str,
    request: &CollectionInstallRequest,
    plan: CollectionPlan,
) -> Result<CollectionSessionView, AppError> {
    let game = games::load(game_id)?;
    let data = root.game(game_id);
    let library = load_library(&data, &game)?;
    let view = LibraryView::new(&library);
    let selected = selected_keys(&plan, &request.selected);
    if selected.is_empty() {
        return Err(AppError::user("collection.nothing_selected"));
    }
    let excluded: Vec<String> = plan
        .items
        .iter()
        .filter(|item| !selected.contains(&item.key))
        .map(|item| item.key.clone())
        .collect();
    let _profiles = lock_profiles();
    let mut profiles = load_profiles(&data)?;
    let (profile_id, is_new) = if let Some(id) = &request.profile {
        {
            let profile = profiles.get(id)?;
            if profile.pack.is_some() {
                return Err(AppError::user("collection.profile_pack").with("profile", id));
            }
            (id.clone(), false)
        }
    } else {
        let wanted = if request.profile_name.trim().is_empty() {
            plan.info.name.clone()
        } else {
            request.profile_name.trim().to_string()
        };
        let id = profiles.create_unique(&wanted)?;
        if request.activate {
            profiles.switch(&id)?;
        }
        (id, true)
    };
    let profile = profiles.get_mut(&profile_id)?;
    if is_new {
        profile.user_data = game.user_data_defaults();
    }
    let origin_index = profile.collections.iter().position(|origin| {
        origin.provider == plan.info.provider
            && origin.game_key == plan.info.game_key
            && origin.slug == plan.info.slug
    });
    let mut origin = origin_index.map_or_else(
        || CollectionOrigin {
            provider: plan.info.provider.clone(),
            game_key: plan.info.game_key.clone(),
            slug: plan.info.slug.clone(),
            revision: plan.info.revision,
            latest_revision: plan.info.latest_revision,
            revision_label: plan.info.revision_label.clone(),
            name: plan.info.name.clone(),
            author: plan.info.author.clone(),
            url: plan.info.url.clone(),
            excluded: Vec::new(),
            manual: Vec::new(),
            added: Vec::new(),
            families: Vec::new(),
            disabled: Vec::new(),
            installed_at: 0,
        },
        |index| profile.collections[index].clone(),
    );
    origin.revision = plan.info.revision;
    origin.latest_revision = plan.info.latest_revision;
    origin.revision_label.clone_from(&plan.info.revision_label);
    origin.name.clone_from(&plan.info.name);
    origin.author.clone_from(&plan.info.author);
    origin.url.clone_from(&plan.info.url);
    origin.excluded = excluded;
    origin.excluded.sort();
    origin.manual.retain(|key| plan.has_item(key));
    origin
        .added
        .retain(|family| profile.mods.iter().any(|entry| &entry.id == family));
    let previous_added: BTreeSet<&str> = origin.added.iter().map(String::as_str).collect();
    let previous_disabled: BTreeSet<&str> = origin.disabled.iter().map(String::as_str).collect();
    // A new revision can drop a member. Switch off the mods this collection added and the new plan no longer names,
    // but never a mod the user changed by hand or one another collection of the profile still uses. A limited plan
    // has no archive and cannot resolve every family, so it must not switch anything off.
    if !plan.limited {
        let shared: BTreeSet<String> = profile
            .collections
            .iter()
            .enumerate()
            .filter(|(index, _)| Some(*index) != origin_index)
            .flat_map(|(_, other)| other.added.iter().chain(&other.families).cloned())
            .collect();
        let shared: BTreeSet<&str> = shared.iter().map(String::as_str).collect();
        let mut new_members: BTreeSet<String> = BTreeSet::new();
        for item in &plan.items {
            new_members.insert(item.predicted_family());
            if let Some((family, _)) = find_version(&view, &item.wanted()) {
                new_members.insert(family);
            }
        }
        let members: BTreeSet<&str> = new_members.iter().map(String::as_str).collect();
        let change = crate::domain::collection::reconcile_disabled(
            &previous_added,
            &previous_disabled,
            &members,
            &shared,
        );
        for family in &change.disable {
            if let Some(entry) = profile
                .mods
                .iter_mut()
                .find(|entry| entry.id == family.as_str())
            {
                entry.enabled = false;
            }
        }
        for family in &change.enable {
            if let Some(entry) = profile
                .mods
                .iter_mut()
                .find(|entry| entry.id == family.as_str())
            {
                entry.enabled = true;
            }
        }
        origin.disabled = change.next.into_iter().collect();
        profile
            .waiting
            .retain(|waiting| !change.disable.contains(&waiting.entry.family));
    }
    let origin_for_status = origin.clone();
    let existing = profile.clone();
    let before_status = status::statuses(&plan, &library, &existing, Some(&origin_for_status));
    let mut waiting = Vec::new();
    let mut anchor = match request.position {
        CollectionPosition::Top => profile.mods.first().map(|entry| entry.id.clone()),
        CollectionPosition::Bottom => None,
    };
    let ordered: Vec<&CollectionItem> = plan
        .ordered()
        .into_iter()
        .filter(|item| selected.contains(&item.key))
        .collect();
    let mut added = origin.added.clone();
    for (index, item) in ordered.iter().enumerate() {
        let item_status = before_status
            .statuses
            .get(&item.key)
            .cloned()
            .unwrap_or(CollectionItemStatus::Missing);
        if matches!(
            item_status,
            CollectionItemStatus::Installed
                | CollectionItemStatus::Satisfied { .. }
                | CollectionItemStatus::Manual
                | CollectionItemStatus::Skipped
                | CollectionItemStatus::Unsupported
        ) || matches!(
            item.source,
            CollectionSource::Browse { .. } | CollectionSource::Manual
        ) {
            continue;
        }
        let source = item.wanted();
        let accepted = item_status == CollectionItemStatus::InLibrary
            || (matches!(
                item_status,
                CollectionItemStatus::InLibraryDifferentVersion { .. }
            ) && item.update_policy
                != crate::domain::collection::CollectionUpdatePolicy::Exact);
        if accepted && let Some((family, folder)) = find_version(&view, &source) {
            profile.add_mod(&family, &folder);
            if !added.contains(&family) {
                added.push(family);
            }
            continue;
        }
        if profile
            .waiting
            .iter()
            .any(|waiting| waiting.entry.is_same_mod(&source))
        {
            continue;
        }
        let next = ordered[index + 1..]
            .iter()
            .map(|next| next.predicted_family())
            .next()
            .or_else(|| anchor.clone());
        waiting.push(WaitingMod {
            before: next,
            entry: source,
        });
        anchor = None;
    }
    if request.position == CollectionPosition::Top {
        // The waiting chain starts above the old first mod. The final reorder pulls the whole block together.
        if let Some(first) = waiting.first_mut() {
            first.before = profile.mods.first().map(|entry| entry.id.clone());
        }
    }
    profile.waiting.extend(waiting);
    origin.added = added;
    origin.families = member_families(&plan, &view, profile, &origin);
    origin.installed_at = crate::infra::clock::now_seconds();
    if let Some(index) = origin_index {
        profile.collections[index] = origin.clone();
    } else {
        profile.collections.push(origin.clone());
    }
    save_profiles(&data, &profiles)?;
    let profile = profiles.get(&profile_id)?;
    let current = status::statuses(&plan, &library, profile, Some(&origin));
    Ok(session_view(profile_id, origin, plan, current))
}
pub(super) fn selected_keys(plan: &CollectionPlan, selected: &[String]) -> BTreeSet<String> {
    if selected.is_empty() {
        return plan
            .items
            .iter()
            .filter(|item| !item.optional)
            .map(|item| item.key.clone())
            .collect();
    }
    let known: BTreeSet<&str> = plan.items.iter().map(|item| item.key.as_str()).collect();
    selected
        .iter()
        .filter(|key| known.contains(key.as_str()))
        .cloned()
        .collect()
}
