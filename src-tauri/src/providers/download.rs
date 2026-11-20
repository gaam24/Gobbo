//! Downloading an archive from a provider's CDN: the name the file takes, the temporary part file, the check that the
//! answer is an archive, the move into place and the sidecar that names the source. A provider supplies its links,
//! its name rule and its error for a non-archive answer; the rest is shared.

use std::path::{Path, PathBuf};

use crate::domain::rate::AccountTier;
use crate::domain::source::SourceRef;
use crate::error::AppError;
use crate::infra::http::{self, BodyProblem, DownloadInfo, PartError};
use crate::infra::sidecar;

/// What one provider adds to an archive download.
pub struct ArchiveRules<'a> {
    /// The provider whose CDN serves the file, and the tier the download is made with. Together they pick the pacing
    /// rule; they are data on the rule, never a global delay.
    pub provider: &'a str,
    pub tier: AccountTier,
    /// The sidecar written next to the finished file. It names the source the file came from. `None` is for an archive
    /// that is a recipe, such as a collection, not a mod version.
    pub sidecar: Option<SourceRef>,
    /// The final path of the file, given the response and the link it came from.
    pub target: &'a dyn Fn(&DownloadInfo, &str) -> PathBuf,
    /// The error for an answer that is an empty file or a web page.
    pub body_error: &'a dyn Fn(BodyProblem) -> AppError,
    /// The error when there is no link to try.
    pub no_link: AppError,
}

/// Downloads the first working link into `dir` as an archive. The bytes go to a temporary part file first; an empty
/// answer or a web page is refused, and the next link is tried. A cancelled download stops at once. The error of the
/// last failed try is returned as it came, so the provider maps it to its own codes.
pub fn fetch_archive(
    dir: &Path,
    links: &[&str],
    rules: ArchiveRules<'_>,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<PathBuf, AppError> {
    let mut last_error = None;
    let policy = super::download_policy(rules.provider, rules.tier);
    let clock = http::pacer(&format!("{}:{}", rules.provider, rules.tier as u8));
    // One 429 budget for the whole operation, shared by every CDN link. The pacing is per provider and tier and runs
    // before every try: an unlimited policy returns at once, so no user waits for another provider's ceiling.
    let mut rate = http::RateRetry::default();

    for &url in links {
        match http::download_part(dir, url, &mut rate, &mut || clock.wait(policy), progress) {
            Ok(part) => {
                let target = (rules.target)(&part.info, url);
                part.keep_as(&target)?;
                // Without the source the import reads the mod from the archive's name, so a failed write loses little.
                if let Some(sidecar) = rules.sidecar {
                    sidecar::write_source(&target, sidecar);
                }

                return Ok(target);
            }
            Err(PartError::Body(problem)) => last_error = Some((rules.body_error)(problem)),
            Err(PartError::Failed(error)) if error.code == crate::error::DOWNLOAD_CANCELLED => {
                return Err(error);
            }
            // A failed link, a 429 past the budget included, is remembered and the next link gets its turn.
            Err(PartError::Failed(error)) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or(rules.no_link))
}

/// The name of the finished file. The CDN's `Content-Disposition` is the exact archive name; the URL, the requested
/// name and the response type are fallbacks for a CDN that sends no header. `fallback` names the file when none of
/// those does, without an extension; the extension comes from the response type.
pub fn target_name(info: &DownloadInfo, url: &str, requested: &str, fallback: &str) -> String {
    if let Some(name) = info
        .content_disposition
        .as_deref()
        .and_then(filename_from_content_disposition)
    {
        return name;
    }

    if let Some(name) = url_archive_name(url) {
        return name;
    }

    let extension = archive_extension(info.content_type.as_deref());
    let requested = requested.trim();

    if !requested.is_empty() {
        return if crate::domain::archive::extension_of(requested).is_some() {
            requested.to_string()
        } else {
            format!("{requested}.{extension}")
        };
    }

    format!("{fallback}.{extension}")
}

/// The archive name at the end of a signed CDN URL. New Nexus uploads use a UUID path, so this is only a fallback.
fn url_archive_name(url: &str) -> Option<String> {
    let url = url::Url::parse(url).ok()?;
    let name = percent_decode(url.path_segments()?.next_back()?)?;
    crate::domain::archive::extension_of(&name)?;
    Some(name)
}

/// The kind of archive a response says it is.
fn archive_extension(content_type: Option<&str>) -> &'static str {
    let content_type = content_type.unwrap_or_default().to_ascii_lowercase();

    if content_type.contains("7z") {
        "7z"
    } else if content_type.contains("rar") {
        "rar"
    } else {
        "zip"
    }
}

/// The name in a `Content-Disposition` header. `filename*` (RFC 5987) wins over `filename` because it carries UTF-8.
fn filename_from_content_disposition(header: &str) -> Option<String> {
    let mut plain = None;

    for part in header.split(';').skip(1) {
        let Some((name, value)) = part.trim().split_once('=') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim().trim_matches('"');

        if name == "filename*" {
            // `UTF-8''name`, `UTF-8'en'name` or a bare name.
            let value = value.splitn(3, '\'').nth(2).unwrap_or(value);

            if let Some(decoded) = percent_decode(value).filter(|name| !name.is_empty()) {
                return Some(decoded);
            }
        } else if name == "filename" && !value.is_empty() {
            plain = Some(value.to_string());
        }
    }

    plain
}

/// A percent-decoded UTF-8 string, for a `filename*` value.
fn percent_decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text;

    while let Some(at) = rest.find('%') {
        bytes.extend_from_slice(&rest.as_bytes()[..at]);
        let hex = rest.get(at + 1..at + 3)?;
        bytes.push(u8::from_str_radix(hex, 16).ok()?);
        rest = &rest[at + 3..];
    }

    bytes.extend_from_slice(rest.as_bytes());
    String::from_utf8(bytes).ok()
}

/// A free target name in `dir`. The same source replaces its earlier download; another file with the same name gets a
/// suffix, so a download never overwrites a different archive. `None` is for a file with no provider identity: an
/// existing file with that name is always a different archive, so the new one is numbered instead.
pub fn free_download_name(dir: &Path, name: &str, source: Option<&SourceRef>) -> PathBuf {
    let safe = crate::domain::naming::safe_limited_file_name(name);
    dir.join(crate::domain::naming::first_free(
        |number| crate::domain::naming::numbered_file_name(&safe, number),
        |candidate| {
            let path = dir.join(candidate);
            !path.exists()
                || source.is_some_and(|source| {
                    sidecar::read_source(&path).is_some_and(|known| known.same_file(source))
                })
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_content_disposition_name_is_preferred() {
        let info = DownloadInfo {
            bytes: 1,
            content_type: Some("application/zip".into()),
            content_disposition: Some("attachment; filename=\"CET 1.38.1-107-1-38-1.zip\"".into()),
        };
        assert_eq!(
            target_name(
                &info,
                "https://cf-files.nexusmods.com/a.zip",
                "fallback.zip",
                "nexus-107-168615"
            ),
            "CET 1.38.1-107-1-38-1.zip"
        );
    }

    #[test]
    fn an_encoded_content_disposition_name_is_decoded() {
        assert_eq!(
            filename_from_content_disposition(
                "attachment; filename*=UTF-8''Cr%C3%A8me%20Br%C3%BBl%C3%A9e.zip"
            )
            .as_deref(),
            Some("Crème Brûlée.zip")
        );
    }

    #[test]
    fn a_fallback_name_is_used_when_the_header_is_missing() {
        let info = DownloadInfo {
            bytes: 1,
            content_type: Some("application/octet-stream".into()),
            content_disposition: None,
        };
        assert_eq!(
            target_name(
                &info,
                "https://cf-files.nexusmods.com/8e/4b/39/uuid",
                "fallback",
                "nexus-107-168615"
            ),
            "fallback.zip"
        );
    }

    #[test]
    fn the_same_source_replaces_its_download_but_a_stranger_is_numbered() {
        let dir = crate::test_support::scratch!("download-free-name");
        let source = SourceRef::new("example", "1").with_file("2");
        let first = free_download_name(&dir, "mod.zip", Some(&source));
        assert_eq!(first, dir.join("mod.zip"));

        std::fs::write(&first, b"an archive").unwrap();
        sidecar::write_source(&first, source.clone());
        assert_eq!(
            free_download_name(&dir, "mod.zip", Some(&source)),
            first,
            "the same file keeps its own name on a later download"
        );
        assert_eq!(
            free_download_name(&dir, "mod.zip", None),
            dir.join("mod (2).zip"),
            "a file with no source never replaces an existing archive"
        );
        assert_eq!(
            free_download_name(&dir, "mod.zip", Some(&SourceRef::new("other", "3"))),
            dir.join("mod (2).zip"),
            "a different source gets the same numbered name"
        );
    }

    #[test]
    fn the_response_type_names_a_7z_when_the_header_is_missing() {
        let info = DownloadInfo {
            bytes: 1,
            content_type: Some("application/x-7z-compressed".into()),
            content_disposition: None,
        };
        assert_eq!(
            target_name(
                &info,
                "https://cf-files.nexusmods.com/8e/4b/39/uuid",
                "CET 1.38.1",
                "nexus-107-168615"
            ),
            "CET 1.38.1.7z"
        );
    }
}
