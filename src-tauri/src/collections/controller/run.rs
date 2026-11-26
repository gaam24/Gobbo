use std::collections::BTreeSet;
use std::sync::atomic::AtomicBool;

use crate::collections::download;
use crate::collections::model::{CollectionInstallView, CollectionItemAction, CollectionProgress};
use crate::context::Data;
use crate::domain::collection::CollectionSource;
use crate::error::AppError;
use crate::sources::downloads::Downloads;

use super::classify::classify;
use super::model::{CollectionPageRequest, CollectionRoundView};

/// Drives one full automatic round: every member that needs no decision is imported, then the fresh session and the
/// first prompt are returned. Repeating rounds happens here until a prompt, a waiting download or completion; the
/// window calls this once per decision, never in a loop.
pub fn run_round(
    root: &Data,
    game_id: &str,
    profile: &str,
    collection: &str,
    downloads: &Downloads,
    progress: &mut dyn FnMut(CollectionProgress),
    cancel: Option<&AtomicBool>,
) -> Result<CollectionRoundView, AppError> {
    let install =
        download::install_automatic(root, game_id, profile, collection, progress, cancel)?;
    finish_round(root, game_id, profile, collection, downloads, install)
}

/// Turns an automatic install into the round the window renders. A member whose provider file has to be picked from a
/// web page is not offered as a manual decision: its expectation is registered, the first page request is returned for
/// the window to open, and the round waits for the owned download. The explicit pick-file / mark-done / skip prompt is
/// still the path for members that need no provider page.
pub(super) fn finish_round(
    root: &Data,
    game_id: &str,
    profile: &str,
    collection: &str,
    downloads: &Downloads,
    mut install: CollectionInstallView,
) -> Result<CollectionRoundView, AppError> {
    let pages = page_requests(root, game_id, profile, collection, &install, downloads)?;
    let waiting = pages.first().cloned();

    if !pages.is_empty() {
        let waiting_keys: BTreeSet<&str> = pages.iter().map(|page| page.item.as_str()).collect();
        install
            .needs_user
            .retain(|key| !waiting_keys.contains(key.as_str()));
    }

    let mut round = round_view(install);
    round.waiting = waiting;
    Ok(round)
}

/// Registers the download expectation of every member of `view` whose provider needs a web page and returns the page
/// request for each one. The window opens one page at a time; the other expectations are already in place so downloads
/// that arrive in a different order are still owned by their member.
fn page_requests(
    root: &Data,
    game_id: &str,
    profile: &str,
    collection: &str,
    view: &CollectionInstallView,
    downloads: &Downloads,
) -> Result<Vec<CollectionPageRequest>, AppError> {
    let mut pages = Vec::new();

    for key in &view.needs_user {
        let Some(item) = view.session.plan.item(key) else {
            continue;
        };
        let CollectionSource::Provider {
            provider, mod_id, ..
        } = &item.source
        else {
            continue;
        };
        let Some(game_key) = crate::games::load(game_id)?
            .provider_key(provider.as_str())
            .map(str::to_string)
        else {
            continue;
        };
        let action = download::prepare_item(root, game_id, profile, collection, key, downloads)?;
        let CollectionItemAction::Waiting { url, .. } = action else {
            continue;
        };
        pages.push(CollectionPageRequest {
            game: game_id.to_string(),
            profile: profile.to_string(),
            collection: collection.to_string(),
            item: key.clone(),
            name: item.name.clone(),
            provider: provider.to_string(),
            game_key,
            mod_id: mod_id.clone(),
            url,
        });
    }

    Ok(pages)
}

pub(super) fn round_view(install: CollectionInstallView) -> CollectionRoundView {
    let state = classify(&install);
    CollectionRoundView {
        install,
        prompt: state.prompt,
        waiting: None,
        finished: state.finished,
    }
}
