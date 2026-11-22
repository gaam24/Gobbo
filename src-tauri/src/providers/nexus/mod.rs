//! Nexus Mods as a provider: the `SourceProvider` adapter and its client. The client owns the API, the page and file
//! data and the download primitives. The queue is shared with every provider in `sources::downloads`. Update checks
//! orchestrate a library and its profiles, so they live in `sources::update`; this module stays a client that does not
//! know the library.

pub mod api;
pub mod archive_name;
pub mod collections;
pub mod download;
pub mod files;
mod key;
mod link;
mod page;
mod requirements;
mod text;
mod types;
mod update;

#[cfg(any(test, feature = "bench"))]
pub use types::NexusMeta;
pub use types::{NexusRequirement, NexusRequiresSync, NexusSource};

pub use download::download_into;
pub use key::{ACCOUNT as KEY_ACCOUNT, required as required_key, validate_key};
pub use link::{NxmError, parse_nxm};
pub use page::{ModPage, mod_details};
pub use requirements::{ModRequirement, file_requirements};

// ---------------------------------------------------------------------------------------------
// The provider adapter. Every Nexus-specific mapping lives here; the rest of the app sees generic source types.
// ---------------------------------------------------------------------------------------------

use self::api::NexusClient;
use crate::domain::library::ArchiveLabel;
use crate::domain::rate::{AccountTier, DownloadPolicy};
use crate::domain::source::{
    ProviderId, SourceAccount, SourceAuthKind, SourceCapabilities, SourceLink, SourceMeta,
    SourceRef, SourceStats,
};
use crate::error::AppError;
use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use crate::providers::link::is_slug;
use crate::providers::{
    CollectionSource, PAYMENT_REQUIRED, SourceDownloadRequest, SourceDownloaded, SourceProvider,
};

/// A Nexus mod id that is not a positive number.
pub fn bad_id(id: &str) -> AppError {
    AppError::user("nexus.bad_id").with("id", id)
}

static NEXUS_COLLECTIONS: collections::NexusCollections = collections::NexusCollections;

/// The provider id, shared by the download rules and the adapter below.
pub const ID: &str = "nexus";

/// More than 200 downloads in five minutes can get a free account's IP suspended for ten minutes. The number comes
/// from the community downloaders that pace whole collections; it is deliberately a ceiling, not a target.
const FREE_MAX: u32 = 200;
const FREE_WINDOW: Duration = Duration::from_mins(5);

/// Validated account tiers, keyed by API key. A validation is one network call, so its answer is kept for the life
/// of the process; a failed check is remembered as free, which is the conservative pacing rule.
static TIERS: std::sync::OnceLock<Mutex<BTreeMap<String, AccountTier>>> =
    std::sync::OnceLock::new();

/// Records the tier of an account. A download that Nexus refuses for a paid file records the account as free, so the
/// rest of the run routes it through the browser.
fn remember_tier(api_key: &str, premium: bool) {
    TIERS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(
            api_key.to_string(),
            if premium {
                AccountTier::Premium
            } else {
                AccountTier::Free
            },
        );
}

/// The tier a download is made with. Without a cached answer this validates the key once; an unreachable API is
/// treated as free so the safety ceiling still applies.
pub fn account_tier(api_key: &str) -> AccountTier {
    if let Some(tier) = cached_tier(api_key) {
        return tier;
    }

    let premium = key::validate_key(api_key).is_ok_and(|(_, premium)| premium);
    remember_tier(api_key, premium);

    if premium {
        AccountTier::Premium
    } else {
        AccountTier::Free
    }
}

/// The tier already known for a key, without a network call.
fn cached_tier(api_key: &str) -> Option<AccountTier> {
    TIERS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(api_key)
        .copied()
}

/// Records the saved key's account as free: Nexus refused a paid file for it, so later routes use the browser.
fn record_free_account() {
    if let Ok(key) = required_key() {
        remember_tier(&key, false);
    }
}

/// The Nexus adapter. It owns every Nexus-specific mapping; the rest of the app sees only generic source types.
pub struct NexusProvider;

impl SourceProvider for NexusProvider {
    fn id(&self) -> ProviderId {
        ProviderId::new(ID)
    }

    fn display_name(&self) -> &'static str {
        "Nexus"
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            collections: true,
            ..SourceCapabilities::new(SourceAuthKind::ApiKey)
        }
    }

    fn parallel_downloads(&self) -> bool {
        required_key().is_ok_and(|key| account_tier(&key) == AccountTier::Premium)
    }

    fn download_policy(&self, tier: AccountTier) -> DownloadPolicy {
        if tier == AccountTier::Free {
            DownloadPolicy::WindowBudget {
                max: FREE_MAX,
                window: FREE_WINDOW,
            }
        } else {
            DownloadPolicy::Unlimited
        }
    }

    fn known_direct_download(&self, key: &str) -> Option<bool> {
        // A direct download is a premium one: a free account reads the files but downloads them in the browser.
        cached_tier(key).map(|tier| tier == AccountTier::Premium)
    }

    fn schemes(&self) -> &'static [&'static str] {
        &[link::SCHEME]
    }

    fn page_url(&self, game_key: &str, mod_id: &str, tab: Option<&str>) -> Option<String> {
        let domain = plain_domain(game_key)?;
        let mod_id: u32 = mod_id.parse().ok()?;
        let section = match tab {
            Some("files") => "?tab=files",
            _ => "",
        };
        Some(format!(
            "https://www.nexusmods.com/{domain}/mods/{mod_id}{section}"
        ))
    }

    fn file_page_url(&self, game_key: &str, mod_id: &str, file_id: &str) -> Option<String> {
        let domain = plain_domain(game_key)?;
        let mod_id: u32 = mod_id.parse().ok()?;
        let file_id: u32 = file_id.parse().ok()?;
        Some(format!(
            "https://www.nexusmods.com/{domain}/mods/{mod_id}?tab=files&file_id={file_id}&nmm=1"
        ))
    }

    fn collection(&self) -> Option<&'static dyn CollectionSource> {
        Some(&NEXUS_COLLECTIONS)
    }

    fn parse_link(&self, url: &str) -> Result<SourceLink, AppError> {
        let link = parse_nxm(url).map_err(NxmError::into_app)?;
        let (download_key, expires) = match link.key {
            Some((key, expires)) => (Some(key), Some(expires)),
            None => (None, None),
        };
        Ok(SourceLink {
            source: SourceRef::new(ID, link.mod_id.to_string()).with_file(link.file_id.to_string()),
            game_key: Some(link.domain),
            download_key,
            expires,
        })
    }

    fn credential_account(&self) -> Option<&'static str> {
        Some(KEY_ACCOUNT)
    }

    fn key_page(&self) -> Option<&'static str> {
        Some("https://www.nexusmods.com/users/myaccount?tab=api%20access")
    }

    fn valid_mod_id(&self, mod_id: &str) -> bool {
        mod_id.parse::<u32>().is_ok_and(|id| id > 0)
    }

    fn source_from_archive_name(&self, file_name: &str) -> Option<SourceRef> {
        let archive = archive_name::parse(file_name)?;
        Some(SourceRef::new(ID, archive.mod_id.to_string()))
    }

    fn archive_name(&self, file_name: &str) -> Option<ArchiveLabel> {
        archive_name::parse(file_name).map(|archive| ArchiveLabel {
            name: Some(archive.name),
            version: archive.version,
        })
    }

    fn validate_key(&self, key: &str) -> Result<SourceAccount, AppError> {
        let (name, premium) = validate_key(key)?;
        Ok(account(name, premium))
    }

    fn download(
        &self,
        request: SourceDownloadRequest<'_>,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<SourceDownloaded, AppError> {
        let nexus =
            types::source_of(&request.source).ok_or_else(|| bad_id(&request.source.mod_id))?;
        let download_key = request
            .download_key
            .map(|(key, expires)| (key.to_string(), expires));
        let path = download_into(
            request.directory,
            request.game_key,
            nexus,
            request.archive_name,
            download_key.as_ref(),
            progress,
        )
        .inspect_err(|error| {
            if error.code == PAYMENT_REQUIRED {
                record_free_account();
            }
        })?;
        // The provider may have resolved a file id the request did not name; the sidecar carries it.
        let source = crate::infra::sidecar::read_source(&path).unwrap_or(request.source);
        Ok(SourceDownloaded { path, source })
    }

    fn check_updates(
        &self,
        key: &str,
        requests: &[crate::providers::SourceUpdateRequest],
    ) -> Vec<crate::providers::SourceUpdate> {
        update::check_updates(key, requests)
    }

    fn fetch_meta(&self, key: &str, game_key: &str, mod_id: &str) -> Result<SourceMeta, AppError> {
        let domain = plain_domain(game_key)
            .ok_or_else(|| AppError::new("nexus.no_domain").with("domain", game_key))?;
        let mod_id: u32 = mod_id.parse().map_err(|_| bad_id(mod_id))?;
        let client = NexusClient::new(key);
        let page = mod_details(&client, domain, mod_id)?;
        Ok(meta_from_page(&page))
    }
}

/// Test support: the page data as a check stores it. The caller puts it into the family it tests, so this module does
/// not need the library type.
#[cfg(test)]
pub fn checked_page_meta(
    old: Option<&SourceMeta>,
    page: &ModPage,
    checked_at: u32,
    picture: Option<String>,
) -> SourceMeta {
    update::page_meta(old, page, checked_at, picture)
}

/// The account behind a validated key. A free Nexus account reads the files but cannot download them without the
/// browser step, so the generic direct-download flag follows the premium tier.
const fn account(name: String, premium: bool) -> SourceAccount {
    SourceAccount {
        name,
        premium,
        direct_download: premium,
    }
}

/// A Nexus game domain: ASCII letters, digits and `-`, with no path or query.
fn plain_domain(domain: &str) -> Option<&str> {
    is_slug(domain, false).then_some(domain)
}

/// The page data mapped to the shared metadata model. Fields the shared model does not name go to `extra`.
pub(super) fn meta_from_page(page: &ModPage) -> SourceMeta {
    let mut extra = std::collections::BTreeMap::new();

    if let Some(uid) = &page.uid {
        extra.insert("uid".to_string(), uid.clone());
    }

    if let Some(game_id) = page.game_id {
        extra.insert("game_id".to_string(), game_id.to_string());
    }

    SourceMeta {
        name: page.name.clone(),
        author: page.author.clone(),
        uploader: page.uploader.clone(),
        category: page.category.clone(),
        summary: page.summary.clone(),
        description: page.description.clone(),
        picture_url: page.picture_url.clone(),
        picture: None,
        version: Some(page.version.clone()),
        stats: SourceStats {
            downloads: page.downloads,
            unique_downloads: page.unique_downloads,
            endorsements: page.endorsements,
            subscribers: None,
        },
        adult: page.adult,
        checked_at: None,
        updated_at: page.updated_at,
        created_at: page.created_at,
        status: page.status.clone(),
        tags: page.tags.clone(),
        extra,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_free_tier_has_a_download_ceiling() {
        assert_eq!(
            NexusProvider.download_policy(AccountTier::Free),
            DownloadPolicy::WindowBudget {
                max: 200,
                window: Duration::from_mins(5)
            }
        );
        for tier in [AccountTier::Anonymous, AccountTier::Premium] {
            assert_eq!(
                NexusProvider.download_policy(tier),
                DownloadPolicy::Unlimited
            );
        }
    }

    #[test]
    fn only_a_premium_account_downloads_directly() {
        let premium = account("Paid".to_string(), true);
        assert!(premium.premium && premium.direct_download);
        let free = account("Free".to_string(), false);
        assert!(!free.premium && !free.direct_download);
        assert_eq!(free.name, "Free");
    }

    #[test]
    fn an_nxm_link_becomes_a_generic_source_link() {
        let link = NexusProvider
            .parse_link("nxm://examplegame/mods/123/files/456?key=secret&expires=99")
            .unwrap();
        assert_eq!(link.source.provider.as_str(), "nexus");
        assert_eq!(link.source.mod_id, "123");
        assert_eq!(link.source.file_id.as_deref(), Some("456"));
        assert_eq!(link.game_key.as_deref(), Some("examplegame"));
        assert_eq!(link.download_key.as_deref(), Some("secret"));
        assert_eq!(link.expires, Some(99));
    }

    #[test]
    fn a_page_url_accepts_only_a_plain_domain_and_a_number() {
        assert_eq!(
            NexusProvider.page_url("examplegame", "123", Some("files")),
            Some("https://www.nexusmods.com/examplegame/mods/123?tab=files".to_string())
        );
        assert!(NexusProvider.page_url("../evil", "123", None).is_none());
        assert!(NexusProvider.page_url("examplegame", "abc", None).is_none());
    }

    #[test]
    fn page_data_keeps_the_common_fields_and_the_nexus_ids() {
        let page = ModPage {
            uid: Some("42".to_string()),
            game_id: Some(7),
            version: "1.2.0".to_string(),
            name: Some("Example".to_string()),
            downloads: Some(10),
            endorsements: Some(2),
            ..ModPage::default()
        };
        let meta = meta_from_page(&page);
        assert_eq!(meta.name.as_deref(), Some("Example"));
        assert_eq!(meta.version.as_deref(), Some("1.2.0"));
        assert_eq!(meta.stats.downloads, Some(10));
        assert_eq!(meta.extra.get("uid").map(String::as_str), Some("42"));
        assert_eq!(meta.extra.get("game_id").map(String::as_str), Some("7"));
    }
}
