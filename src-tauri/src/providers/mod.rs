//! The external mod services, behind one port.
//!
//! A provider maps its API to the generic types in `domain::source`. The library, the profiles and the deployment
//! never learn which service a mod came from; they only see `SourceRef`, `SourceMeta` and `SourceRequirement`.

pub mod download;
mod error;
pub mod fetch;
mod link;
mod lookup;
pub mod nexus;
pub mod picture;
mod port;
mod registry;
mod routing;
#[cfg(test)]
pub mod testing;
#[cfg(test)]
mod tests;
pub mod thunderstore;
mod workers;

use std::sync::atomic::{AtomicUsize, Ordering};

/// How many provider requests run at once during an update check. A push notification or a bulk check keeps the same
/// request budget; a library of many mods just does not wait for one round trip at a time.
pub const UPDATE_WORKERS: usize = 4;

/// The next index of `len` for a worker. `None` when every index was already handed out. The counter is shared, so each
/// index reaches exactly one worker.
pub fn take_index(next: &AtomicUsize, len: usize) -> Option<usize> {
    let index = next.fetch_add(1, Ordering::Relaxed);
    (index < len).then_some(index)
}

pub use error::{PAYMENT_REQUIRED, bad_link, bad_response, no_game_key, unsupported_by};
pub use lookup::{
    archive_name_from, collection_link, collection_provider, for_link, info, is_source_link,
    page_url_for, source_from_archive_name,
};
pub use port::{
    CollectionExternal, CollectionLink, CollectionMod, CollectionRevision,
    CollectionRevisionSummary, CollectionSource, SourceArchive, SourceDownloadRequest,
    SourceDownloaded, SourceProvider, SourceUpdate, SourceUpdateRequest,
};
pub use registry::{provider, providers, require};
pub use routing::{
    browser_route_error, download_route, download_route_for_id, required_key, route_or_browser,
    saved_key,
};
