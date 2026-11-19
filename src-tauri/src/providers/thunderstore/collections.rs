//! Thunderstore modpacks as collections. A modpack is a package whose version lists its members as dependencies
//! (`owner-name-version`), so the API is the whole recipe: there is no archive, no installer choice and no rule to read.
//! The collection feature installs each member through the normal Thunderstore download.

use std::path::{Path, PathBuf};

use url::Url;

use super::api;
use super::link::{full_name, is_part, is_version, split_dependency, split_full_name};
use crate::domain::collection::CollectionInfo;
use crate::domain::source::ProviderId;
use crate::error::AppError;
use crate::providers::link::is_slug;
use crate::providers::{CollectionLink, CollectionMod, CollectionRevision, CollectionSource};

/// The Thunderstore modpack adapter.
pub struct ThunderstoreCollections;

impl CollectionSource for ThunderstoreCollections {
    fn parse_link(&self, url: &str) -> Result<CollectionLink, AppError> {
        parse_pack_link(url)
    }

    fn revision(
        &self,
        _key: &str,
        link: &CollectionLink,
        revision: Option<u32>,
    ) -> Result<CollectionRevision, AppError> {
        let (owner, name) = split_full_name(&link.slug)
            .ok_or_else(|| AppError::user("collection.bad_link").with("url", &link.slug))?;
        let package = api::read_package(owner, name)?;
        let latest = api::latest(&package)?;
        let wanted = revision.map(version_of_revision);
        let version = match wanted {
            Some(version) if version != latest.version_number => {
                api::read_version(owner, name, &version)?
            }
            _ => latest.clone(),
        };
        let adult = package
            .community_listings
            .iter()
            .any(|listing| listing.community == link.game_key && listing.has_nsfw_content);
        Ok(revision_of(
            link,
            &package.namespace,
            name,
            &version,
            &latest.version_number,
            adult,
        ))
    }

    fn download_archive(
        &self,
        _key: &str,
        _game_key: &str,
        _archive_link: &str,
        _directory: &Path,
        _progress: &mut dyn FnMut(u64, u64) -> bool,
    ) -> Result<PathBuf, AppError> {
        // A modpack has no archive beyond its dependency list; `complete` tells the caller not to ask.
        Err(AppError::user("collection.no_archive"))
    }

    fn page_url(&self, game_key: &str, slug: &str) -> Option<String> {
        let (owner, name) = split_full_name(slug)?;
        is_slug(game_key, true)
            .then(|| format!("https://thunderstore.io/c/{game_key}/p/{owner}/{name}/"))
    }
}

/// Reads a package page link: `https://thunderstore.io/c/<community>/p/<owner>/<name>/`, optionally with
/// `v/<version>/`, or the older `https://<community>.thunderstore.io/package/<owner>/<name>/`.
pub fn parse_pack_link(text: &str) -> Result<CollectionLink, AppError> {
    let bad = || AppError::user("collection.bad_link").with("url", text);
    let url = Url::parse(text.trim()).map_err(|_| bad())?;
    if url.scheme() != "https" || url.port().is_some() {
        return Err(bad());
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let parts: Vec<&str> = url
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let (community, owner, name, version) = match (host.as_str(), parts.as_slice()) {
        ("thunderstore.io", ["c", community, "p", owner, name, rest @ ..]) => (
            *community,
            *owner,
            *name,
            version_part(rest).map_err(|()| bad())?,
        ),
        (host, ["package", owner, name, rest @ ..]) if host.ends_with(".thunderstore.io") => {
            let community = host.trim_end_matches(".thunderstore.io");
            (
                community,
                *owner,
                *name,
                version_part(rest).map_err(|()| bad())?,
            )
        }
        _ => return Err(bad()),
    };
    if !is_slug(community, true) || !is_part(owner) || !is_part(name) {
        return Err(bad());
    }
    Ok(CollectionLink {
        provider: ProviderId::new(super::ID),
        game_key: community.to_string(),
        slug: full_name(owner, name),
        revision: version.map(revision_of_version),
        page_url: Some(text.trim().to_string()),
    })
}

/// The optional `v/<version>` tail of a page link: `Ok(None)` when there is none, an error when the tail is not a
/// version.
fn version_part<'a>(rest: &[&'a str]) -> Result<Option<&'a str>, ()> {
    match rest {
        [] => Ok(None),
        ["v", version] if is_version(version) => Ok(Some(*version)),
        _ => Err(()),
    }
}

/// A Thunderstore version as an ordered revision number: `major.minor.patch` packed so that a newer version is a larger
/// number. Parts above 999 (and a major above 4293) saturate; the label keeps the real text.
fn revision_of_version(version: &str) -> u32 {
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0));
    let major = parts.next().unwrap_or(0).min(4293);
    let minor = parts.next().unwrap_or(0).min(999);
    let patch = parts.next().unwrap_or(0).min(999);
    major * 1_000_000 + minor * 1_000 + patch
}

/// The version text of a revision number made by [`revision_of_version`].
fn version_of_revision(revision: u32) -> String {
    format!(
        "{}.{}.{}",
        revision / 1_000_000,
        (revision / 1_000) % 1_000,
        revision % 1_000
    )
}

/// The display name Thunderstore shows for a package: the name with `_` as spaces.
fn display_name(name: &str) -> String {
    name.replace('_', " ")
}

fn revision_of(
    link: &CollectionLink,
    owner: &str,
    name: &str,
    version: &api::VersionRecord,
    latest: &str,
    adult: bool,
) -> CollectionRevision {
    let mods: Vec<CollectionMod> = version
        .dependencies
        .iter()
        .filter_map(|text| split_dependency(text))
        .map(|dependency| CollectionMod {
            mod_id: full_name(&dependency.owner, &dependency.name),
            file_id: dependency.version.clone(),
            optional: false,
            name: display_name(&dependency.name),
            version: Some(dependency.version),
            author: Some(dependency.owner),
            category: None,
            summary: String::new(),
            picture_url: None,
            adult: false,
            file_name: None,
            size_kb: 0,
        })
        .collect();
    let revision = revision_of_version(&version.version_number);
    let info = CollectionInfo {
        provider: super::ID.to_string(),
        game_key: link.game_key.clone(),
        slug: link.slug.clone(),
        revision,
        latest_revision: revision_of_version(latest).max(revision),
        revision_label: format!("v{}", version.version_number),
        name: display_name(name),
        author: (!owner.is_empty()).then(|| owner.to_string()),
        summary: version.description.clone(),
        description: String::new(),
        instructions: String::new(),
        picture_url: None,
        adult,
        total_size_kb: 0,
        mod_count: u32::try_from(mods.len()).unwrap_or(u32::MAX),
        game_versions: Vec::new(),
        changelog: None,
        url: format!(
            "https://thunderstore.io/c/{}/p/{}/",
            link.game_key,
            link.slug.replacen('-', "/", 1)
        ),
    };
    CollectionRevision {
        revision,
        latest_revision: info.latest_revision,
        info,
        mods,
        external: Vec::new(),
        revisions: Vec::new(),
        archive_link: None,
        complete: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_community_page_link_names_the_pack() {
        let link =
            parse_pack_link("https://thunderstore.io/c/example-game/p/example_owner/Example_Pack/")
                .unwrap();
        assert_eq!(link.provider.as_str(), "thunderstore");
        assert_eq!(link.game_key, "example-game");
        assert_eq!(link.slug, "example_owner-Example_Pack");
        assert_eq!(link.revision, None);
    }

    #[test]
    fn a_version_tail_and_the_old_subdomain_link_are_read() {
        let link = parse_pack_link(
            "https://thunderstore.io/c/example-game/p/example_owner/Example_Pack/v/1.2.3/",
        )
        .unwrap();
        assert_eq!(link.revision, Some(1_002_003));
        let old =
            parse_pack_link("https://example-game.thunderstore.io/package/owner/pack/").unwrap();
        assert_eq!(old.game_key, "example-game");
        assert_eq!(old.slug, "owner-pack");
    }

    #[test]
    fn other_links_are_refused() {
        for text in [
            "http://thunderstore.io/c/game/p/owner/pack/",
            "https://example.com/c/game/p/owner/pack/",
            "https://thunderstore.io/c/game/p/owner/",
            "https://thunderstore.io/c/game/p/owner/pack/v/latest/",
            "https://www.nexusmods.com/games/game/collections/abc",
        ] {
            assert!(parse_pack_link(text).is_err(), "{text}");
        }
    }

    #[test]
    fn versions_pack_into_ordered_revisions_and_back() {
        assert!(revision_of_version("1.10.0") > revision_of_version("1.9.9"));
        assert!(revision_of_version("2.0.0") > revision_of_version("1.999.999"));
        assert_eq!(version_of_revision(revision_of_version("4.0.12")), "4.0.12");
    }

    #[test]
    fn the_dependencies_become_members_and_the_plan_is_complete() {
        let link = parse_pack_link("https://thunderstore.io/c/example-game/p/owner/Pack/").unwrap();
        let version = api::VersionRecord {
            version_number: "1.0.1".into(),
            description: "A pack".into(),
            dependencies: vec![
                "dep_owner-First_Mod-2.0.0".into(),
                "not a dependency".into(),
                "other-Second-0.1.0".into(),
            ],
            ..api::VersionRecord::default()
        };
        let revision = revision_of(&link, "owner", "Pack", &version, "1.2.0", false);
        assert!(revision.complete);
        assert_eq!(revision.info.revision_label, "v1.0.1");
        assert!(revision.info.latest_revision > revision.info.revision);
        assert_eq!(revision.mods.len(), 2);
        assert_eq!(revision.mods[0].mod_id, "dep_owner-First_Mod");
        assert_eq!(revision.mods[0].file_id, "2.0.0");
        assert_eq!(revision.mods[0].name, "First Mod");
        assert_eq!(
            revision.info.url,
            "https://thunderstore.io/c/example-game/p/owner/Pack/"
        );
    }
}
