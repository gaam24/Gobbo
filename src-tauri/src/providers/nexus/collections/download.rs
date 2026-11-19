use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::AppError;
use crate::infra::http::{BodyProblem, DownloadInfo};
use crate::providers::download::{ArchiveRules, fetch_archive, target_name};
use crate::providers::nexus::api::{NexusClient, bad_response};

#[derive(Debug, Deserialize)]
struct ArchiveLinks {
    #[serde(default)]
    download_links: Vec<ArchiveLink>,
    download_link: Option<ArchiveLink>,
}

#[derive(Debug, Deserialize)]
struct ArchiveLink {
    #[serde(rename = "URI", alias = "uri", alias = "url")]
    url: String,
}

pub(super) fn download_archive(
    key: &str,
    game_key: &str,
    archive_link: &str,
    directory: &Path,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<PathBuf, AppError> {
    if !archive_link.starts_with("/v2/collections/") || archive_link.contains("..") {
        return Err(AppError::user("collection.bad_link").with("url", archive_link));
    }
    let client = NexusClient::new(key);
    let text = client.get_v2(archive_link)?;
    let links: ArchiveLinks = serde_json::from_str(&text).map_err(bad_response)?;
    let urls: Vec<&str> = links
        .download_links
        .iter()
        .chain(links.download_link.iter())
        .map(|link| link.url.as_str())
        .collect();
    let target = |info: &DownloadInfo, url: &str| {
        let name = target_name(info, url, "", "collection");
        let name = if crate::domain::archive::extension_of(&name).is_some() {
            name
        } else {
            format!("{name}.7z")
        };
        directory.join(crate::domain::naming::safe_limited_file_name(&name))
    };
    let rules = ArchiveRules {
        // A collection recipe is one file, not a burst of mod downloads: it never takes a slot from the free tier's
        // safety budget.
        provider: super::ID,
        tier: crate::domain::rate::AccountTier::Anonymous,
        sidecar: None,
        target: &target,
        body_error: &|problem: BodyProblem| match problem {
            BodyProblem::Empty => AppError::user("collection.bad_archive")
                .with("detail", "the server sent an empty archive"),
            BodyProblem::Html => AppError::user("collection.bad_archive")
                .with("detail", "the server sent a web page instead of an archive"),
        },
        no_link: AppError::new("collection.download_failed")
            .with("slug", archive_link)
            .with("code", "no link"),
    };
    let path = fetch_archive(directory, &urls, rules, progress).map_err(|error| {
        if error.code == "http.status" {
            AppError::user("collection.download_failed")
                .with("slug", game_key)
                .with(
                    "code",
                    error.params.get("code").cloned().unwrap_or_default(),
                )
        } else {
            error
        }
    })?;
    Ok(path)
}
