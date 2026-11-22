//! Mapping the Thunderstore records to the provider-neutral source model: the page data of a package, the requirements
//! its newest version lists, and the communities a package is listed in.

use std::collections::{BTreeMap, BTreeSet};

use super::ID;
use super::api::{ListingRecord, PackageRecord, VersionRecord};
use super::link;
use crate::domain::source::{SourceMeta, SourceRef, SourceRequirement, SourceStats};

/// The page data of a package at its newest version, in the shared model. `picture` is the saved copy of the icon, when
/// there is one.
pub fn meta_of(
    package: &PackageRecord,
    version: &VersionRecord,
    picture: Option<String>,
) -> SourceMeta {
    let mut extra = BTreeMap::new();

    if let Some(website) = non_empty(&version.website_url) {
        extra.insert("website_url".to_string(), website);
    }

    let tags: BTreeSet<&str> = package
        .community_listings
        .iter()
        .flat_map(|listing| listing.categories.iter().map(String::as_str))
        .collect();
    SourceMeta {
        name: non_empty(&package.name),
        author: non_empty(&package.namespace),
        uploader: None,
        category: None,
        summary: non_empty(&version.description),
        description: None,
        picture_url: non_empty(&version.icon),
        picture,
        version: non_empty(&version.version_number),
        stats: SourceStats {
            downloads: u32::try_from(version.downloads).ok(),
            unique_downloads: None,
            endorsements: None,
            subscribers: None,
        },
        adult: package
            .community_listings
            .iter()
            .any(|listing| listing.has_nsfw_content),
        checked_at: None,
        updated_at: package
            .date_updated
            .as_deref()
            .and_then(crate::infra::clock::unix_time),
        created_at: package
            .date_created
            .as_deref()
            .and_then(crate::infra::clock::unix_time),
        status: None,
        tags: tags.into_iter().map(str::to_string).collect(),
        extra,
    }
}

/// The page data a check stores: the newest version's data with this check's picture, merged with the meta saved for the
/// same package. A picture this check could not save keeps the saved copy of the same address.
pub fn checked_meta(
    package: &PackageRecord,
    version: &VersionRecord,
    picture: Option<String>,
    saved: Option<&SourceMeta>,
    checked_at: u32,
) -> SourceMeta {
    SourceMeta::checked(meta_of(package, version, picture), saved, checked_at)
}

/// The packages the newest version depends on, as requirements. Thunderstore lists them for the version, so the user
/// cannot remove them here; a missing one is a warning.
pub fn requirements_of(version: &VersionRecord) -> Vec<SourceRequirement> {
    version
        .dependencies
        .iter()
        .filter_map(|text| link::split_dependency(text))
        .map(|dependency| SourceRequirement {
            source: SourceRef::new(ID, link::full_name(&dependency.owner, &dependency.name)),
            name: dependency.name,
            from_source: true,
            group: None,
            ignored: false,
        })
        .collect()
}

/// The communities whose listing is usable, in the order the API gives them. A rejected listing does not make the
/// package part of a game.
pub fn communities(package: &PackageRecord) -> Vec<String> {
    package
        .community_listings
        .iter()
        .filter(|listing| is_usable(listing))
        .map(|listing| listing.community.clone())
        .filter(|community| !community.is_empty())
        .collect()
}

/// A listing is usable when it is approved, not yet reviewed (Thunderstore shows those too), or names no status at all.
/// Any other status, such as `rejected`, does not count.
fn is_usable(listing: &ListingRecord) -> bool {
    matches!(
        listing.review_status.as_str(),
        "" | "approved" | "unreviewed"
    )
}

fn non_empty(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version() -> VersionRecord {
        VersionRecord {
            version_number: "5.0.5".to_string(),
            description: "A modding API".to_string(),
            icon: "https://example.invalid/icon.png".to_string(),
            dependencies: vec![
                "other_owner-BepInExPack-5.4.2109".to_string(),
                "example_dep-example_core-5.0.10".to_string(),
                "not a dependency".to_string(),
            ],
            downloads: 2_631_513,
            website_url: "https://github.com/example-org/example_mod".to_string(),
        }
    }

    fn package() -> PackageRecord {
        PackageRecord {
            namespace: "example_owner".to_string(),
            name: "example_mod".to_string(),
            date_created: Some("2019-05-21T05:13:02.761422Z".to_string()),
            date_updated: Some("2023-06-06T00:12:03.808487Z".to_string()),
            latest: Some(version()),
            community_listings: vec![
                ListingRecord {
                    community: "example-community".to_string(),
                    categories: vec!["Libraries".to_string()],
                    has_nsfw_content: false,
                    review_status: "approved".to_string(),
                },
                ListingRecord {
                    community: "pending-game".to_string(),
                    categories: Vec::new(),
                    has_nsfw_content: true,
                    review_status: "pending".to_string(),
                },
            ],
        }
    }

    #[test]
    fn the_page_data_maps_the_common_fields() {
        let meta = meta_of(&package(), &version(), None);
        assert_eq!(meta.name.as_deref(), Some("example_mod"));
        assert_eq!(
            meta.picture_url.as_deref(),
            Some("https://example.invalid/icon.png")
        );
        assert_eq!(meta.author.as_deref(), Some("example_owner"));
        assert_eq!(meta.version.as_deref(), Some("5.0.5"));
        assert_eq!(meta.summary.as_deref(), Some("A modding API"));
        assert_eq!(meta.stats.downloads, Some(2_631_513));
        assert_eq!(meta.tags, vec!["Libraries".to_string()]);
        assert!(
            meta.adult,
            "a pending listing still counts for the adult flag"
        );
        assert_eq!(meta.created_at, Some(1_558_415_582));
        assert_eq!(
            meta.extra.get("website_url").map(String::as_str),
            Some("https://github.com/example-org/example_mod")
        );
    }

    #[test]
    fn the_saved_picture_goes_with_the_page_data() {
        let meta = meta_of(
            &package(),
            &version(),
            Some("thunderstore-example_owner-example_mod.png".to_string()),
        );
        assert_eq!(
            meta.picture.as_deref(),
            Some("thunderstore-example_owner-example_mod.png")
        );
    }

    #[test]
    fn a_failed_picture_save_keeps_the_saved_picture_and_fields() {
        let saved = SourceMeta {
            picture_url: Some("https://example.invalid/icon.png".to_string()),
            picture: Some("thunderstore-example_owner-example_mod.png".to_string()),
            summary: Some("Saved summary".to_string()),
            extra: BTreeMap::from([("kept".to_string(), "yes".to_string())]),
            ..SourceMeta::default()
        };
        let meta = checked_meta(&package(), &version(), None, Some(&saved), 9);
        assert_eq!(
            meta.picture.as_deref(),
            Some("thunderstore-example_owner-example_mod.png"),
            "the saved copy stays when the new save fails"
        );
        assert_eq!(meta.version.as_deref(), Some("5.0.5"));
        assert_eq!(meta.summary.as_deref(), Some("A modding API"));
        assert_eq!(meta.extra.get("kept").map(String::as_str), Some("yes"));
        assert!(meta.extra.contains_key("website_url"));
    }

    #[test]
    fn a_changed_icon_address_keeps_no_saved_picture() {
        let saved = SourceMeta {
            picture_url: Some("https://example.invalid/old.png".to_string()),
            picture: Some("thunderstore-example_owner-example_mod.png".to_string()),
            ..SourceMeta::default()
        };
        let meta = checked_meta(&package(), &version(), None, Some(&saved), 9);
        assert_eq!(meta.picture, None);
    }

    #[test]
    fn the_requirements_name_the_packages_and_skip_a_bad_string() {
        let requirements = requirements_of(&version());
        let mods: Vec<&str> = requirements
            .iter()
            .map(|requirement| requirement.source.mod_id.as_str())
            .collect();
        assert_eq!(
            mods,
            ["other_owner-BepInExPack", "example_dep-example_core"]
        );
        assert_eq!(requirements[0].name, "BepInExPack");
        assert!(
            requirements
                .iter()
                .all(|requirement| requirement.from_source)
        );
        assert!(
            requirements
                .iter()
                .all(|requirement| requirement.source.provider.as_str() == ID)
        );
    }

    #[test]
    fn only_approved_listings_name_a_community() {
        assert_eq!(communities(&package()), ["example-community"]);
    }

    #[test]
    fn an_unreviewed_listing_still_names_its_community() {
        let mut package = package();
        package.community_listings = vec![
            ListingRecord {
                community: "other-community".to_string(),
                categories: Vec::new(),
                has_nsfw_content: false,
                review_status: "unreviewed".to_string(),
            },
            ListingRecord {
                community: "rejected-community".to_string(),
                categories: Vec::new(),
                has_nsfw_content: false,
                review_status: "rejected".to_string(),
            },
        ];
        assert_eq!(communities(&package), ["other-community"]);
    }
}
