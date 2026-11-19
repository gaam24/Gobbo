//! Thunderstore names and links: the `ror2mm://` install links of the "Install with Mod Manager" button, the `owner-name`
//! id of a package, and the dependency strings of a version. Pure text rules; no request is made here.
//!
//! Thunderstore allows only letters, digits and `_` in an owner or a package name, so a dash always separates the
//! parts of `owner-name` and of `owner-name-version`.

use crate::error::AppError;
use crate::providers::link::split_link;

/// The protocol version in the host of an install link. Only `v1` is known.
const PROTOCOL: &str = "v1";
/// The one Thunderstore server Gobbo downloads from.
const SERVER: &str = "thunderstore.io";

/// A package at one version: the owner, the package name and the version number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageVersion {
    pub owner: String,
    pub name: String,
    pub version: String,
}

/// Reads a `ror2mm://v1/install/thunderstore.io/<owner>/<name>/<version>/` link. The link names the package and the
/// version, not the game: the game comes from the package's community. Another protocol version, another server, or
/// any other shape is `thunderstore.bad_link`.
pub fn parse_ror2mm(url: &str) -> Result<PackageVersion, AppError> {
    let bad = || AppError::user("thunderstore.bad_link").with("url", url);
    let link = split_link(url, "ror2mm").ok_or_else(bad)?;
    // A non-special scheme keeps the host's case, so the protocol version is compared without it.
    if !link.host().eq_ignore_ascii_case(PROTOCOL) {
        return Err(bad());
    }
    let parts = link.segments();
    let ["install", server, owner, name, version] = parts.as_slice() else {
        return Err(bad());
    };
    if !server.eq_ignore_ascii_case(SERVER)
        || !is_part(owner)
        || !is_part(name)
        || !is_version(version)
    {
        return Err(bad());
    }
    Ok(PackageVersion {
        owner: (*owner).to_string(),
        name: (*name).to_string(),
        version: (*version).to_string(),
    })
}

/// The package id `owner-name` and its two parts, when the text is one. A dash anywhere else is not a package id.
pub fn split_full_name(full_name: &str) -> Option<(&str, &str)> {
    let (owner, name) = full_name.split_once('-')?;
    (is_part(owner) && is_part(name)).then_some((owner, name))
}

/// The package id `owner-name` of an owner and a name.
pub fn full_name(owner: &str, name: &str) -> String {
    format!("{owner}-{name}")
}

/// A package from its three parts, when each part has the shape Thunderstore uses.
fn package(owner: &str, name: &str, version: &str) -> Option<PackageVersion> {
    (is_part(owner) && is_part(name) && is_version(version)).then(|| PackageVersion {
        owner: owner.to_string(),
        name: name.to_string(),
        version: version.to_string(),
    })
}

/// A dependency string of a version, `owner-name-version`, as the API lists it.
pub fn split_dependency(text: &str) -> Option<PackageVersion> {
    let mut parts = text.rsplitn(3, '-');
    let version = parts.next()?;
    let name = parts.next()?;
    let owner = parts.next()?;
    package(owner, name, version)
}

/// The name of a downloaded archive, `owner-name-version.zip`, as the download gives it. `None` for any other name.
pub fn archive_package(file_name: &str) -> Option<PackageVersion> {
    let stem = crate::domain::archive::strip_extension(file_name);
    let mut parts = stem.splitn(3, '-');
    let owner = parts.next()?;
    let name = parts.next()?;
    let version = parts.next()?;
    package(owner, name, version)
}

/// An owner or a package name: letters, digits and `_`, at least one.
pub fn is_part(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// A version number: digits separated by dots, for example `5.0.5`.
pub fn is_version(text: &str) -> bool {
    text.len() <= 64
        && !text.is_empty()
        && text
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_install_link_names_the_package_and_the_version() {
        let link =
            parse_ror2mm("ror2mm://v1/install/thunderstore.io/example_owner/example_mod/5.0.5/")
                .unwrap();
        assert_eq!(
            link,
            PackageVersion {
                owner: "example_owner".to_string(),
                name: "example_mod".to_string(),
                version: "5.0.5".to_string(),
            }
        );
        let bare = parse_ror2mm("ror2mm://v1/install/thunderstore.io/other_owner/other_mod/3.1.25")
            .unwrap();
        assert_eq!(
            (bare.owner.as_str(), bare.version.as_str()),
            ("other_owner", "3.1.25")
        );
    }

    #[test]
    fn the_scheme_the_host_and_the_server_are_case_insensitive() {
        let link = parse_ror2mm("ROR2MM://V1/install/Thunderstore.IO/a_b/c/1.0.0/").unwrap();
        assert_eq!((link.owner.as_str(), link.name.as_str()), ("a_b", "c"));
    }

    #[test]
    fn other_links_are_refused_with_the_thunderstore_code() {
        for link in [
            "https://thunderstore.io/package/other_owner/other_mod/",
            "ror2mm://v2/install/thunderstore.io/a/b/1.0.0/",
            "ror2mm://v1/install/example.com/a/b/1.0.0/",
            "ror2mm://v1/launch/thunderstore.io/a/b/1.0.0/",
            "ror2mm://v1/install/thunderstore.io/a/b/",
            "ror2mm://v1/install/thunderstore.io/a/b/1.0.0/extra/",
            "ror2mm://v1/install/thunderstore.io/a-b/c/1.0.0/",
            "ror2mm://v1/install/thunderstore.io/a/b/latest/",
            "ror2mm://v1/install/thunderstore.io/../b/1.0.0/",
            "ror2mm://v1:99/install/thunderstore.io/a/b/1.0.0/",
            "ror2mm",
        ] {
            let error = parse_ror2mm(link).unwrap_err();
            assert_eq!(error.code, "thunderstore.bad_link", "{link}");
        }
    }

    #[test]
    fn a_package_id_has_exactly_one_dash() {
        assert_eq!(
            split_full_name("example_owner-example_mod"),
            Some(("example_owner", "example_mod"))
        );
        assert_eq!(split_full_name("example_mod"), None);
        assert_eq!(split_full_name("a-b-c"), None);
        assert_eq!(split_full_name("a-"), None);
        assert_eq!(split_full_name("a/b-c"), None);
        assert_eq!(
            full_name("other_owner", "other_mod"),
            "other_owner-other_mod"
        );
    }

    #[test]
    fn a_dependency_string_splits_into_owner_name_and_version() {
        let dependency = split_dependency("example_dep-example_core-5.0.10").unwrap();
        assert_eq!(
            (
                dependency.owner.as_str(),
                dependency.name.as_str(),
                dependency.version.as_str()
            ),
            ("example_dep", "example_core", "5.0.10")
        );
        assert_eq!(split_dependency("other_owner-BepInExPack"), None);
        assert_eq!(split_dependency("other_owner-BepInExPack-5.4.x"), None);
    }

    #[test]
    fn an_archive_name_of_a_download_names_the_package() {
        let package = archive_package("example_owner-example_mod-5.0.5.zip").unwrap();
        assert_eq!(
            (
                package.owner.as_str(),
                package.name.as_str(),
                package.version.as_str()
            ),
            ("example_owner", "example_mod", "5.0.5")
        );
        assert!(archive_package("example_owner-example_mod-5.0.5.ZIP").is_some());
        assert!(archive_package("Some Mod 1.2.zip").is_none());
        assert!(archive_package("a-b-c-1.0.0.zip").is_none());
        assert!(archive_package("a-b-1.0.0.txt").is_none());
    }

    #[test]
    fn a_community_is_a_slug_of_letters_digits_dashes_and_underscores() {
        use crate::providers::link::is_slug;
        assert!(is_slug("example-community", true));
        assert!(is_slug("other-community", true));
        assert!(!is_slug("", true));
        assert!(!is_slug("../etc", true));
        assert!(!is_slug("a/b", true));
    }
}
