//! Downloading one version of a Thunderstore package into the game's downloads folder, with its source next to it.

use std::path::{Path, PathBuf};

use super::api;
use super::link;
use super::{ID, ThunderstoreProvider};
use crate::domain::rate::AccountTier;
use crate::domain::source::SourceRef;
use crate::error::AppError;
use crate::infra::http::DownloadInfo;
use crate::providers::SourceProvider;
use crate::providers::download::{ArchiveRules, archive_target, body_error, fetch_archive};

/// Downloads one version of a package into `dir`. The CDN's name wins; when it sends none the file keeps the
/// package's own `owner-name-version.zip`, so the on-disk name is unchanged for a server that names nothing special.
/// Another archive already using that name gets a numbered name instead of being overwritten.
pub fn download_into(
    dir: &Path,
    owner: &str,
    name: &str,
    version: &str,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<PathBuf, AppError> {
    let url = api::download_url(owner, name, version);
    let package_name = format!("{owner}-{name}-{version}.zip");
    let source = SourceRef::new(ID, link::full_name(owner, name)).with_file(version);
    let target = |info: &DownloadInfo, url: &str| {
        archive_target(
            dir,
            info,
            url,
            &package_name,
            &package_name,
            None,
            Some(&source),
        )
    };
    let body_error = body_error(
        api::bad_response("the server sent an empty file"),
        api::bad_response("the server sent a web page instead of an archive"),
    );
    let rules = ArchiveRules {
        policy: ThunderstoreProvider.download_policy(AccountTier::Anonymous),
        pacer_key: ID,
        tier: AccountTier::Anonymous,
        sidecar: Some(source.clone()),
        target: &target,
        body_error: &body_error,
        no_link: api::bad_response("the package names no download link"),
    };
    fetch_archive(dir, &[url.as_str()], rules, progress).map_err(download_error)
}

/// A missing version is the package's error; any other failure keeps the shared HTTP code.
fn download_error(error: AppError) -> AppError {
    if error.code == "http.status" && error.params.get("code").map(String::as_str) == Some("404") {
        return api::not_found();
    }

    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::http::body_problem;

    fn info(bytes: u64, content_type: Option<&str>) -> DownloadInfo {
        DownloadInfo {
            bytes,
            content_type: content_type.map(str::to_string),
            content_disposition: None,
        }
    }

    /// The error the download gives for this answer, or `None` for an archive.
    fn refused(info: &DownloadInfo, path: &Path) -> Option<AppError> {
        let body_error = body_error(
            api::bad_response("the server sent an empty file"),
            api::bad_response("the server sent a web page instead of an archive"),
        );
        body_problem(info, path).map(body_error)
    }

    #[test]
    fn an_empty_or_a_web_page_answer_is_not_an_archive() {
        let no_file = Path::new("not-a-file");
        assert_eq!(
            refused(&info(0, Some("application/zip")), no_file)
                .unwrap()
                .code,
            "thunderstore.bad_response"
        );
        assert_eq!(
            refused(&info(12, Some("text/html; charset=utf-8")), no_file)
                .unwrap()
                .code,
            "thunderstore.bad_response"
        );
        assert!(refused(&info(12, Some("application/zip")), no_file).is_none());
        assert!(refused(&info(12, None), no_file).is_none());
    }

    #[test]
    fn a_missing_version_is_the_package_error() {
        let error = AppError::new("http.status").with("code", "404");
        assert_eq!(download_error(error).code, "thunderstore.not_found");
        let other = AppError::new("http.status").with("code", "500");
        assert_eq!(download_error(other).code, "http.status");
    }

    #[test]
    fn an_unnamed_answer_keeps_the_packages_own_name() {
        let dir = crate::test_support::scratch!("thunderstore-name");
        let source =
            SourceRef::new(ID, link::full_name("example_owner", "example_mod")).with_file("5.0.5");
        let package = "example_owner-example_mod-5.0.5.zip";
        let plain = info(12, Some("application/zip"));
        assert_eq!(
            archive_target(
                &dir,
                &plain,
                "https://thunderstore.io/package/download/example_owner/example_mod/5.0.5/",
                package,
                package,
                None,
                Some(&source)
            ),
            dir.join(package)
        );
        let named = DownloadInfo {
            bytes: 12,
            content_type: Some("application/zip".into()),
            content_disposition: Some("attachment; filename=\"cdn-name.zip\"".into()),
        };
        assert_eq!(
            archive_target(
                &dir,
                &named,
                "https://thunderstore.io/package/download/example_owner/example_mod/5.0.5/",
                package,
                package,
                None,
                Some(&source)
            ),
            dir.join("cdn-name.zip")
        );
    }

    #[test]
    fn a_same_name_collision_numbers_the_second_download() {
        let dir = crate::test_support::scratch!("thunderstore-collision");
        let source =
            SourceRef::new(ID, link::full_name("example_owner", "example_mod")).with_file("5.0.5");
        let package = "example_owner-example_mod-5.0.5.zip";
        let plain = info(12, Some("application/zip"));
        let taken = dir.join(package);
        std::fs::write(&taken, b"an archive").unwrap();
        assert_eq!(
            archive_target(
                &dir,
                &plain,
                "https://example.invalid/download",
                package,
                package,
                None,
                Some(&source)
            ),
            dir.join("example_owner-example_mod-5.0.5 (2).zip")
        );
        crate::infra::sidecar::write_source(&taken, source.clone());
        assert_eq!(
            archive_target(
                &dir,
                &plain,
                "https://example.invalid/download",
                package,
                package,
                None,
                Some(&source)
            ),
            taken,
            "the same version still replaces its own earlier download"
        );
    }
}
