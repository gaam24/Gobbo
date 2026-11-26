//! How a provider's downloads are routed, and the keys the route reads.

use super::error::{PAYMENT_REQUIRED, unsupported_by};
use super::port::SourceProvider;
use super::registry::provider;
use crate::domain::source::DownloadRoute;
use crate::error::{AppError, KEY_MISSING};
use crate::infra::credentials;

/// The provider's saved key. A provider that keeps no key answers an empty one; `None` when it keeps one and the user
/// has not saved it.
pub fn saved_key(provider: &dyn SourceProvider) -> Result<Option<String>, AppError> {
    let Some(account) = provider.credential_account() else {
        return Ok(Some(String::new()));
    };
    credentials::load(account)
}

/// The provider's saved key, or the error that asks the user to save it.
pub fn required_key(provider: &dyn SourceProvider) -> Result<String, AppError> {
    saved_key(provider)?.ok_or_else(|| {
        AppError::user(KEY_MISSING)
            .with("provider", provider.id().as_str())
            .recovery(
                crate::error::Recovery::new(crate::error::RecoveryAction::SaveProviderKey)
                    .target(provider.id().as_str()),
            )
    })
}

/// Where a provider's files are downloaded. This is the one rule every update, "update all", collection and download
/// button follows:
///
/// - `Browser` when the provider has no direct download, when it needs a key and none is saved, or when the saved
///   account is known to have no direct download (a free Nexus account, or one that just answered `payment_required`).
/// - `Direct` otherwise. An account nothing is known about yet tries a direct download once; its answer then decides.
pub fn download_route(provider: &dyn SourceProvider) -> DownloadRoute {
    let (key_missing, known) = match saved_key(provider) {
        Ok(Some(key)) => (false, provider.known_direct_download(&key)),
        Ok(None) => (true, None),
        // A keyring that cannot be read is left to the download itself, which reports the error.
        Err(_) => (false, None),
    };
    route_from(provider.capabilities().direct_download, key_missing, known)
}

/// The download route of the provider with this id, or `Browser` when this build does not know the id. An unknown
/// provider has no direct downloads, exactly like a known one whose account is not entitled.
pub fn download_route_for_id(id: &str) -> DownloadRoute {
    route_or_browser(provider(id).map(download_route))
}

/// The route a batch of rows uses when its one-time route map has no entry for a provider. This is the same `Browser`
/// fallback as [`download_route_for_id`], so every map reader and an unknown id agree on one rule.
pub fn route_or_browser(route: Option<DownloadRoute>) -> DownloadRoute {
    route.unwrap_or(DownloadRoute::Browser)
}

/// The routing rule over the facts it reads: the provider's download capability, whether a needed key is missing, and
/// what the saved account is known to be able to do.
pub(super) const fn route_from(
    can_download: bool,
    key_missing: bool,
    known_direct: Option<bool>,
) -> DownloadRoute {
    if !can_download || key_missing {
        return DownloadRoute::Browser;
    }

    match known_direct {
        Some(false) => DownloadRoute::Browser,
        Some(true) | None => DownloadRoute::Direct,
    }
}

/// The stable error that explains a browser route to the window, or `None` when the account downloads directly. A
/// provider without the download capability or without a saved key keeps its own code; a saved account known to have no
/// direct download answers `payment_required`, which the window handles by opening the provider's files page. Callers
/// that just need the route read [`download_route`]; this is the mapping for a site that was about to download.
pub fn browser_route_error(provider: &dyn SourceProvider) -> Option<AppError> {
    if download_route(provider) == DownloadRoute::Direct {
        return None;
    }

    if !provider.capabilities().direct_download {
        return Some(unsupported_by(provider.id().as_str()));
    }

    if let Err(error) = required_key(provider) {
        return Some(error);
    }

    Some(AppError::user(PAYMENT_REQUIRED))
}
