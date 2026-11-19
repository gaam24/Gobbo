//! Reading the Nexus file and download-link answers into the shared types.

use serde::Deserialize;

use super::{DownloadLink, DownloadLinks, FileVersion, ModFile};
use crate::domain::archive::is_number;
use crate::error::AppError;
use crate::infra::clock::unix_time;
use crate::providers::nexus::api::{Envelope, Named, bad_response};

/// The versions of one persistent file.
#[derive(Debug, Clone, Deserialize)]
pub(super) struct ModFileGroup {
    pub(super) id: String,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) file_category: Option<String>,
    #[serde(default = "default_active")]
    pub(super) is_active: bool,
}

const fn default_active() -> bool {
    true
}

/// Parses the file version response. The global id is the one the dependencies endpoint needs, and `file` names the
/// persistent group that the version belongs to.
pub(super) fn parse_file_version(text: &str, requested: u32) -> Result<FileVersion, AppError> {
    #[derive(Deserialize)]
    struct Version {
        id: String,
        #[serde(default)]
        game_scoped_id: Option<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        file_category: Option<String>,
        #[serde(default)]
        file: Option<Named>,
    }
    let version: Version = serde_json::from_str::<Envelope<Version>>(text)
        .map_err(bad_response)?
        .data;
    if !is_number(&version.id) {
        return Err(bad_response("version id"));
    }
    let file_id = version
        .game_scoped_id
        .as_deref()
        .and_then(|id| id.parse().ok())
        .unwrap_or(requested);
    let file = version.file.unwrap_or(Named {
        id: None,
        name: None,
    });
    let file_name = file.name.filter(|name| !name.trim().is_empty());
    Ok(FileVersion {
        version_id: version.id,
        file_id,
        mod_file_id: file.id.filter(|id| is_number(id)),
        mod_file_name: file_name.clone(),
        file_category: version.file_category,
        name: version
            .name
            .filter(|name| !name.trim().is_empty())
            .or(file_name)
            .unwrap_or_else(|| format!("file {file_id}")),
        version: version.version.unwrap_or_default(),
    })
}

/// The persistent files of a mod.
pub(super) fn parse_mod_file_groups(text: &str) -> Result<Vec<ModFileGroup>, AppError> {
    #[derive(Deserialize)]
    struct Files {
        #[serde(default)]
        mod_files: Vec<ModFileGroup>,
    }
    Ok(serde_json::from_str::<Envelope<Files>>(text)
        .map_err(bad_response)?
        .data
        .mod_files)
}

/// The versions inside one persistent file.
pub(super) fn parse_file_versions(text: &str, group: &ModFileGroup) -> Vec<ModFile> {
    #[derive(Deserialize)]
    struct Versions {
        #[serde(default)]
        versions: Vec<Version>,
    }
    #[derive(Deserialize)]
    struct Version {
        #[serde(default)]
        game_scoped_id: Option<String>,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        position: Option<String>,
        #[serde(default)]
        uploaded_at: Option<String>,
        #[serde(default, alias = "file_category")]
        category: Option<String>,
    }
    let Ok(response) = serde_json::from_str::<Envelope<Versions>>(text) else {
        return Vec::new();
    };
    response
        .data
        .versions
        .into_iter()
        .filter_map(|version| {
            let file_id = version
                .game_scoped_id
                .as_deref()
                .and_then(|id| id.parse().ok())?;
            let file_name = version
                .name
                .filter(|name| !name.trim().is_empty())
                .or_else(|| group.name.clone())
                .or_else(|| version.version.clone())
                .unwrap_or_else(|| format!("file {file_id}"));
            Some(ModFile {
                file_id,
                mod_file_id: group.id.clone(),
                mod_file_name: group.name.clone(),
                file_name,
                version: version.version.filter(|version| !version.trim().is_empty()),
                file_category: group.file_category.clone(),
                version_category: version
                    .category
                    .filter(|category| !category.trim().is_empty()),
                position: version
                    .position
                    .filter(|position| !position.trim().is_empty()),
                uploaded_at: version.uploaded_at.as_deref().and_then(unix_time),
                is_active: group.is_active,
            })
        })
        .collect()
}

/// Parses the signed links, keeping only secure addresses.
pub(super) fn parse_download_links(text: &str) -> Result<DownloadLinks, AppError> {
    #[derive(Deserialize)]
    struct Data {
        #[serde(default)]
        links: Vec<Link>,
        #[serde(default)]
        expires_at: Option<String>,
    }
    #[derive(Deserialize)]
    struct Link {
        #[serde(default)]
        name: String,
        #[serde(default)]
        short_name: String,
        #[serde(default)]
        url: Option<String>,
    }
    let data = serde_json::from_str::<Envelope<Data>>(text)
        .map_err(bad_response)?
        .data;
    let links: Vec<DownloadLink> = data
        .links
        .into_iter()
        .filter_map(|link| {
            let url = link.url.filter(|url| url.starts_with("https://"))?;
            Some(DownloadLink {
                name: link.name,
                short_name: link.short_name,
                url,
            })
        })
        .collect();
    if links.is_empty() {
        return Err(bad_response("no download link"));
    }
    Ok(DownloadLinks {
        links,
        expires_at: data.expires_at,
    })
}
