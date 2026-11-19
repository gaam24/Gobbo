use std::sync::atomic::AtomicBool;

use super::loaded::{LoadedCollection, PreparedDownload};
use super::source::provider_source;
use crate::collections::COLLECTION_CANCELLED;
use crate::collections::status;
use crate::domain::collection::{CollectionItem, CollectionItemStatus, CollectionSource};
use crate::domain::source::DownloadRoute;
use crate::error::AppError;
use crate::infra::tasks::cancelled;
use crate::providers::{self, SourceDownloaded};

/// The concurrency gate on top of the route: a browser-routed account never prefetches, and a provider that caps an
/// account's bandwidth only ever transfers one file at a time. The route itself is the account decision, read from the
/// saved key; `parallel_downloads` only says how many files may be in flight at once.
fn may_prefetch(route: DownloadRoute, parallel_downloads: bool) -> bool {
    route == DownloadRoute::Direct && parallel_downloads
}

/// Whether the collection's provider account may pull several files at once.
pub(super) fn may_download_in_parallel(loaded: &LoadedCollection) -> bool {
    providers::provider(&loaded.origin.provider).is_some_and(|provider| {
        may_prefetch(
            providers::download_route(provider),
            provider.parallel_downloads(),
        )
    })
}

/// Whether a provider member's file must be picked from the provider's web page. A free account cannot fetch the file
/// through the API, so the round leaves the member pending and lets the controller open its page and own the download.
pub(super) fn provider_needs_page(item: &CollectionItem) -> bool {
    let CollectionSource::Provider { provider, .. } = &item.source else {
        return false;
    };
    providers::provider(provider.as_str()).is_some_and(|provider| {
        provider.credential_account().is_some()
            && providers::browser_route_error(provider).is_some()
    })
}

/// Downloads one provider member. `progress` only reports a percentage; it is a no-op in the parallel phase, where
/// the download runs on a scoped thread and cannot borrow the window's callback.
fn fetch_provider_item(
    loaded: &LoadedCollection,
    item: &CollectionItem,
    cancel: Option<&AtomicBool>,
    progress: &mut dyn FnMut(u32),
) -> Result<SourceDownloaded, AppError> {
    let source = provider_source(item)
        .ok_or_else(|| AppError::user("collection.item_missing").with("item", &item.key))?;
    providers::fetch::fetch(
        providers::fetch::Fetch {
            game_key: loaded.game.provider_key(source.provider.as_str()),
            source,
            directory: &loaded.data.downloads(),
            archive_name: item.file_name.as_deref(),
            download_key: None,
        },
        cancel,
        progress,
    )
}

/// Fetches the archives of a paid account's members in small groups, leaving their imports untouched. A worker that
/// panics is ignored: its slot stays empty and the import phase downloads that member normally.
pub(super) fn prefetch_archives(
    loaded: &LoadedCollection,
    selected: &[&CollectionItem],
    statuses: &status::StatusSet,
    cancel: Option<&AtomicBool>,
) -> Vec<Option<PreparedDownload>> {
    let mut ready: Vec<Option<PreparedDownload>> = (0..selected.len()).map(|_| None).collect();
    let indices: Vec<usize> = (0..selected.len())
        .filter(|&index| {
            let item = selected[index];
            matches!(item.source, CollectionSource::Provider { .. })
                && provider_source(item).is_some_and(|source| !loaded.has_library_file(&source))
                && !statuses
                    .statuses
                    .get(&item.key)
                    .is_some_and(CollectionItemStatus::is_done)
        })
        .collect();
    for group in indices.chunks(4) {
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for &index in group {
                let item = selected[index];
                handles.push(scope.spawn(move || {
                    if cancelled(cancel) {
                        return (
                            index,
                            PreparedDownload::Failed(AppError::user(COLLECTION_CANCELLED)),
                        );
                    }
                    let mut no_progress = |_: u32| {};
                    match fetch_provider_item(loaded, item, cancel, &mut no_progress) {
                        Ok(downloaded) => (index, PreparedDownload::Ready(downloaded)),
                        Err(error) => (index, PreparedDownload::Failed(error)),
                    }
                }));
            }
            for handle in handles {
                if let Ok((index, result)) = handle.join() {
                    ready[index] = Some(result);
                }
            }
        });
    }
    ready
}

#[cfg(test)]
mod tests {
    use super::may_prefetch;
    use crate::domain::source::DownloadRoute;

    #[test]
    fn a_browser_route_never_prefetches_even_when_parallel_transfers_are_allowed() {
        assert!(!may_prefetch(DownloadRoute::Browser, true));
    }

    #[test]
    fn prefetching_needs_both_a_direct_route_and_a_parallel_capable_account() {
        assert!(may_prefetch(DownloadRoute::Direct, true));
        assert!(!may_prefetch(DownloadRoute::Direct, false));
    }
}
