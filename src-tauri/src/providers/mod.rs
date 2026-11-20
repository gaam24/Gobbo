//! The external mod services, behind one port.
//!
//! A provider maps its API to the generic types in `domain::source`. The library, the profiles and the deployment
//! never learn which service a mod came from; they only see `SourceRef`, `SourceMeta` and `SourceRequirement`.

pub mod download;
pub mod fetch;
mod link;
pub mod nexus;
mod pacing;
pub mod picture;
#[cfg(test)]
pub mod testing;
pub mod thunderstore;
mod workers;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// How many provider requests run at once during an update check. A push notification or a bulk check keeps the same
/// request budget; a library of many mods just does not wait for one round trip at a time.
pub const UPDATE_WORKERS: usize = 4;

pub use pacing::download_policy;

/// The next index of `len` for a worker. `None` when every index was already handed out. The counter is shared, so each
/// index reaches exactly one worker.
pub fn take_index(next: &AtomicUsize, len: usize) -> Option<usize> {
    let index = next.fetch_add(1, Ordering::Relaxed);
    (index < len).then_some(index)
}

use crate::domain::library::ArchiveLabel;
use crate::domain::source::{
    DownloadRoute, ProviderId, SourceAccount, SourceCapabilities, SourceLink, SourceMeta,
    SourceProviderInfo, SourceRef, SourceRequirement, SourceSync,
};
use crate::domain::version::UpdateState;
use crate::error::{AppError, ErrorKind, KEY_MISSING, NO_GAME_KEY};
use crate::infra::credentials;

/// One file to download from a provider, into the game's downloads folder.
pub struct SourceDownloadRequest<'a> {
    pub source: SourceRef,
    /// The provider's key for the game.
    pub game_key: &'a str,
    /// The folder the download lands in.
    pub directory: &'a Path,
    /// The archive name the caller expects, when the provider has to pick a file by name.
    pub archive_name: Option<&'a str>,
    /// The short-lived key from a deep link, and the time it stops working.
    pub download_key: Option<(&'a str, u64)>,
}

/// A downloaded file and the source it came from.
pub struct SourceDownloaded {
    pub path: PathBuf,
    pub source: SourceRef,
}

/// One archive of a family, as the provider needs it to recognise its own file: the digest, the original name and the
/// provider file id when the library already knows it.
pub struct SourceArchive {
    pub sha256: String,
    pub original_name: String,
    pub file_id: Option<String>,
    /// The persistent part the archive belongs to, when the library already knows it.
    pub part_id: Option<String>,
}

/// One family to check on a provider.
pub struct SourceUpdateRequest {
    pub family: String,
    pub source: SourceRef,
    /// The provider's key for the game.
    pub game_key: String,
    /// The version in use in the active profile, or the newest installed one.
    pub current: Option<String>,
    /// The provider's id of the file the profile selects, when the provider identifies files and the archive kept it.
    pub current_file_id: Option<String>,
    /// Where the provider keeps page pictures.
    pub pictures_dir: PathBuf,
    pub saved_meta: Option<SourceMeta>,
    /// The last requirement read for this source, so an unchanged file is not read again.
    pub saved_sync: Option<SourceSync>,
    pub archives: Vec<SourceArchive>,
}

/// What one provider update brought. A provider fills the fields it supports; the feature stores what it gets.
pub struct SourceUpdate {
    pub family: String,
    pub source: SourceRef,
    pub current: Option<String>,
    pub latest: Option<String>,
    /// The provider's id of the newest file, when the provider identifies files and the check read a file chain.
    pub latest_file_id: Option<String>,
    /// How the newest version compares with the selected one. A provider never sets `Ignored`; the feature applies
    /// the user's ignore to the result.
    pub state: UpdateState,
    pub error: Option<AppError>,
    /// The page data merged with the old values, when the page was read.
    pub meta: Option<SourceMeta>,
    /// `Some` when the requirements were read; `None` when a recent read made it unnecessary.
    pub requirements: Option<Vec<SourceRequirement>>,
    pub dlcs: Vec<Vec<String>>,
    pub sync: Option<SourceSync>,
    /// The archive digest the provider matched to its file id.
    pub matched: Option<(String, String)>,
    /// The archive digest the provider matched to its persistent part id.
    pub matched_part: Option<(String, String)>,
}

impl SourceUpdate {
    /// An update that says nothing yet: the request's family, source and current version, and the default state.
    pub fn new(request: &SourceUpdateRequest) -> Self {
        Self {
            family: request.family.clone(),
            source: request.source.clone(),
            current: request.current.clone(),
            latest: None,
            latest_file_id: None,
            state: UpdateState::Unknown,
            error: None,
            meta: None,
            requirements: None,
            dlcs: Vec::new(),
            sync: None,
            matched: None,
            matched_part: None,
        }
    }

    /// An update that reports `error` for the request's family.
    pub fn error(request: &SourceUpdateRequest, error: AppError) -> Self {
        let mut update = Self::new(request);
        update.error = Some(error);
        update
    }
}

/// One external mod service.
///
/// Methods a service does not support keep the default implementation and answer with
/// [`AppError`] `source.unsupported`. Callers ask [`SourceProvider::capabilities`] first.
pub trait SourceProvider: Send + Sync {
    /// The id the definitions and the library use, for example `nexus`.
    fn id(&self) -> ProviderId;

    /// The name shown to the user when the interface has no translation for the provider.
    fn display_name(&self) -> &'static str {
        "Source"
    }

    /// What this provider can do.
    fn capabilities(&self) -> SourceCapabilities;

    /// Whether the saved account may download several files at once. A provider that caps an account's bandwidth
    /// gains nothing from parallel transfers; only an account without such a cap says yes.
    fn parallel_downloads(&self) -> bool {
        false
    }

    /// The URL schemes the provider owns, for example `nxm`.
    fn schemes(&self) -> &'static [&'static str] {
        &[]
    }

    /// The page of a mod, when the provider has a web page.
    fn page_url(&self, _game_key: &str, _mod_id: &str, _tab: Option<&str>) -> Option<String> {
        None
    }

    /// The page of one exact file, when the provider has a web page that selects it. A free account uses this page
    /// to download the file by hand.
    fn file_page_url(&self, _game_key: &str, _mod_id: &str, _file_id: &str) -> Option<String> {
        None
    }

    /// The provider's collection adapter, when it can install collections. The caller asks
    /// [`SourceProvider::capabilities`] first; this is the implementation behind that flag.
    fn collection(&self) -> Option<&'static dyn CollectionSource> {
        None
    }

    /// Parses a deep link into a source and the optional short-lived download key.
    fn parse_link(&self, _url: &str) -> Result<SourceLink, AppError> {
        Err(unsupported())
    }

    /// The provider keys of the games a mod is listed for (for example Thunderstore communities), for a link that
    /// names no game. The caller takes the first one the user has a game for.
    fn listed_games(&self, _source: &SourceRef) -> Result<Vec<String>, AppError> {
        Err(unsupported())
    }

    /// The keyring account this provider keeps its key under. `None` when it needs no saved key.
    fn credential_account(&self) -> Option<&'static str> {
        None
    }

    /// The page where the user makes the provider's key, when there is one. The interface opens it through the backend,
    /// so the URL always belongs to the provider.
    fn key_page(&self) -> Option<&'static str> {
        None
    }

    /// Checks a saved key and describes the account behind it. `SourceAccount::direct_download` says whether this
    /// account may fetch files without the provider's web page; [`download_route`] reads that flag for every update
    /// decision.
    fn validate_key(&self, _key: &str) -> Result<SourceAccount, AppError> {
        Err(unsupported())
    }

    /// Whether the account behind `key` may download directly, when an earlier call already learned it. `None` when
    /// nothing is known yet; this never makes a network call.
    fn known_direct_download(&self, _key: &str) -> Option<bool> {
        None
    }

    /// Downloads one file into the game's downloads folder. The provider writes the sidecar that names the source.
    /// A provider that cannot download leaves the file to the user.
    fn download(
        &self,
        _request: SourceDownloadRequest<'_>,
        _progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<SourceDownloaded, AppError> {
        Err(unsupported())
    }

    /// Whether a mod id is usable on this provider. The default only refuses an empty id; a provider with numeric ids
    /// narrows it.
    fn valid_mod_id(&self, mod_id: &str) -> bool {
        !mod_id.trim().is_empty()
    }

    /// The source an archive file name names, when the provider's downloads follow a recognisable naming scheme. The
    /// import uses it when the archive has no sidecar.
    fn source_from_archive_name(&self, _file_name: &str) -> Option<SourceRef> {
        None
    }

    /// The display version a file id names, when the provider's file id is the version (Thunderstore). `None` when the
    /// id is opaque (Nexus) or the provider has no files. The import uses it when `info.json` gives no version.
    fn version_of_file<'a>(&self, _file_id: &'a str) -> Option<&'a str> {
        None
    }

    /// The name and version an archive file name gives, when the provider writes them. The name is `None` when the file
    /// name has only a package slug. The import reads it once and the archive keeps the result, so the version shows as
    /// the provider named it, also after a rescan.
    fn archive_name(&self, _file_name: &str) -> Option<ArchiveLabel> {
        None
    }

    /// Reads a saved page picture as a data URL. The shared reader accepts only plain picture names in `images`.
    fn picture(&self, images: &Path, file: &str) -> Result<Option<String>, AppError> {
        picture::read_picture(images, file)
    }

    /// Reads a mod page by its provider id.
    fn fetch_meta(
        &self,
        _key: &str,
        _game_key: &str,
        _mod_id: &str,
    ) -> Result<SourceMeta, AppError> {
        Err(unsupported())
    }

    /// Checks a batch of families for updates and reports each one. A provider returns one result per request.
    fn check_updates(&self, _key: &str, requests: &[SourceUpdateRequest]) -> Vec<SourceUpdate> {
        requests
            .iter()
            .map(|request| SourceUpdate::error(request, unsupported()))
            .collect()
    }
}

/// One collection link: the provider, the game and the collection, with a revision when the link names one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionLink {
    pub provider: ProviderId,
    pub game_key: String,
    pub slug: String,
    /// `None` means the latest published revision.
    pub revision: Option<u32>,
    /// The website URL the user pasted, when the link was one.
    pub page_url: Option<String>,
}

/// One member of a collection revision, before the archive manifest adds choices and rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionMod {
    pub mod_id: String,
    pub file_id: String,
    pub optional: bool,
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub summary: String,
    pub picture_url: Option<String>,
    pub adult: bool,
    pub file_name: Option<String>,
    /// The download size in KB, saturated.
    pub size_kb: u32,
}

/// A resource of a collection that is not a Nexus file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionExternal {
    pub name: String,
    pub resource_type: String,
    pub url: Option<String>,
}

/// One published revision, for the revision picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionRevisionSummary {
    pub revision: u32,
    pub created_at: Option<u32>,
    pub latest: bool,
    pub mod_count: u32,
    pub total_size_kb: u32,
}

/// A collection revision as the provider API describes it. The archive itself is downloaded separately; its manifest
/// adds the choices, rules, bundles and patches the API does not expose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionRevision {
    pub revision: u32,
    pub latest_revision: u32,
    pub info: crate::domain::collection::CollectionInfo,
    pub mods: Vec<CollectionMod>,
    pub external: Vec<CollectionExternal>,
    pub revisions: Vec<CollectionRevisionSummary>,
    /// The opaque API path of the collection archive.
    pub archive_link: Option<String>,
    /// The API already describes the whole recipe (a Thunderstore modpack is its dependency list), so there is no
    /// archive to read and the plan is not limited.
    pub complete: bool,
}

/// A provider that can install collections: a Nexus collection or a Thunderstore modpack. A provider without the capability returns `None` from
/// [`SourceProvider::collection`].
pub trait CollectionSource: Send + Sync {
    /// Parses a website URL or a deep link that names a collection.
    fn parse_link(&self, url: &str) -> Result<CollectionLink, AppError>;

    /// Reads one revision. Without a revision number the latest published one is read. `key` is the provider's saved
    /// key; an empty string asks for the public metadata only.
    fn revision(
        &self,
        key: &str,
        link: &CollectionLink,
        revision: Option<u32>,
    ) -> Result<CollectionRevision, AppError>;

    /// Downloads the collection archive to `directory` and returns its path. `archive_link` is the opaque path the
    /// revision carries.
    fn download_archive(
        &self,
        key: &str,
        game_key: &str,
        archive_link: &str,
        directory: &Path,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<PathBuf, AppError>;

    /// The page of the collection on the provider.
    fn page_url(&self, game_key: &str, slug: &str) -> Option<String>;
}

fn unsupported() -> AppError {
    AppError::user("source.unsupported")
}

/// The code of a direct download that needs a paid account. The interface points the user to the mod's page instead.
/// Defined once in `error.rs` so every entitlement failure uses the same code.
pub use crate::error::PAYMENT_REQUIRED;

/// The provider has no key for the game: the game definition names none.
pub fn no_game_key(provider: &str) -> AppError {
    AppError::user(NO_GAME_KEY).with("provider", provider)
}

/// A link that does not belong to a mod provider Gobbo knows.
pub fn bad_link() -> AppError {
    AppError::user("source.bad_link")
}

/// A response that is not what the API promises. A retry gives the same answer, so it is not retryable.
pub fn bad_response(code: &str, detail: impl ToString) -> AppError {
    AppError::new(code)
        .with("detail", detail)
        .kind(ErrorKind::Permanent)
}

/// The error for an action the provider `id` does not offer.
pub fn unsupported_by(id: &str) -> AppError {
    unsupported().with("provider", id)
}

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

static NEXUS: nexus::NexusProvider = nexus::NexusProvider;
static THUNDERSTORE: thunderstore::ThunderstoreProvider = thunderstore::ThunderstoreProvider;

static PROVIDERS: &[&dyn SourceProvider] = &[&NEXUS, &THUNDERSTORE];

/// Every provider this build knows, in the order the interface lists them.
pub fn providers() -> &'static [&'static dyn SourceProvider] {
    PROVIDERS
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

/// The routing rule over the facts it reads: the provider's download capability, whether a needed key is missing, and
/// what the saved account is known to be able to do.
pub const fn route_from(
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

/// The provider override a test installed for `id`, when there is one. This item is compiled only under `cfg(test)`;
/// a shipped build has no such thing and [`provider`] resolves exactly as it always did.
#[cfg(test)]
fn test_override(id: &str) -> Option<&'static dyn SourceProvider> {
    testing::get(id)
}

/// The production half of the test seam: no override exists outside `cfg(test)`, so [`provider`] keeps its original
/// behaviour in every shipped build.
#[cfg(not(test))]
fn test_override(_id: &str) -> Option<&'static dyn SourceProvider> {
    None
}

/// The provider with this id, when it is registered. A test may install a network-free override for one id on its own
/// thread (the test-only `testing` module); with no override, this is the first registered provider with the id, exactly
/// as before.
pub fn provider(id: &str) -> Option<&'static dyn SourceProvider> {
    test_override(id).or_else(|| {
        PROVIDERS
            .iter()
            .copied()
            .find(|provider| provider.id().as_str() == id)
    })
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

/// The provider with this id. An unknown one is a user-facing error.
pub fn require(id: &str) -> Result<&'static dyn SourceProvider, AppError> {
    provider(id).ok_or_else(|| AppError::user("source.unknown").with("provider", id))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::source::SourceAuthKind;

    /// Every adapter answers the same port questions, and a provider cannot be registered without a case here.
    #[test]
    fn every_registered_provider_keeps_the_port_contract() {
        testing::check_all();
    }

    #[test]
    fn an_invalid_nexus_link_keeps_the_provider_error() {
        let error = provider("nexus")
            .unwrap()
            .parse_link("not a link")
            .unwrap_err();
        assert_eq!(error.code, "nexus.bad_link");
    }

    #[test]
    fn source_links_use_a_registered_scheme() {
        assert!(is_source_link("nxm://examplegame/mods/1/files/2"));
        assert!(is_source_link("NXM://examplegame/mods/1/files/2"));
        assert!(is_source_link(
            "ror2mm://v1/install/thunderstore.io/other_owner/other_mod/3.1.25/"
        ));
        assert!(!is_source_link("https://example.com/mod"));
        assert!(!is_source_link("nxm"));
        assert_eq!(
            for_link("nxm://examplegame/mods/1/files/2").map(|provider| provider.id().to_string()),
            Some("nexus".to_string())
        );
        assert_eq!(
            for_link("ror2mm://v1/install/thunderstore.io/other_owner/other_mod/3.1.25/")
                .map(|provider| provider.id().to_string()),
            Some("thunderstore".to_string())
        );
    }

    #[test]
    fn a_provider_keeps_its_key_under_one_account() {
        assert_eq!(
            provider("nexus").and_then(SourceProvider::credential_account),
            Some("nexus_api_key")
        );
        assert!(provider("not_registered").is_none());
        let Err(error) = require("not_registered") else {
            panic!("an unregistered provider must not resolve");
        };
        assert_eq!(error.code, "source.unknown");
    }

    #[test]
    fn the_settings_list_says_which_provider_needs_a_key_and_has_links() {
        let nexus = info(provider("nexus").expect("nexus must be registered"));
        assert_eq!(nexus.name, "Nexus");
        assert!(nexus.needs_key && nexus.has_key_page && nexus.direct_download);
        assert_eq!(nexus.schemes, ["nxm"]);

        let thunderstore = info(provider("thunderstore").expect("thunderstore must be registered"));
        assert!(
            !thunderstore.needs_key && !thunderstore.has_key_page && thunderstore.direct_download
        );
        assert_eq!(thunderstore.schemes, ["ror2mm"]);
    }

    /// A provider that needs no key has no account to check, so it downloads directly.
    #[test]
    fn a_provider_without_a_key_downloads_directly() {
        let thunderstore = provider("thunderstore").expect("thunderstore must be registered");
        assert_eq!(download_route(thunderstore), DownloadRoute::Direct);
        assert_eq!(info(thunderstore).download_route, DownloadRoute::Direct);
    }

    #[test]
    fn the_route_is_browser_unless_a_direct_account_is_possible() {
        // No direct capability, a missing key, or a known account without direct downloads: the browser.
        assert_eq!(route_from(false, false, Some(true)), DownloadRoute::Browser);
        assert_eq!(route_from(true, true, Some(true)), DownloadRoute::Browser);
        assert_eq!(route_from(true, false, Some(false)), DownloadRoute::Browser);
        // A saved key with an unknown or a direct account tries the direct download.
        assert_eq!(route_from(true, false, None), DownloadRoute::Direct);
        assert_eq!(route_from(true, false, Some(true)), DownloadRoute::Direct);
    }

    /// A provider with no keyring account, so the route reads only the cached account tier. `known` stands in for what
    /// the backend learned when the key was checked or a paid download was refused.
    struct TierProvider {
        direct: bool,
        known: Option<bool>,
    }

    impl SourceProvider for TierProvider {
        fn id(&self) -> ProviderId {
            ProviderId::new("tier-test")
        }

        fn capabilities(&self) -> SourceCapabilities {
            let mut capabilities = SourceCapabilities::new(SourceAuthKind::None);
            capabilities.direct_download = self.direct;
            capabilities
        }

        fn known_direct_download(&self, _key: &str) -> Option<bool> {
            self.known
        }
    }

    #[test]
    fn a_known_free_account_is_routed_to_the_browser_and_reports_payment_required() {
        let free = TierProvider {
            direct: true,
            known: Some(false),
        };
        assert_eq!(download_route(&free), DownloadRoute::Browser);
        let error = browser_route_error(&free).expect("a browser route has an error");
        assert_eq!(error.code, PAYMENT_REQUIRED);
    }

    #[test]
    fn an_unknown_or_direct_account_downloads_directly_without_an_error() {
        let unknown = TierProvider {
            direct: true,
            known: None,
        };
        assert_eq!(download_route(&unknown), DownloadRoute::Direct);
        assert!(browser_route_error(&unknown).is_none());
    }

    #[test]
    fn a_provider_without_direct_download_keeps_its_unsupported_error() {
        let no_download = TierProvider {
            direct: false,
            known: Some(true),
        };
        assert_eq!(download_route(&no_download), DownloadRoute::Browser);
        let error = browser_route_error(&no_download).expect("a browser route has an error");
        assert_eq!(error.code, "source.unsupported");
    }
}
