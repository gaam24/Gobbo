//! Lookups that combine a provider with game data, links or archive names.

use super::port::{CollectionLink, CollectionSource, SourceProvider};
use super::registry::{PROVIDERS, provider, providers};
use super::routing::download_route;
use crate::domain::library::ArchiveLabel;
use crate::domain::source::{SourceProviderInfo, SourceRef};

/// What the settings show for a provider: its id and name, whether it keeps a key and where that key is made, how its
/// downloads are routed, and the schemes of its download links.
pub fn info(provider: &dyn SourceProvider) -> SourceProviderInfo {
    SourceProviderInfo {
        id: provider.id().as_str().to_string(),
        name: provider.display_name().to_string(),
        needs_key: provider.credential_account().is_some(),
        has_key_page: provider.key_page().is_some(),
        direct_download: provider.capabilities().direct_download,
        download_route: download_route(provider),
        collections: provider.capabilities().collections,
        schemes: provider
            .schemes()
            .iter()
            .map(|scheme| (*scheme).to_string())
            .collect(),
    }
}

/// The collection adapter of a registered provider, when it has one.
pub fn collection_provider(id: &str) -> Option<&'static dyn CollectionSource> {
    provider(id)?.collection()
}

/// The first registered collection adapter that recognises `url`. A website URL is checked before the deep links, so
/// a `https://www.nexusmods.com/.../collections/...` link does not fall through to the mod download path.
pub fn collection_link(url: &str) -> Option<CollectionLink> {
    PROVIDERS
        .iter()
        .copied()
        .filter_map(SourceProvider::collection)
        .find_map(|collection| collection.parse_link(url).ok())
}

/// The source the first provider recognises in an archive file name, when any does.
pub fn source_from_archive_name(file_name: &str) -> Option<SourceRef> {
    PROVIDERS
        .iter()
        .copied()
        .find_map(|provider| provider.source_from_archive_name(file_name))
}

/// The page of a mod on `provider_id`, when the game has a key for that provider. The provider owns the URL shape;
/// this is the one place that combines a game key with a provider.
pub fn page_url_for(
    provider_id: &str,
    game_key: Option<&str>,
    mod_id: &str,
    tab: Option<&str>,
) -> Option<String> {
    provider(provider_id)?.page_url(game_key?, mod_id, tab)
}

/// The archive name of a download, read first by the providers the archive came from. When none of them reads it (or
/// none is recorded, as for an archive with no file id), the first registered provider that reads the name answers,
/// as an import names a download (`source_from_archive_name`).
pub fn archive_name_from(provider_ids: &[&str], original_name: &str) -> Option<ArchiveLabel> {
    provider_ids
        .iter()
        .find_map(|provider_id| provider(provider_id)?.archive_name(original_name))
        .or_else(|| {
            PROVIDERS
                .iter()
                .copied()
                .find_map(|provider| provider.archive_name(original_name))
        })
}

/// The provider that owns a deep link, by its scheme. A URL scheme is case-insensitive, so `NXM://` names Nexus too.
pub fn for_link(url: &str) -> Option<&'static dyn SourceProvider> {
    let (scheme, _) = url.split_once("://")?;
    providers().iter().copied().find(|provider| {
        provider
            .schemes()
            .iter()
            .any(|owned| owned.eq_ignore_ascii_case(scheme))
    })
}

/// Whether a URL uses a scheme owned by a registered provider.
pub fn is_source_link(url: &str) -> bool {
    for_link(url).is_some()
}
