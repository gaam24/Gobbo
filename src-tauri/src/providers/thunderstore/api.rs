//! The Thunderstore requests: the package record of the experimental API, read as JSON, and the address of a version's
//! archive. Public packages need no key.

use serde::Deserialize;
use serde::de::DeserializeOwned;

use crate::error::AppError;
use crate::infra::http;

/// The package endpoint: a package is named by its owner and name. The API is marked experimental by Thunderstore;
/// the rest of the module reads only the fields it names here.
const PACKAGE_API: &str = "https://thunderstore.io/api/experimental/package";
/// The archive of one version. Thunderstore answers it with a redirect to the file.
const DOWNLOAD: &str = "https://thunderstore.io/package/download";

/// A package as the API describes it: its owner, name, dates, the communities it is listed in, and its newest version.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PackageRecord {
    pub namespace: String,
    pub name: String,
    pub date_created: Option<String>,
    pub date_updated: Option<String>,
    pub latest: Option<VersionRecord>,
    pub community_listings: Vec<ListingRecord>,
}

/// One version of a package. `dependencies` are `owner-name-version` strings.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct VersionRecord {
    pub version_number: String,
    pub description: String,
    pub icon: String,
    pub dependencies: Vec<String>,
    pub downloads: u64,
    pub website_url: String,
}

/// The listing of a package in one community.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ListingRecord {
    pub community: String,
    pub categories: Vec<String>,
    pub has_nsfw_content: bool,
    pub review_status: String,
}

/// Reads a package by its owner and name.
pub fn read_package(owner: &str, name: &str) -> Result<PackageRecord, AppError> {
    get_json(&format!("{PACKAGE_API}/{owner}/{name}/"))
}

/// Reads one version of a package, for a modpack revision that is not the newest.
pub fn read_version(owner: &str, name: &str, version: &str) -> Result<VersionRecord, AppError> {
    get_json(&format!("{PACKAGE_API}/{owner}/{name}/{version}/"))
}

/// The address of one version's archive.
pub fn download_url(owner: &str, name: &str, version: &str) -> String {
    format!("{DOWNLOAD}/{owner}/{name}/{version}/")
}

/// The package's newest version, or an error when the API did not name one. Borrowing the record lets a read-only
/// caller keep the package it already owns.
pub fn latest(package: &PackageRecord) -> Result<&VersionRecord, AppError> {
    package
        .latest
        .as_ref()
        .ok_or_else(|| bad_response("the package names no version"))
}

/// A package id that is not `owner-name`.
pub fn bad_id(mod_id: &str) -> AppError {
    AppError::user("thunderstore.bad_id").with("id", mod_id)
}

/// The package or the version is not on Thunderstore. This is the one 404 code of the provider, paired with
/// `nexus.not_found` across the provider family.
pub fn not_found() -> AppError {
    AppError::user(crate::providers::thunderstore::THUNDERSTORE_NOT_FOUND)
}

/// The server sent something that is not the expected data.
pub fn bad_response(detail: impl ToString) -> AppError {
    crate::providers::bad_response("thunderstore.bad_response", detail)
}

/// One GET that answers JSON. A 404 is a package that does not exist; other failures keep the shared HTTP codes.
fn get_json<T: DeserializeOwned>(url: &str) -> Result<T, AppError> {
    let response = http::Client::new("thunderstore")
        .header("Accept", "application/json")
        .get(url, &[])
        .map_err(|error| http::network("thunderstore", error))?;
    if response.status == 404 {
        return Err(not_found());
    }

    if !(200..300).contains(&response.status) {
        return Err(http::status_error("thunderstore", response.status));
    }

    serde_json::from_str(&response.body).map_err(bad_response)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape of a live package record (the fields the app reads are the ones that matter here).
    const PACKAGE: &str = r#"{
        "namespace": "example_owner", "name": "example_mod", "full_name": "example_owner-example_mod",
        "owner": "example_owner", "package_url": "https://thunderstore.io/package/example_owner/example_mod/",
        "date_created": "2019-05-21T05:13:02.761422Z", "date_updated": "2023-06-06T00:12:03.808487Z",
        "rating_score": -1, "is_pinned": true, "is_deprecated": false, "total_downloads": -1,
        "latest": {
            "namespace": "example_owner", "name": "example_mod", "version_number": "5.0.5",
            "full_name": "example_owner-example_mod-5.0.5", "description": "An example mod",
            "icon": "https://ccdn.thunderstore.io/live/repository/icons/example_owner-example_mod-5.0.5.png",
            "dependencies": ["example_dep-example_patcher-1.2.3", "other_owner-BepInExPack-5.4.2109"],
            "download_url": "https://thunderstore.io/package/download/example_owner/example_mod/5.0.5/",
            "downloads": 2631513, "date_created": "2023-06-06T00:12:02.453549Z",
            "website_url": "https://github.com/example-org/example_mod", "is_active": true
        },
        "community_listings": [
            {"has_nsfw_content": false, "categories": ["Libraries"], "community": "example-community", "review_status": "approved"}
        ]
    }"#;

    #[test]
    fn a_live_package_record_reads_into_the_fields_the_app_uses() {
        let package: PackageRecord = serde_json::from_str(PACKAGE).unwrap();
        assert_eq!(
            (package.namespace.as_str(), package.name.as_str()),
            ("example_owner", "example_mod")
        );
        let latest = latest(&package).unwrap();
        assert_eq!(latest.version_number, "5.0.5");
        assert_eq!(latest.dependencies.len(), 2);
        assert_eq!(latest.downloads, 2_631_513);
        assert_eq!(package.community_listings[0].community, "example-community");
    }

    #[test]
    fn a_package_without_a_version_is_a_bad_response() {
        let package: PackageRecord =
            serde_json::from_str(r#"{"namespace":"a","name":"b"}"#).unwrap();
        assert_eq!(
            latest(&package).unwrap_err().code,
            "thunderstore.bad_response"
        );
    }

    #[test]
    fn the_download_address_names_the_version() {
        assert_eq!(
            download_url("example_owner", "example_mod", "5.0.5"),
            "https://thunderstore.io/package/download/example_owner/example_mod/5.0.5/"
        );
    }
}
