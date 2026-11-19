//! The files of a mod on Nexus, their versions and the signed links each one downloads from.

use super::api::{Endpoint, NexusClient, bad_response};
use crate::domain::archive::is_number;
use crate::error::AppError;

mod matching;
mod order;
mod parse;

pub use matching::{find_file_id, newest_match};
use order::{is_active_version, newest_order};
use parse::ModFileGroup;
use parse::{parse_download_links, parse_file_version, parse_file_versions, parse_mod_file_groups};

/// A file of a mod on Nexus, as its versions list names it. `file_id` is the game-scoped id that an `nxm://` link and
/// the download endpoint use. `mod_file_id` is the persistent group: versions sharing it replace each other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModFile {
    pub file_id: u32,
    /// The persistent `mod_file_id`, the update chain of one installable file.
    pub mod_file_id: String,
    /// The group's display name, when the group list named it.
    pub mod_file_name: Option<String>,
    pub file_name: String,
    pub version: Option<String>,
    /// The provider's category for the file group, for example `main` or `optional`.
    pub file_category: Option<String>,
    /// The provider's category for this version, for example `main` or `archived`.
    pub version_category: Option<String>,
    /// The position of the version within its file group. Nexus appends a new version after the latest one, so a
    /// higher position is a newer version even when its version string sorts lower.
    pub position: Option<String>,
    /// Unix time in seconds of the upload, when Nexus wrote a date the app can read.
    pub uploaded_at: Option<u32>,
    /// Whether Nexus still lists the file group as active.
    pub is_active: bool,
}

/// One version of a mod file: the game-scoped id from an NXM link, the global id its dependencies are asked by, and the
/// name and version the download is saved under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVersion {
    /// The global version id, used by `/mod-file-versions/{id}/dependencies`.
    pub version_id: String,
    /// The game-scoped id, used by an NXM link and `/games/{domain}/mod-file-versions/{id}`.
    pub file_id: u32,
    /// The persistent group id, when the provider returned the file object.
    pub mod_file_id: Option<String>,
    /// The group's display name, when the provider returned it.
    pub mod_file_name: Option<String>,
    /// The provider's category for the group, for example `main` or `optional`.
    pub file_category: Option<String>,
    pub name: String,
    pub version: String,
}

/// One signed CDN link of a file version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadLink {
    pub name: String,
    pub short_name: String,
    pub url: String,
}

/// Every signed link of one file version, best CDN location first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadLinks {
    pub links: Vec<DownloadLink>,
    /// The time the links stop working, as Nexus wrote it.
    pub expires_at: Option<String>,
}

/// One file version by its game-scoped id (the `file_id` in an `nxm://` link).
pub fn file_version(
    client: &NexusClient,
    domain: &str,
    file_id: u32,
) -> Result<FileVersion, AppError> {
    let text = client.get_v3(
        &format!("/games/{domain}/mod-file-versions/{file_id}"),
        &[],
        Endpoint::File,
    )?;
    parse_file_version(&text, file_id)
}

/// Every file version of a mod, found from the mod's global id. The v3 file list names the persistent files; their
/// versions carry the game-scoped ids an archive name is matched against.
pub fn mod_files(client: &NexusClient, global_mod_id: &str) -> Result<Vec<ModFile>, AppError> {
    if !is_number(global_mod_id) {
        return Err(bad_response("mod id"));
    }
    let text = client.get_v3(&format!("/mods/{global_mod_id}/files"), &[], Endpoint::File)?;
    let groups = parse_mod_file_groups(&text)?;
    let mut files = Vec::new();
    for group in groups {
        let path = format!("/mod-files/{}/versions", group.id);
        // One unreadable group does not hide the versions of the others.
        let Ok(text) = client.get_v3(&path, &[], Endpoint::File) else {
            continue;
        };
        files.extend(parse_file_versions(&text, &group));
    }
    Ok(files)
}

/// Every version of one persistent file group, by its `mod_file_id`.
pub fn part_files(client: &NexusClient, mod_file_id: &str) -> Result<Vec<ModFile>, AppError> {
    if !is_number(mod_file_id) {
        return Err(bad_response("mod file id"));
    }
    let text = client.get_v3(
        &format!("/mod-files/{mod_file_id}/versions"),
        &[],
        Endpoint::File,
    )?;
    Ok(parse_file_versions(
        &text,
        &ModFileGroup {
            id: mod_file_id.to_string(),
            name: None,
            file_category: None,
            is_active: true,
        },
    ))
}

/// The newest active version of a persistent file group. Nexus orders the versions of one file group itself, so the
/// position and upload time decide, not the version string: a hotfix such as `1.1.3a` is newer than `1.1.3` even
/// though the version string sorts lower. The file id is the last fallback for an old response without either field.
pub fn newest_part_file(files: &[ModFile]) -> Option<&ModFile> {
    files
        .iter()
        .filter(|file| is_active_version(file))
        .max_by(|left, right| newest_order(left, right))
}

/// The signed links of a file version. With an `nxm://` key any account may ask; without one the account must be
/// premium or the file must allow direct downloads.
pub fn download_links(
    client: &NexusClient,
    domain: &str,
    file_id: u32,
    key: Option<&(String, u64)>,
) -> Result<DownloadLinks, AppError> {
    let expires = key.map(|(_, expires)| expires.to_string());
    let mut query: Vec<(&str, &str)> = Vec::new();
    if let (Some((key, _)), Some(expires)) = (key, expires.as_deref()) {
        query.push(("key", key.as_str()));
        query.push(("expires", expires));
    }
    let text = client.get_v3(
        &format!("/games/{domain}/mod-file-versions/{file_id}/download-link"),
        &query,
        Endpoint::DownloadLink {
            has_key: key.is_some(),
        },
    )?;
    parse_download_links(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mod_file(id: u32, name: &str, version: &str) -> ModFile {
        ModFile {
            file_id: id,
            mod_file_id: "100".into(),
            mod_file_name: Some("CET".into()),
            file_name: name.into(),
            version: Some(version.into()),
            file_category: Some("main".into()),
            version_category: Some("main".into()),
            position: None,
            uploaded_at: None,
            is_active: true,
        }
    }

    #[test]
    fn a_file_version_gives_both_ids_and_the_name() {
        let text = r#"{"data":{"id":"14315126166183","game_scoped_id":"168615",
            "name":"CET 1.38.1 - Game version 2.5","version":"1.38.1",
            "file":{"id":"1","name":"Main files"}}}"#;
        let version = parse_file_version(text, 168_615).unwrap();
        assert_eq!(version.version_id, "14315126166183");
        assert_eq!(version.file_id, 168_615);
        assert_eq!(version.mod_file_id.as_deref(), Some("1"));
        assert_eq!(version.mod_file_name.as_deref(), Some("Main files"));
        assert_eq!(version.name, "CET 1.38.1 - Game version 2.5");
        assert_eq!(version.version, "1.38.1");
    }

    #[test]
    fn a_version_without_the_extra_fields_still_has_its_ids() {
        let text = r#"{"data":{"id":"1"}}"#;
        let version = parse_file_version(text, 7).unwrap();
        assert_eq!(version.version_id, "1");
        assert_eq!(version.file_id, 7);
        assert_eq!(version.name, "file 7");
        assert!(version.version.is_empty());
    }

    #[test]
    fn a_version_id_that_is_not_a_number_is_refused() {
        assert!(parse_file_version(r#"{"data":{"id":"1/../2"}}"#, 1).is_err());
    }

    #[test]
    fn the_download_links_keep_the_secure_ones_in_order() {
        let text = r#"{"data":{"links":[
            {"name":"Nexus CDN","short_name":"CDN","url":"https://cf-files.nexusmods.com/a.zip?md5=x"},
            {"name":"Plain","short_name":"P","url":"http://plain"}],
            "expires_at":"2026-10-31T12:00:00Z"}}"#;
        let links = parse_download_links(text).unwrap();
        assert_eq!(links.links.len(), 1);
        assert_eq!(
            links.links[0].url,
            "https://cf-files.nexusmods.com/a.zip?md5=x"
        );
        assert_eq!(links.expires_at.as_deref(), Some("2026-10-31T12:00:00Z"));
        assert!(parse_download_links(r#"{"data":{"links":[]}}"#).is_err());
    }

    #[test]
    fn a_file_group_keeps_its_name_and_category() {
        let text = r#"{"data":{"mod_files":[{"id":"100","name":"Better Movement",
            "file_category":"main","is_active":true}]}}"#;
        let groups = parse_mod_file_groups(text).unwrap();
        assert_eq!(groups[0].id, "100");
        assert_eq!(groups[0].name.as_deref(), Some("Better Movement"));
        assert_eq!(groups[0].file_category.as_deref(), Some("main"));
        let versions = parse_file_versions(
            r#"{"data":{"versions":[{"game_scoped_id":"9","name":"Better Movement 1.0","version":"1.0"}]}}"#,
            &groups[0],
        );
        assert_eq!(versions[0].mod_file_id, "100");
        assert_eq!(
            versions[0].mod_file_name.as_deref(),
            Some("Better Movement")
        );
        assert_eq!(versions[0].file_category.as_deref(), Some("main"));
    }

    #[test]
    fn a_version_carries_its_position_upload_time_and_category() {
        let text = r#"{"data":{"mod_files":[{"id":"100","name":"Better Movement",
            "file_category":"main","is_active":true}]}}"#;
        let groups = parse_mod_file_groups(text).unwrap();
        let versions = parse_file_versions(
            r#"{"data":{"versions":[{"game_scoped_id":"20","name":"Better Movement 1.1.3a",
                "version":"1.1.3a","position":"2","uploaded_at":"2025-10-26T02:00:00.000+02:00",
                "category":"main"}]}}"#,
            &groups[0],
        );
        assert_eq!(versions[0].position.as_deref(), Some("2"));
        assert_eq!(versions[0].uploaded_at, Some(1_761_436_800));
        assert_eq!(versions[0].version_category.as_deref(), Some("main"));
    }

    fn part_file(
        file_id: u32,
        version: &str,
        position: Option<&str>,
        uploaded_at: Option<u32>,
        category: Option<&str>,
    ) -> ModFile {
        ModFile {
            file_id,
            mod_file_id: "100".into(),
            mod_file_name: Some("Addon".into()),
            file_name: format!("Addon {version}"),
            version: Some(version.into()),
            file_category: Some("main".into()),
            version_category: category.map(str::to_string),
            position: position.map(str::to_string),
            uploaded_at,
            is_active: true,
        }
    }

    #[test]
    fn a_hotfix_after_the_base_version_is_the_newest() {
        // Nexus appends a new version after the latest one, so the position wins over the version string.
        let files = [
            part_file(10, "1.1.3", Some("1"), Some(100), None),
            part_file(20, "1.1.3a", Some("2"), Some(200), None),
        ];
        assert_eq!(newest_part_file(&files).map(|file| file.file_id), Some(20));
    }

    #[test]
    fn positions_compare_as_numbers() {
        let files = [
            part_file(20, "1.9.0", Some("9"), None, None),
            part_file(30, "1.10.0", Some("10"), None, None),
        ];
        assert_eq!(newest_part_file(&files).map(|file| file.file_id), Some(30));
    }

    #[test]
    fn decimal_positions_compare_as_decimals() {
        let files = [
            part_file(20, "1.5", Some("1.5"), None, None),
            part_file(30, "1.25", Some("1.25"), None, None),
        ];
        assert_eq!(newest_part_file(&files).map(|file| file.file_id), Some(20));
    }

    #[test]
    fn an_archived_version_is_not_the_newest() {
        let files = [
            part_file(10, "1.1.3", Some("1"), Some(100), Some("main")),
            part_file(20, "1.1.3a", Some("2"), Some(200), Some("archived")),
        ];
        assert_eq!(newest_part_file(&files).map(|file| file.file_id), Some(10));
    }

    #[test]
    fn without_positions_the_newest_upload_wins() {
        let files = [
            part_file(30, "1.0", None, Some(100), None),
            part_file(10, "2.0", None, Some(200), None),
        ];
        assert_eq!(newest_part_file(&files).map(|file| file.file_id), Some(10));
    }

    #[test]
    fn without_positions_or_dates_the_file_id_wins() {
        let files = [
            part_file(30, "1.0", None, None, None),
            part_file(40, "0.9", None, None, None),
        ];
        assert_eq!(newest_part_file(&files).map(|file| file.file_id), Some(40));
    }

    #[test]
    fn an_archive_name_matches_the_version_name_before_its_nexus_suffix() {
        let files = [
            mod_file(10, "CET 1.37.0", "1.37.0"),
            mod_file(20, "CET 1.38.1 - Game version 2.5", "1.38.1"),
        ];
        assert_eq!(
            find_file_id(
                &files,
                "CET 1.38.1 - Game version 2.5-107-1-38-1-1759193708.zip"
            ),
            Some(20)
        );
        assert_eq!(
            find_file_id(&files, "CET 1.37.0-107-1-37-0-1757629209.zip"),
            Some(10)
        );
        assert_eq!(find_file_id(&files, "Other.zip"), None);
    }

    #[test]
    fn the_most_specific_version_name_wins() {
        let files = [
            mod_file(30, "CET", "1.0"),
            mod_file(20, "CET 1.0 hotfix", "1.0.1"),
        ];
        assert_eq!(
            find_file_id(&files, "CET 1.0 hotfix-107-1-0-1-1759193708.zip"),
            Some(20)
        );
    }
}
