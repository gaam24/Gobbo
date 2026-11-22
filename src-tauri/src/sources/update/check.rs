//! Update checks and the provider page data they read. The window asks for one game, one mod or a metadata fill; the
//! provider owns the requests and the parsing.

use crate::context::Data;
use crate::context::edit_family;
use crate::domain::library::{Library, LibraryView};
use crate::domain::profiles::Profiles;
use crate::domain::source::ProviderId;
use crate::domain::version::{IgnoredVersion, with_ignored};
use crate::error::AppError;
use crate::games::{self, GameDef};
use crate::providers::{self, SourceArchive, SourceUpdate, SourceUpdateRequest};
use crate::status::views::UpdateReport;

use super::store;

/// Checks every source of the library that its provider can check.
pub fn check_updates(root: &Data, game_id: &str) -> Result<Vec<UpdateReport>, AppError> {
    let (updates, library) = run_checks(root, game_id, None, true)?;
    let view = LibraryView::new(&library);
    let mut reports: Vec<UpdateReport> = updates
        .into_iter()
        .map(|update| report(update, &view))
        .collect();
    reports.sort_by(|a, b| {
        (a.id.as_str(), a.provider.as_str(), a.mod_id.as_str()).cmp(&(
            b.id.as_str(),
            b.provider.as_str(),
            b.mod_id.as_str(),
        ))
    });
    Ok(reports)
}

/// Checks every source of one mod family.
pub fn check_mod_update(root: &Data, game_id: &str, id: &str) -> Result<UpdateReport, AppError> {
    let (updates, library) = run_checks(root, game_id, Some(id), true)?;
    let view = LibraryView::new(&library);
    updates
        .into_iter()
        .next()
        .map(|update| report(update, &view))
        .ok_or_else(|| no_source(id))
}

/// Runs the update checks for `only` (the whole library when it is `None`), stores the results and answers them with
/// the library they were stored in.
fn run_checks(
    root: &Data,
    game_id: &str,
    only: Option<&str>,
    require_key: bool,
) -> Result<(Vec<SourceUpdate>, std::sync::Arc<Library>), AppError> {
    let (game, snapshot) = check_inputs(root, game_id)?;
    let requests = requests(
        &snapshot.data,
        &game,
        &snapshot.library,
        &snapshot.profiles,
        only,
    );
    let updates = run_updates(requests, require_key)?;
    let library = store::store_updates(&snapshot.data, &game, &updates)?;
    Ok((updates, library))
}

/// Reads the page of every source that was never read, or whose picture is not saved yet: name, author, category,
/// description and picture. It runs after an import, so a new mod shows all of it without an update check. Answers how
/// many sources got their page.
pub fn fill_source_info(root: &Data, game_id: &str) -> Result<u32, AppError> {
    let (game, snap) = check_inputs(root, game_id)?;
    let requests: Vec<SourceUpdateRequest> =
        requests(&snap.data, &game, &snap.library, &snap.profiles, None)
            .into_iter()
            // A source is read when it was never read, or when its page names a picture that is not saved yet (an earlier
            // read, or a download that failed): the read saves it, so the list shows it without an update check.
            .filter(|request| {
                request.saved_meta.as_ref().is_none_or(|meta| {
                    meta.checked_at.is_none()
                        || (meta.picture_url.is_some() && meta.picture.is_none())
                })
            })
            .collect();
    if requests.is_empty() {
        return Ok(0);
    }

    let updates = run_updates(requests, false)?;
    let filled = updates
        .iter()
        .filter(|update| update.meta.is_some())
        .count();
    store::store_updates(&snap.data, &game, &updates)?;
    Ok(u32::try_from(filled).unwrap_or(u32::MAX))
}

/// What the checks read of a game as it is on disk now.
struct Snapshot {
    data: Data,
    library: Library,
    profiles: Profiles,
}

fn snapshot(root: &Data, game_id: &str, game: &GameDef) -> Result<Snapshot, AppError> {
    let data = root.game(game_id);
    let library = Library::load(&data.index(), &data.library(), game)?;
    let profiles = crate::context::load_profiles(&data)?;
    Ok(Snapshot {
        data,
        library,
        profiles,
    })
}

fn check_inputs(root: &Data, game_id: &str) -> Result<(GameDef, Snapshot), AppError> {
    let game = games::load(game_id)?;
    let snap = snapshot(root, game_id, &game)?;
    Ok((game, snap))
}

/// Builds one request per source whose provider can check updates and that the game is on. `only` is one family id,
/// or `None` for the whole library; a family outside it is skipped before its saved data is cloned.
fn requests(
    data: &Data,
    game: &GameDef,
    library: &Library,
    profiles: &Profiles,
    only: Option<&str>,
) -> Vec<SourceUpdateRequest> {
    let view = LibraryView::new(library);
    let selected = selected_folders(profiles);
    let images = data.images();
    let mut requests = Vec::new();

    for family in library.families.values() {
        if only.is_some_and(|id| family.id != id) {
            continue;
        }

        for (provider_id, source) in &family.sources {
            let Some(provider) = providers::provider(provider_id.as_str()) else {
                continue;
            };

            if !provider.capabilities().update_check {
                continue;
            }

            let Some(game_key) = game.provider_key(provider_id.as_str()) else {
                continue;
            };
            let selected_version = selected
                .get(family.id.as_str())
                .and_then(|folder| library.versions.get(*folder))
                .or_else(|| view.default_version(&family.id));
            let current = selected_version.and_then(|version| version.info.version.clone());
            let current_file_id = selected_version
                .and_then(|version| view.file_id_of(version, provider_id.as_str()))
                .map(str::to_string);
            let archives = view
                .versions_of(&family.id)
                .iter()
                .filter_map(|version| {
                    let sha256 = version.archive.clone()?;
                    let archive = library.archives.get(&sha256)?;
                    Some(SourceArchive {
                        sha256,
                        original_name: archive.original_name.clone(),
                        file_id: archive.source_files.get(provider_id).cloned(),
                        part_id: archive.source_parts.get(provider_id).cloned(),
                    })
                })
                .collect();
            requests.push(SourceUpdateRequest {
                family: family.id.clone(),
                source: source.source(provider_id),
                game_key: game_key.to_string(),
                current,
                current_file_id,
                pictures_dir: images.clone(),
                saved_meta: source.meta.clone(),
                saved_sync: family.source_sync.get(provider_id).cloned(),
                archives,
            });
        }
    }

    requests
}

/// Runs the requests by provider. With `require_key`, a provider whose key is missing reports the key error on each of
/// its sources; without it the provider is skipped, which is what the metadata fill does for a game whose key is not
/// configured.
fn run_updates(
    requests: Vec<SourceUpdateRequest>,
    require_key: bool,
) -> Result<Vec<SourceUpdate>, AppError> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }

    let mut groups: std::collections::BTreeMap<String, Vec<SourceUpdateRequest>> =
        std::collections::BTreeMap::new();
    for request in requests {
        groups
            .entry(request.source.provider.to_string())
            .or_default()
            .push(request);
    }

    let mut updates = Vec::new();

    for (provider_id, requests) in groups {
        let provider = providers::require(&provider_id)?;

        if !provider.capabilities().update_check {
            continue;
        }

        let key = if require_key {
            // A missing key fails this provider's mods with the key error the window turns into "save your key"; the
            // other providers of the game are still checked.
            match providers::required_key(provider) {
                Ok(key) => key,
                Err(error) => {
                    updates.extend(requests.iter().map(|request| SourceUpdate {
                        error: Some(error.clone()),
                        ..SourceUpdate::new(request)
                    }));
                    continue;
                }
            }
        } else {
            let Some(key) = providers::saved_key(provider)? else {
                continue;
            };
            key
        };
        updates.extend(provider.check_updates(&key, &requests));
    }

    Ok(updates)
}

/// The version each family selects in the active profile. A lookup reads it once instead of scanning the whole
/// profile for every family.
fn selected_folders(profiles: &Profiles) -> rustc_hash::FxHashMap<&str, &str> {
    let mut selected = rustc_hash::FxHashMap::default();

    for entry in &profiles.active().mods {
        // The first entry wins, matching the linear lookup this map replaced.
        selected
            .entry(entry.id.as_str())
            .or_insert(entry.folder.as_str());
    }

    selected
}

fn report(update: SourceUpdate, view: &LibraryView<'_>) -> UpdateReport {
    let latest_file = update.latest_file_id.clone();
    let state = with_ignored(
        update.state,
        update.latest.as_deref(),
        latest_file.as_deref(),
        view.ignored_version(&update.family, &update.source.provider),
    );
    UpdateReport {
        id: update.family,
        provider: update.source.provider.to_string(),
        mod_id: update.source.mod_id,
        current: update.current,
        latest: update.latest,
        latest_file_id: latest_file.clone(),
        state,
        latest_installed: false,
        installed_folder: None,
        has_update: false,
        error: update.error,
    }
    .with_library(view, latest_file.as_deref())
}

fn no_source(id: &str) -> AppError {
    crate::context::library::not_in_library(id)
}

/// Remembers that the user does not want to be reminded about the version the provider lists now. A check that
/// finds another version drops the entry, so the reminder returns.
pub fn ignore_version(
    root: &Data,
    game_id: &str,
    id: &str,
    provider: &str,
) -> Result<(), AppError> {
    edit_family(root, game_id, id, |family| {
        let provider = ProviderId::new(provider);
        let latest = family
            .latest
            .get(&provider)
            .cloned()
            .or_else(|| {
                let source = family.sources.get(&provider)?;

                if source.part_id.is_some() {
                    return None;
                }

                source.meta.as_ref().and_then(|meta| meta.version.clone())
            })
            .ok_or_else(|| {
                AppError::user("source.no_version")
                    .with("id", id)
                    .with("provider", provider.as_str())
            })?;
        let file_id = family.latest_file_ids.get(&provider).cloned();
        family.ignored_versions.insert(
            provider,
            IgnoredVersion {
                version: latest,
                file_id,
            },
        );
        Ok(())
    })
}

/// Shows the ignored version again.
pub fn unignore_version(
    root: &Data,
    game_id: &str,
    id: &str,
    provider: &str,
) -> Result<(), AppError> {
    edit_family(root, game_id, id, |family| {
        family.ignored_versions.remove(&ProviderId::new(provider));
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::library::Family;
    use crate::domain::source::SourceMod;
    use crate::test_support::{profile, profiles, scratch};
    use std::collections::BTreeMap;

    fn family(id: &str, mod_id: &str) -> Family {
        Family {
            id: id.to_string(),
            sources: BTreeMap::from([(
                ProviderId::new("nexus"),
                SourceMod {
                    mod_id: mod_id.to_string(),
                    ..SourceMod::default()
                },
            )]),
            ..Family::default()
        }
    }

    #[test]
    fn a_targeted_request_builds_only_that_family() {
        let data = Data::new(scratch!("update-requests"));
        let game = GameDef::parse(crate::test_support::games::SLOT).unwrap();
        let mut library = Library::default();
        library
            .families
            .insert("nexus:1".into(), family("nexus:1", "10"));
        library
            .families
            .insert("nexus:2".into(), family("nexus:2", "20"));
        let profiles = profiles("default", vec![profile("default", &[])]);
        let all = requests(&data, &game, &library, &profiles, None);
        assert_eq!(
            all.iter()
                .map(|request| request.family.as_str())
                .collect::<Vec<_>>(),
            ["nexus:1", "nexus:2"]
        );
        let one = requests(&data, &game, &library, &profiles, Some("nexus:2"));
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].family, "nexus:2");
        assert_eq!(one[0].source.mod_id, "20");
    }
}
