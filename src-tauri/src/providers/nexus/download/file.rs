//! Downloading one file from the CDN: signed links, the temporary file and the final placement.

use std::path::{Path, PathBuf};

use super::super::api::NexusClient;
use super::super::files::{
    DownloadLinks, FileVersion, download_links, file_version, find_file_id, mod_files,
};
use super::super::required_key;
use crate::domain::rate::AccountTier;
use crate::domain::source::SourceRef;
use crate::error::AppError;
use crate::infra::http::{BodyProblem, DownloadInfo};
use crate::providers::download::{ArchiveRules, fetch_archive, free_download_name, target_name};
use crate::providers::nexus::NexusSource;

/// Downloads one Nexus file into `dir`, which is the game's downloads folder. Without a file id the file is found by
/// its name among the mod's files. `download_key` is the short-lived key from an `nxm://` link; without one the account
/// must be premium or the file must allow direct downloads.
pub fn download_into(
    dir: &Path,
    game_key: &str,
    source: NexusSource,
    archive_name: Option<&str>,
    download_key: Option<&(String, u64)>,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<PathBuf, AppError> {
    let api_key = required_key()?;
    let tier = crate::providers::nexus::account_tier(&api_key);
    let client = NexusClient::new(api_key);
    let file_id = match source.file_id {
        Some(file_id) => file_id,
        None => resolve_file_id(&client, game_key, source.mod_id, archive_name)?,
    };
    // The version is only a fallback name for a CDN that sends no `Content-Disposition`. A failure here must not stop
    // a download the signed link could still serve. The same response names the persistent file group, which is what
    // makes two addons of one page separate mods in the library.
    let version = file_version(&client, game_key, file_id).ok();
    let requested = version.as_ref().map(requested_name).unwrap_or_default();
    let links = download_links(&client, game_key, file_id, download_key)?;
    let source = NexusSource {
        mod_id: source.mod_id,
        file_id: Some(file_id),
        part_id: version
            .as_ref()
            .and_then(|file| file.mod_file_id.as_deref())
            .and_then(|id| id.parse().ok())
            .or(source.part_id),
    };
    let sidecar = version.as_ref().map_or_else(
        || SourceRef::from(source),
        |file| {
            SourceRef::from(source)
                .with_part_meta(file.mod_file_name.clone(), file.file_category.clone())
        },
    );
    save(dir, &requested, source, sidecar, tier, &links, progress)
}

/// The game-scoped file id of a pack entry that names only an archive. The mod's page gives the global id that the
/// v3 file list asks by.
fn resolve_file_id(
    client: &NexusClient,
    domain: &str,
    mod_id: u32,
    archive_name: Option<&str>,
) -> Result<u32, AppError> {
    let name = archive_name.unwrap_or_default();
    let page = crate::providers::nexus::mod_details(client, domain, mod_id)?;
    let uid = page
        .uid
        .ok_or_else(|| AppError::user("nexus.file_unknown").with("name", name))?;
    let files = mod_files(client, &uid)?;
    find_file_id(&files, name)
        .ok_or_else(|| AppError::user("nexus.file_unknown").with("name", name))
}

/// A name to save a download under when the CDN does not send `Content-Disposition`. The real archive name is the
/// preferred one; this is only the fallback, and the extension comes from the response type.
fn requested_name(file: &FileVersion) -> String {
    file.name.trim().to_string()
}

/// Downloads the first working CDN link into the game's downloads folder, under its name on Nexus, with its source
/// next to it.
fn save(
    dir: &Path,
    requested_name: &str,
    source: NexusSource,
    sidecar: SourceRef,
    tier: AccountTier,
    links: &DownloadLinks,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<PathBuf, AppError> {
    // The name when neither the CDN nor the requested name gives one: the mod, and the file when it is known.
    let fallback = source.file_id.map_or_else(
        || format!("nexus-{}", source.mod_id),
        |file_id| format!("nexus-{}-{file_id}", source.mod_id),
    );
    let target = |info: &DownloadInfo, url: &str| {
        let name = target_name(info, url, requested_name, &fallback);
        free_download_name(dir, &name, Some(&SourceRef::from(source)))
    };
    let urls: Vec<&str> = links.links.iter().map(|link| link.url.as_str()).collect();
    let rules = ArchiveRules {
        provider: super::super::ID,
        tier,
        sidecar: Some(sidecar),
        target: &target,
        body_error: &|problem: BodyProblem| match problem {
            BodyProblem::Empty => {
                crate::providers::bad_response("nexus.bad_response", "the CDN sent an empty file")
            }
            BodyProblem::Html => AppError::user("nexus.link_expired")
                .with("detail", "the CDN sent an HTML page instead of an archive"),
        },
        no_link: crate::providers::bad_response("nexus.bad_response", "no download link"),
    };
    fetch_archive(dir, &urls, rules, progress).map_err(download_error)
}

/// A CDN that refuses every signed link is telling the user the file is unavailable or the link expired, not that the
/// HTTP server is broken. A signed link is only issued after the account tier was accepted, so a 403 here is a genuine
/// moderation or quarantine refusal, not a missing entitlement.
pub(super) fn download_error(error: AppError) -> AppError {
    if error.code != "http.status" {
        return error;
    }
    match error.params.get("code").map(String::as_str) {
        Some("410") => AppError::user("nexus.link_expired"),
        Some("403" | "404") => AppError::user(crate::error::DOWNLOAD_UNAVAILABLE),
        _ => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;

    #[test]
    fn a_refused_cdn_link_becomes_a_clear_download_error() {
        let error = AppError::new("http.status")
            .with("code", 403)
            .kind(ErrorKind::Permanent);
        assert_eq!(
            download_error(error).code,
            crate::error::DOWNLOAD_UNAVAILABLE
        );
        let expired = AppError::new("http.status")
            .with("code", 410)
            .kind(ErrorKind::Permanent);
        assert_eq!(download_error(expired).code, "nexus.link_expired");
        let network = AppError::retryable("http.network");
        assert_eq!(download_error(network).code, "http.network");
    }
}
