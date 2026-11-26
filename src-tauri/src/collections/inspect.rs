//! Opening a collection: parsing the link, reading the API revision, fetching the archive and building the plan. A
//! preview never changes the profile; it only reads the library and the selected profile.

use std::path::PathBuf;

use super::cache;
use super::manifest;
use super::model::{CollectionPreview, CollectionProfileView};
use super::status;
use crate::context::{Data, load_game};
use crate::domain::collection::{CollectionIssue, CollectionIssueSeverity, CollectionPlan};
use crate::domain::profiles::Profiles;
use crate::domain::source::DownloadRoute;
use crate::error::{AppError, Message};
use crate::providers::{self, CollectionRevision};

/// A collection revision opened for the installer.
pub struct Opened {
    pub plan: CollectionPlan,
    pub archive: Option<PathBuf>,
}

/// Opens `link` for `game_id`.
pub fn inspect(
    root: &Data,
    game_id: &str,
    link_text: &str,
    revision: Option<u32>,
) -> Result<CollectionPreview, AppError> {
    let opened = open(root, game_id, link_text, revision)?;
    let loaded = load_game(root, game_id)?;
    let active = loaded
        .profiles
        .profiles
        .iter()
        .find(|profile| profile.id == loaded.profiles.active)
        .ok_or_else(|| Profiles::not_found(&loaded.profiles.active))?;
    let origin = active.collections.iter().find(|origin| {
        origin.provider == opened.plan.info.provider
            && origin.game_key == opened.plan.info.game_key
            && origin.slug == opened.plan.info.slug
    });
    let status_set = status::statuses(&opened.plan, &loaded.library, active, origin);
    let profiles = loaded
        .profiles
        .profiles
        .iter()
        .map(|profile| CollectionProfileView {
            id: profile.id.clone(),
            name: profile.name.clone(),
            active: profile.id == loaded.profiles.active,
            pack: profile.pack.is_some(),
            collections: u32::try_from(profile.collections.len()).unwrap_or(u32::MAX),
        })
        .collect();
    let source = providers::require(&opened.plan.info.provider)?;
    let key = providers::saved_key(source)?;
    let needs_key = source.credential_account().is_some();
    let account = if needs_key {
        key.as_deref().and_then(|key| source.validate_key(key).ok())
    } else {
        None
    };
    // The preview's mode flag is the same route every update and download reads, not a second rule: it comes from the
    // saved account (whose tier the validation above just refreshed). The window renders this flag; it never decides
    // the mode from a provider name or a capability on its own.
    let direct_download = providers::download_route(source) == DownloadRoute::Direct;
    Ok(CollectionPreview {
        plan: opened.plan,
        statuses: status_set.statuses,
        counts: status_set.counts,
        profiles,
        profile: active.id.clone(),
        has_archive: opened.archive.is_some(),
        key_saved: key.is_some(),
        direct_download,
        needs_key,
        account: account.map(|account| account.name),
        already_installed: origin.is_some(),
    })
}

/// Opens the plan and keeps its archive when one was fetched. Shared by the preview and the starter.
pub fn open(
    root: &Data,
    game_id: &str,
    link_text: &str,
    revision: Option<u32>,
) -> Result<Opened, AppError> {
    let game = crate::games::load(game_id)?;
    let link = providers::collection_link(link_text)
        .ok_or_else(|| AppError::user("collection.bad_link").with("url", link_text))?;
    let game_key = game
        .provider_key(link.provider.as_str())
        .ok_or_else(|| providers::no_game_key(link.provider.as_str()).with("game", game_id))?
        .to_string();
    let source = providers::require(link.provider.as_str())?;
    let collection = providers::collection_provider(link.provider.as_str())
        .ok_or_else(|| providers::unsupported_by(link.provider.as_str()))?;
    let key = providers::saved_key(source)?.unwrap_or_default();
    let revision = collection.revision(&key, &link, revision.or(link.revision))?;
    let info = revision.info.clone();

    if !info.game_key.eq_ignore_ascii_case(&game_key) {
        return Err(AppError::user("collection.game_mismatch")
            .with("expected", &game_key)
            .with("found", info.game_key));
    }

    let data = root.game(game_id);
    let cached = cache::read_plan(&data, &info);
    let mut plan = match cached {
        Some(plan) => plan,
        // The API is the whole recipe: there is no archive to fetch.
        None if revision.complete => manifest::plan_from_api(&revision, &game_key),
        None => match read_or_fetch(&data, &game_key, &info, &revision, &key, collection) {
            Ok(plan) => plan,
            Err(error) => {
                let mut plan = manifest::plan_from_api(&revision, &game_key);
                plan.issues.push(CollectionIssue {
                    severity: CollectionIssueSeverity::Warning,
                    item: None,
                    message: Message::new("collection.limited_preview").with(
                        "detail",
                        error.params.get("detail").cloned().unwrap_or(error.code),
                    ),
                });
                plan
            }
        },
    };
    // The API has the fresh latest revision and changelog even when the plan came from the cache.
    plan.info.latest_revision = revision.latest_revision;
    plan.info.changelog.clone_from(&revision.info.changelog);

    if plan.info.name.trim().is_empty() {
        plan.info.name.clone_from(&revision.info.name);
    }

    if plan.info.summary.trim().is_empty() {
        plan.info.summary.clone_from(&revision.info.summary);
    }

    if plan.info.description.trim().is_empty() {
        plan.info.description.clone_from(&revision.info.description);
    }

    if plan.info.instructions.trim().is_empty() {
        plan.info
            .instructions
            .clone_from(&revision.info.instructions);
    }

    if plan.info.picture_url.is_none() {
        plan.info.picture_url.clone_from(&revision.info.picture_url);
    }

    if plan.info.author.is_none() {
        plan.info.author.clone_from(&revision.info.author);
    }

    if let Some(url) = collection.page_url(&game_key, &plan.info.slug) {
        plan.info.url = url;
    }

    // The cache only saves a fetch next time; the plan in hand is complete without it.
    if let Err(error) = cache::write_plan(&data, &plan) {
        crate::infra::log::warn(format!("could not cache the collection plan: {error}"));
    }

    let archive = cache::archive(&data, &plan.info);
    Ok(Opened { plan, archive })
}

fn read_or_fetch(
    data: &Data,
    game_key: &str,
    info: &crate::domain::collection::CollectionInfo,
    revision: &CollectionRevision,
    key: &str,
    collection: &'static dyn providers::CollectionSource,
) -> Result<CollectionPlan, AppError> {
    if let Some(archive) = cache::archive(data, info)
        && let Ok(plan) = manifest::read_plan(&archive, revision, game_key)
    {
        return Ok(plan);
    }

    let Some(link) = revision.archive_link.as_deref().filter(|_| !key.is_empty()) else {
        return Err(AppError::user("collection.no_archive"));
    };
    let dir = cache::prepare_dir(data, info)?;
    let archive = collection.download_archive(key, game_key, link, &dir, &mut |_, _| true)?;
    manifest::read_plan(&archive, revision, game_key)
}
