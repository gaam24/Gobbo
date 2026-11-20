//! Thunderstore as a provider: the `SourceProvider` adapter. A package is `owner-name` and a version is a plain number;
//! the public API needs no key. A game is on Thunderstore through its community (`[providers.thunderstore]` in its
//! `game.toml`), and a `ror2mm://` install link names the package, so the game comes from the package's listings.

mod api;
pub mod collections;
pub mod community_profile;
mod download;
mod link;
mod meta;

use crate::domain::library::ArchiveLabel;
use crate::domain::source::{
    ProviderId, SourceAuthKind, SourceCapabilities, SourceLink, SourceMeta, SourceRef,
};
use crate::domain::version::update_state;
use crate::error::AppError;
use crate::providers::link::is_slug;
use crate::providers::picture::{save_picture, stem};
use crate::providers::workers::{WORKER_PANIC, per_request};
use crate::providers::{
    SourceDownloadRequest, SourceDownloaded, SourceProvider, SourceUpdate, SourceUpdateRequest,
};

/// The id the definitions and the library use for Thunderstore.
pub const ID: &str = "thunderstore";

static THUNDERSTORE_COLLECTIONS: collections::ThunderstoreCollections =
    collections::ThunderstoreCollections;

/// The Thunderstore adapter. It owns every Thunderstore-specific mapping; the rest of the app sees generic source types.
pub struct ThunderstoreProvider;

impl SourceProvider for ThunderstoreProvider {
    fn id(&self) -> ProviderId {
        ProviderId::new(ID)
    }

    fn display_name(&self) -> &'static str {
        "Thunderstore"
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            collections: true,
            ..SourceCapabilities::new(SourceAuthKind::None)
        }
    }

    fn collection(&self) -> Option<&'static dyn crate::providers::CollectionSource> {
        Some(&THUNDERSTORE_COLLECTIONS)
    }

    fn schemes(&self) -> &'static [&'static str] {
        &["ror2mm"]
    }

    fn page_url(&self, game_key: &str, mod_id: &str, _tab: Option<&str>) -> Option<String> {
        let (owner, name) = link::split_full_name(mod_id)?;

        if !is_slug(game_key, true) {
            return None;
        }

        Some(format!(
            "https://thunderstore.io/c/{game_key}/p/{owner}/{name}/"
        ))
    }

    fn parse_link(&self, url: &str) -> Result<SourceLink, AppError> {
        let package = link::parse_ror2mm(url)?;
        Ok(SourceLink {
            source: SourceRef::new(ID, link::full_name(&package.owner, &package.name))
                .with_file(package.version),
            // The link names no game: the game is found from the package's communities.
            game_key: None,
            download_key: None,
            expires: None,
        })
    }

    fn valid_mod_id(&self, mod_id: &str) -> bool {
        link::split_full_name(mod_id).is_some()
    }

    fn source_from_archive_name(&self, file_name: &str) -> Option<SourceRef> {
        let package = link::archive_package(file_name)?;
        Some(
            SourceRef::new(ID, link::full_name(&package.owner, &package.name))
                .with_file(package.version),
        )
    }

    fn archive_name(&self, file_name: &str) -> Option<ArchiveLabel> {
        // The file name carries the package slug, not the display name, so the label has the version only.
        let package = link::archive_package(file_name)?;
        Some(ArchiveLabel {
            name: None,
            version: package.version,
        })
    }

    fn version_of_file<'a>(&self, file_id: &'a str) -> Option<&'a str> {
        link::is_version(file_id).then_some(file_id)
    }

    fn listed_games(&self, source: &SourceRef) -> Result<Vec<String>, AppError> {
        let (owner, name) =
            link::split_full_name(&source.mod_id).ok_or_else(|| api::bad_id(&source.mod_id))?;
        let package = api::read_package(owner, name)?;
        Ok(meta::communities(&package))
    }

    fn download(
        &self,
        request: SourceDownloadRequest<'_>,
        progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<SourceDownloaded, AppError> {
        let (owner, name) = link::split_full_name(&request.source.mod_id)
            .ok_or_else(|| api::bad_id(&request.source.mod_id))?;
        let version = match request.source.file_id.clone() {
            Some(version) => version,
            None => read_newest(&request.source.mod_id)?.1.version_number,
        };

        if !link::is_version(&version) {
            return Err(api::bad_response("the version is not a version number"));
        }

        let path = download::download_into(request.directory, owner, name, &version, progress)?;
        Ok(SourceDownloaded {
            path,
            source: SourceRef::new(ID, request.source.mod_id).with_file(version),
        })
    }

    fn check_updates(&self, _key: &str, requests: &[SourceUpdateRequest]) -> Vec<SourceUpdate> {
        // Thunderstore needs no key and its API tolerates a few parallel reads. A request whose worker stopped is that
        // family's error, not the whole check's.
        per_request(requests, check_one, |request| {
            SourceUpdate::error(request, AppError::unexpected(WORKER_PANIC))
        })
    }

    fn fetch_meta(
        &self,
        _key: &str,
        _game_key: &str,
        mod_id: &str,
    ) -> Result<SourceMeta, AppError> {
        let (package, version) = read_newest(mod_id)?;
        // The picture is saved by a check, which knows the game's folder; a read here keeps none.
        Ok(meta::meta_of(&package, &version, None))
    }
}

/// Reads the newest version of one family's package and reports it. A failed read is the family's error, not the
/// whole check's.
fn check_one(request: &SourceUpdateRequest) -> SourceUpdate {
    let mut update = SourceUpdate::new(request);

    match read_newest(&request.source.mod_id) {
        Ok((package, version)) => {
            update.state = update_state(
                Some(&version.version_number),
                None,
                request.current.as_deref(),
                None,
            );
            // The icon is saved like a Nexus picture: the same address keeps the saved copy, a new one is downloaded.
            let picture = save_picture(
                &request.pictures_dir,
                &stem(ID, &request.source.mod_id),
                &version.icon,
                request
                    .saved_meta
                    .as_ref()
                    .and_then(|meta| meta.picture_url.as_deref()),
            );
            update.meta = Some(meta::checked_meta(
                &package,
                &version,
                picture,
                request.saved_meta.as_ref(),
            ));
            update.requirements = Some(meta::requirements_of(&version));
            update.latest = Some(version.version_number);
        }
        Err(error) => update.error = Some(error),
    }

    update
}

/// A package and its newest version, by the package id.
fn read_newest(mod_id: &str) -> Result<(api::PackageRecord, api::VersionRecord), AppError> {
    let (owner, name) = link::split_full_name(mod_id).ok_or_else(|| api::bad_id(mod_id))?;
    let package = api::read_package(owner, name)?;
    let version = api::latest(&package)?.clone();
    Ok((package, version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::version::UpdateState;

    #[test]
    fn the_adapter_describes_thunderstore() {
        let provider = ThunderstoreProvider;
        assert_eq!(provider.id().as_str(), "thunderstore");
        assert_eq!(provider.display_name(), "Thunderstore");
        assert_eq!(provider.schemes(), ["ror2mm"]);
        let capabilities = provider.capabilities();
        assert!(capabilities.update_check && capabilities.deep_link && capabilities.requirements);
        assert_eq!(capabilities.auth, SourceAuthKind::None);
        assert!(provider.credential_account().is_none());
    }

    #[test]
    fn an_install_link_becomes_a_source_without_a_game() {
        let link = ThunderstoreProvider
            .parse_link("ror2mm://v1/install/thunderstore.io/example_owner/example_mod/5.0.5/")
            .unwrap();
        assert_eq!(link.source.provider.as_str(), "thunderstore");
        assert_eq!(link.source.mod_id, "example_owner-example_mod");
        assert_eq!(link.source.file_id.as_deref(), Some("5.0.5"));
        assert_eq!(link.game_key, None);
        assert_eq!(link.download_key, None);
    }

    #[test]
    fn a_page_url_needs_a_community_and_a_package_id() {
        assert_eq!(
            ThunderstoreProvider.page_url("example-community", "example_owner-example_mod", None),
            Some(
                "https://thunderstore.io/c/example-community/p/example_owner/example_mod/"
                    .to_string()
            )
        );
        assert!(
            ThunderstoreProvider
                .page_url("../evil", "example_owner-example_mod", None)
                .is_none()
        );
        assert!(
            ThunderstoreProvider
                .page_url("example-community", "example_mod", None)
                .is_none()
        );
    }

    #[test]
    fn only_an_owner_and_a_name_is_a_valid_mod_id() {
        assert!(ThunderstoreProvider.valid_mod_id("example_owner-example_mod"));
        assert!(!ThunderstoreProvider.valid_mod_id("example_mod"));
        assert!(!ThunderstoreProvider.valid_mod_id("1234"));
        assert!(!ThunderstoreProvider.valid_mod_id("owner-name-1.0.0"));
    }

    #[test]
    fn a_downloaded_archive_names_its_package_and_version() {
        let source = ThunderstoreProvider
            .source_from_archive_name("example_owner-example_mod-5.0.5.zip")
            .unwrap();
        assert_eq!(source.mod_id, "example_owner-example_mod");
        assert_eq!(source.file_id.as_deref(), Some("5.0.5"));
        assert!(
            ThunderstoreProvider
                .source_from_archive_name("Some Mod.zip")
                .is_none()
        );
    }

    #[test]
    fn a_downloaded_archive_label_has_its_version_and_no_name() {
        assert_eq!(
            ThunderstoreProvider.archive_name("example_owner-example_mod-5.0.5.zip"),
            Some(ArchiveLabel {
                name: None,
                version: "5.0.5".to_string(),
            })
        );
        assert_eq!(
            ThunderstoreProvider.archive_name("Some Mod.zip"),
            None,
            "a name without the owner-name-version shape is not a Thunderstore archive"
        );
    }

    /// Reads and downloads a real package from the live API. It needs the network, so it runs by hand:
    /// `cargo test --lib a_live_package -- --ignored`.
    #[test]
    #[ignore = "reads and downloads from the live Thunderstore API"]
    fn a_live_package_is_listed_downloaded_and_checked() {
        let dir = crate::test_support::scratch!("thunderstore-live");
        let provider = ThunderstoreProvider;
        let source = SourceRef::new(ID, "example_owner-example_mod");

        let listed = provider.listed_games(&source).unwrap();
        assert!(
            listed.iter().any(|key| key == "example-community"),
            "{listed:?}"
        );

        let downloaded = provider
            .download(
                SourceDownloadRequest {
                    source: source.clone(),
                    game_key: "example-community",
                    directory: &dir,
                    archive_name: None,
                    download_key: None,
                },
                &mut |_, _| true,
            )
            .unwrap();
        let version = downloaded.source.file_id.clone().unwrap();
        let file_name = downloaded
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            file_name,
            format!("example_owner-example_mod-{version}.zip")
        );
        assert_eq!(
            crate::infra::sidecar::read_source(&downloaded.path),
            Some(downloaded.source.clone())
        );

        let meta = provider
            .fetch_meta("", "example-community", "example_owner-example_mod")
            .unwrap();
        assert_eq!(meta.version.as_deref(), Some(version.as_str()));

        let updates = provider.check_updates(
            "",
            &[SourceUpdateRequest {
                family: "example_mod".to_string(),
                source,
                game_key: "example-community".to_string(),
                current: Some("0.0.1".to_string()),
                current_file_id: None,
                pictures_dir: dir.clone(),
                saved_meta: None,
                saved_sync: None,
                archives: Vec::new(),
            }],
        );
        assert_eq!(
            updates[0].state,
            UpdateState::Newer,
            "{:?}",
            updates[0].error
        );
        assert!(updates[0].requirements.is_some());
        let picture = updates[0]
            .meta
            .as_ref()
            .and_then(|meta| meta.picture.clone());
        assert!(
            picture.is_some_and(|file| dir.join(file).is_file()),
            "the icon is saved next to the game's images"
        );

        let missing = provider.fetch_meta("", "example-community", "nobody-NoSuchMod");
        assert_eq!(missing.unwrap_err().code, "thunderstore.not_found");
    }
}
