//! Downloading a collection's direct or bundle-adjacent file from outside the collection provider. The URL comes
//! from an untrusted manifest, so only HTTPS public hosts are accepted and the size and MD5 are checked when the
//! manifest gives them.

use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use md5::Md5;
use url::Url;

use crate::error::{AppError, io_at};
use crate::infra::http::{self, BodyProblem, PartError};
use crate::providers::download::{archive_name, free_download_name};

/// The largest direct file Gobbo downloads. A collection is a recipe, not a disk image.
const MAX_EXTERNAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// A downloaded external file.
pub struct Downloaded {
    pub path: PathBuf,
    pub is_archive: bool,
}

/// Downloads `url` into `directory`. `expected_kb` and `md5` come from the manifest; a mismatch is an error.
pub fn download(
    url: &str,
    directory: &Path,
    expected_kb: u32,
    md5: Option<&str>,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<Downloaded, AppError> {
    validate_url(url)?;
    // A direct file has no provider account to pace: the server's own 429 is the only limit that applies here.
    let part = http::download_part(
        directory,
        url,
        &mut http::RateRetry::default(),
        &mut || {},
        &mut |received, total| total <= MAX_EXTERNAL_BYTES && progress(received, total),
    )
    .map_err(|error| match error {
        PartError::Failed(error) => error,
        PartError::Body(BodyProblem::Empty) => AppError::user("collection.external_empty"),
        PartError::Body(BodyProblem::Html) => AppError::user("collection.external_html"),
    })?;

    if part.info.bytes > MAX_EXTERNAL_BYTES {
        return Err(
            AppError::user("collection.external_too_large").with("limit", MAX_EXTERNAL_BYTES)
        );
    }

    let name = archive_name(&part.info, url, "", "external", Some("zip"));
    // A direct file has no provider identity, so never treat an existing file with the same name as its own earlier
    // download: number the new one instead of inventing a provider id for the comparison.
    let target = free_download_name(directory, &name, None);
    part.keep_as(&target)?;
    let actual_kb = u32::try_from(
        fs::metadata(&target)
            .map_err(io_at(&target))?
            .len()
            .saturating_div(1024),
    )
    .unwrap_or(u32::MAX);

    if expected_kb > 0 && actual_kb.saturating_add(1) < expected_kb {
        let _ = fs::remove_file(&target);
        return Err(AppError::user("collection.external_size")
            .with("expected", expected_kb)
            .with("found", actual_kb));
    }

    if let Some(expected) = md5.filter(|text| !text.trim().is_empty()) {
        let actual =
            crate::infra::io::digest_file::<Md5>(&target, &mut |_| {}).map_err(io_at(&target))?;
        if !actual.eq_ignore_ascii_case(expected.trim()) {
            let _ = fs::remove_file(&target);
            return Err(AppError::user("collection.external_md5")
                .with("expected", expected)
                .with("found", actual));
        }
    }

    let is_archive = target
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(crate::infra::archive::is_archive);
    Ok(Downloaded {
        path: target,
        is_archive,
    })
}

/// A direct URL is HTTPS, has a host and does not point at this computer or a private network.
pub fn validate_url(url: &str) -> Result<Url, AppError> {
    let parsed = Url::parse(url.trim())
        .map_err(|_| AppError::user("collection.external_url").with("url", url))?;
    if parsed.scheme() != "https" {
        return Err(AppError::user("collection.external_url").with("url", url));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::user("collection.external_url").with("url", url))?;
    if host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
        || host.to_ascii_lowercase().ends_with(".local")
        || host.to_ascii_lowercase().ends_with(".internal")
    {
        return Err(AppError::user("collection.external_url").with("url", url));
    }

    let address_host = host.trim_start_matches('[').trim_end_matches(']');

    if let Ok(address) = address_host.parse::<IpAddr>() {
        let private = match address {
            IpAddr::V4(address) => {
                address.is_private()
                    || address.is_loopback()
                    || address.is_link_local()
                    || address.is_unspecified()
            }
            IpAddr::V6(address) => {
                address.is_loopback() || address.is_unspecified() || address.is_unique_local()
            }
        };

        if private {
            return Err(AppError::user("collection.external_url").with("url", url));
        }
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_public_https_url_passes() {
        assert!(validate_url("https://github.com/a/b.zip").is_ok());
    }

    #[test]
    fn plain_http_localhost_and_private_addresses_are_refused() {
        for url in [
            "http://example.com/a.zip",
            "https://localhost/a.zip",
            "https://127.0.0.1/a.zip",
            "https://10.0.0.1/a.zip",
            "https://192.168.1.5/a.zip",
            "https://[::1]/a.zip",
            "file:///tmp/a.zip",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
    }
}
