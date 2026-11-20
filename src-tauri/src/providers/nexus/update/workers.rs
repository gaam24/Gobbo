//! The worker pool of a Nexus update check: page and picture first, one GraphQL facet batch per game, then the
//! requirements of the files that need them.

use super::lookup::Lookup;
use super::requirements::{listed_requirements, match_file};
use super::{Listed, page_meta, requirements};
use crate::domain::version::update_state;
use crate::infra::clock::now_seconds;
use crate::providers::SourceUpdate;
use crate::providers::nexus::api::NexusClient;
use crate::providers::nexus::files::{ModFile, file_version, newest_part_file, part_files};
use crate::providers::nexus::{ModPage, mod_details};
use crate::providers::picture::{save_picture, stem};
use std::collections::BTreeMap;
use std::sync::Mutex;

#[cfg(test)]
pub(super) use crate::providers::take_index;
use crate::providers::workers::{WORKER_PANIC, per_request};

/// One page and its picture, shared by all parts of that page during a check.
type PageRead = (
    Option<ModPage>,
    Option<crate::error::AppError>,
    Option<String>,
);
type PageCache = BTreeMap<u32, PageRead>;

/// One lookup and what reading it brought so far.
struct Work {
    item: Lookup,
    mod_uid: Option<String>,
    page: Option<ModPage>,
    error: Option<crate::error::AppError>,
    picture: Option<String>,
    part_latest: Option<ModFile>,
    matched: Option<(String, u32, Option<String>)>,
    requires: Option<Listed>,
}

/// Looks every family up: the page and picture first, then one GraphQL facet batch, then the requirements of the files
/// that need them. The requests run a few at a time, with the client's own rate limit.
pub(super) fn look_up_all(
    client: &NexusClient,
    domain: &str,
    lookups: &[Lookup],
) -> Vec<SourceUpdate> {
    // Several parts of one page share its data and picture; the page is read once per check.
    let pages: Mutex<PageCache> = Mutex::new(BTreeMap::new());
    let mut work: Vec<Work> = per_request(
        lookups,
        |lookup| {
            let mut item = lookup.clone();
            let known = pages
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&item.nexus_id)
                .cloned();
            let (page, error, picture) = known.unwrap_or_else(|| {
                let (page, error) = match mod_details(client, domain, item.nexus_id) {
                    Ok(page) => (Some(page), None),
                    Err(error) => (None, Some(error)),
                };
                let picture = page.as_ref().and_then(|page| {
                    save_picture(
                        &item.pictures_dir,
                        &stem("nexus", &item.nexus_id.to_string()),
                        page.picture_url.as_deref()?,
                        item.picture_url.as_deref(),
                    )
                });
                let entry = (page, error, picture);
                pages
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .insert(item.nexus_id, entry.clone());
                entry
            });
            let mod_uid = page.as_ref().and_then(|page| page.uid.clone());
            let matched = if item.file_id.is_none() {
                match_file(client, &item, mod_uid.as_deref())
            } else {
                None
            };
            let mut part = item
                .part_id
                .map(|id| id.to_string())
                .or_else(|| matched.as_ref().and_then(|(_, _, part)| part.clone()));
            // A library without the part can still name the file. Asking Nexus for that file gives the
            // update chain, so the check does not fall back to the version string for such an install.
            if part.is_none() {
                let file_id = item.current_file_id.or(item.file_id);
                part = file_id
                    .and_then(|file_id| file_version(client, domain, file_id).ok())
                    .and_then(|file| file.mod_file_id);
                if let Some(id) = part.as_deref().and_then(|part| part.parse().ok()) {
                    item.part_id = Some(id);
                }
            }

            let part_latest = part
                .as_deref()
                .and_then(|part| part_files(client, part).ok())
                .and_then(|files| newest_part_file(&files).cloned());
            Work {
                item,
                mod_uid,
                page,
                error,
                picture,
                part_latest,
                matched,
                requires: None,
            }
        },
        lost_work,
    );

    // One facet batch per game. The same game-scoped id is only unique inside its game.
    let mut by_game: BTreeMap<u32, Vec<u32>> = BTreeMap::new();

    for work in &work {
        if let Some(game_id) = work.page.as_ref().and_then(|page| page.game_id) {
            by_game.entry(game_id).or_default().push(work.item.nexus_id);
        }
    }

    let mut facets = BTreeMap::new();

    for (game_id, ids) in by_game {
        facets.extend(crate::providers::nexus::api::facets(client, game_id, &ids));
    }

    for work in &mut work {
        let Some(page) = work.page.as_mut() else {
            continue;
        };
        let Some(facet) = facets.get(&work.item.nexus_id) else {
            continue;
        };
        page.summary.clone_from(&facet.summary);
        page.updated_at = facet.updated_at;
        page.created_at = facet.created_at;
    }

    // A requirement read whose worker stopped is missing, as if Nexus listed none; the other reads are kept.
    let listed = per_request(
        &work,
        |work| {
            let file_id = work
                .part_latest
                .as_ref()
                .map(|file| file.file_id)
                .or(work.item.file_id)
                .or_else(|| work.matched.as_ref().map(|(_, file_id, _)| *file_id));
            listed_requirements(
                client,
                domain,
                &work.item,
                work.mod_uid.as_deref(),
                file_id,
                facets.get(&work.item.nexus_id),
            )
        },
        |_| None,
    );

    for (work, requires) in work.iter_mut().zip(listed) {
        work.requires = requires;
    }

    work.into_iter().map(source_update).collect()
}

/// The work of a lookup whose worker stopped: its family gets the worker error and nothing else was read.
fn lost_work(lookup: &Lookup) -> Work {
    Work {
        item: lookup.clone(),
        mod_uid: None,
        page: None,
        error: Some(crate::error::AppError::unexpected(WORKER_PANIC)),
        picture: None,
        part_latest: None,
        matched: None,
        requires: None,
    }
}

fn source_update(work: Work) -> SourceUpdate {
    let latest = if work.item.part_id.is_some() || work.part_latest.is_some() {
        work.part_latest.as_ref().map(|file| {
            file.version
                .clone()
                .filter(|version| !version.trim().is_empty())
                .unwrap_or_else(|| file.file_name.clone())
        })
    } else {
        work.page.as_ref().map(|page| page.version.clone())
    };
    let latest_file = work.part_latest.as_ref().map(|file| file.file_id);
    let latest_file_id = latest_file.map(|file| file.to_string());
    let current_file_id = work.item.current_file_id.map(|file| file.to_string());
    let state = update_state(
        latest.as_deref(),
        latest_file_id.as_deref(),
        work.item.current.as_deref(),
        current_file_id.as_deref(),
    );
    let meta = work.page.as_ref().map(|page| {
        page_meta(
            work.item.saved_meta.as_ref(),
            page,
            now_seconds(),
            work.picture,
        )
    });
    let (requires, dlcs, sync) = requirements(work.requires);
    let (matched, matched_part) = match work.matched {
        Some((digest, file_id, part)) => (
            Some((digest.clone(), file_id.to_string())),
            part.map(|part| (digest, part)),
        ),
        None => (None, None),
    };
    let mut source = work.item.source;

    if source.part_id.is_none() {
        source.part_id = work.item.part_id.map(|id| id.to_string());
    }

    SourceUpdate {
        family: work.item.family,
        source,
        current: work.item.current,
        latest,
        latest_file_id,
        state,
        error: work.error,
        meta,
        requirements: requires,
        dlcs,
        sync,
        matched,
        matched_part,
    }
}
