//! The port every external mod service implements, and the data it exchanges with the features.

use std::path::{Path, PathBuf};

use super::error::unsupported;
use super::picture;
use crate::domain::library::ArchiveLabel;
use crate::domain::rate::{AccountTier, DownloadPolicy};
use crate::domain::source::{
    ProviderId, SourceAccount, SourceCapabilities, SourceLink, SourceMeta, SourceRef,
    SourceRequirement, SourceSync,
};
use crate::domain::version::UpdateState;
use crate::error::AppError;

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

    /// How this provider's downloads are paced for one account tier. Anything without a documented limit stays
    /// unlimited: slowing those downloads down would only make the user wait for no reason.
    fn download_policy(&self, _tier: AccountTier) -> DownloadPolicy {
        DownloadPolicy::Unlimited
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
    /// account may fetch files without the provider's web page; [`crate::providers::download_route`] reads that flag for
    /// every update decision.
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

/// A resource of a collection that is not one of the provider's files.
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
    /// Archive paths this provider's collections use for resources Gobbo cannot apply. The manifest reader reports
    /// them; the provider owns the names instead of the generic reader.
    pub unsupported_archive_paths: Vec<String>,
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
