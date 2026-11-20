//! A mod's page on Nexus: the version the update check reads and what the app shows about the mod. The summary and the
//! dates come from the GraphQL facets, because v3 does not expose them.

use super::api::{Endpoint, Envelope, Named, NexusClient, bad_response};
use super::text::{decode_entities, plain_text};
use crate::error::AppError;
use serde::Deserialize;

/// The data of a mod's page on Nexus: the version the update check needs, and what the app shows about the mod.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModPage {
    /// The global id of the mod (the game's id shifted left by 32 bits, then the mod's id). `/mods/{id}/files` asks by
    /// it; the game-scoped id in the page address does not work there.
    pub uid: Option<String>,
    /// The game's numeric id, which the GraphQL requirements query asks by.
    pub game_id: Option<u32>,
    pub version: String,
    pub name: Option<String>,
    /// Who made the mod ("Created by" on the page), or the uploader when the page credits no one.
    pub author: Option<String>,
    pub uploader: Option<String>,
    /// The name of the mod's category on the game's page.
    pub category: Option<String>,
    /// One line about the mod, from the GraphQL facets.
    pub summary: Option<String>,
    /// The page's description as plain text.
    pub description: Option<String>,
    /// The picture of the page, its large thumbnail.
    pub picture_url: Option<String>,
    pub endorsements: Option<u32>,
    /// Downloads of every file of the mod.
    pub downloads: Option<u32>,
    /// The page is marked as adult content.
    pub adult: bool,
    /// The page's status on Nexus: `published`, `hidden`, `removed` and so on.
    pub status: Option<String>,
    /// The names of the tags the author put on the page.
    pub tags: Vec<String>,
    /// Unique downloads of every file of the mod.
    pub unique_downloads: Option<u32>,
    /// Unix time of the last update and of the creation of the page, from the GraphQL facets.
    pub updated_at: Option<u32>,
    pub created_at: Option<u32>,
}

/// The mod's page from API v3: one request carries the version together with everything the app shows about the mod.
pub fn mod_details(client: &NexusClient, domain: &str, mod_id: u32) -> Result<ModPage, AppError> {
    let text = client.get_v3(
        &format!("/games/{domain}/mods/{mod_id}"),
        &[],
        Endpoint::Mod,
    )?;
    parse_mod_page(&text)
}

/// Parses the page response. Every field beyond the id and the version is optional, so a response without it still
/// works.
fn parse_mod_page(text: &str) -> Result<ModPage, AppError> {
    #[derive(Deserialize)]
    struct Page {
        id: String,
        #[serde(default)]
        game_id: Option<String>,
        version: String,
        name: Option<String>,
        author: Option<String>,
        uploader: Option<Named>,
        category: Option<Named>,
        description: Option<String>,
        stats: Option<Stats>,
        primary_image: Option<Image>,
        #[serde(default)]
        adult_content: bool,
        status: Option<String>,
        #[serde(default)]
        tags: Vec<Named>,
    }
    #[derive(Deserialize)]
    struct Stats {
        downloads: Option<u32>,
        unique_downloads: Option<u32>,
        endorsements: Option<u32>,
    }
    #[derive(Deserialize)]
    struct Image {
        large_thumbnail_url: Option<String>,
    }

    let page = serde_json::from_str::<Envelope<Page>>(text)
        .map_err(bad_response)?
        .data;
    let filled =
        |text: Option<String>| -> Option<String> { text.filter(|text| !text.trim().is_empty()) };
    let uploader = page
        .uploader
        .and_then(|user| filled(user.name))
        .map(|name| decode_entities(&name));
    let stats = page.stats;
    Ok(ModPage {
        uid: Some(page.id),
        game_id: page.game_id.as_deref().and_then(|id| id.parse().ok()),
        version: page.version,
        name: filled(page.name).map(|name| decode_entities(&name)),
        author: filled(page.author)
            .map(|author| decode_entities(&author))
            .or_else(|| uploader.clone()),
        uploader,
        category: page
            .category
            .and_then(|category| filled(category.name))
            .map(|name| decode_entities(&name)),
        summary: None,
        description: filled(page.description.map(|description| plain_text(&description))),
        // Only a secure address: the picture is fetched from it later.
        picture_url: page
            .primary_image
            .and_then(|image| image.large_thumbnail_url)
            .filter(|url| url.starts_with("https://")),
        endorsements: stats.as_ref().and_then(|stats| stats.endorsements),
        downloads: stats.as_ref().and_then(|stats| stats.downloads),
        unique_downloads: stats.as_ref().and_then(|stats| stats.unique_downloads),
        adult: page.adult_content,
        status: filled(page.status),
        tags: page
            .tags
            .into_iter()
            .filter_map(|tag| filled(tag.name))
            .map(|name| decode_entities(&name))
            .collect(),
        updated_at: None,
        created_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mod_page_carries_version_author_and_category() {
        let page = parse_mod_page(
            r#"{"data":{"id":"14315126033362","game_id":"3333","version":"2.5.0","name":"Remove Mods Menu",
            "author":"","uploader":{"id":"70443253","name":"Q6xF6llIa"},
            "category":{"id":"10","name":"Visuals and Graphics"}}}"#,
        )
        .unwrap();
        assert_eq!(page.uid.as_deref(), Some("14315126033362"));
        assert_eq!(page.game_id, Some(3333));
        assert_eq!(page.version, "2.5.0");
        assert_eq!(page.name.as_deref(), Some("Remove Mods Menu"));
        assert_eq!(page.author.as_deref(), Some("Q6xF6llIa"));
        assert_eq!(page.category.as_deref(), Some("Visuals and Graphics"));
    }

    #[test]
    fn a_mod_page_without_the_extra_fields_still_has_its_version() {
        let page = parse_mod_page(r#"{"data":{"id":"1","version":"1.0"}}"#).unwrap();
        assert_eq!(page.version, "1.0");
        assert_eq!(page.game_id, None);
        assert_eq!(page.name, None);
        assert_eq!(page.author, None);
        assert_eq!(page.category, None);
    }

    #[test]
    fn an_empty_uploader_falls_back_to_the_author_field() {
        let page = parse_mod_page(
            r#"{"data":{"id":"1","version":"1","author":"TestAuthor","uploader":{"name":""}}}"#,
        )
        .unwrap();
        assert_eq!(page.author.as_deref(), Some("TestAuthor"));
        assert_eq!(page.uploader, None);
    }

    #[test]
    fn a_full_page_gives_everything_the_panel_shows() {
        let page = parse_mod_page(
            r#"{"data":{"id":"1234567890","game_id":"1111","version":"1.38.1","name":"Example Mod",
            "author":"Example &amp; Team","uploader":{"id":"5","name":"example-uploader"},
            "description":"[size=4][b]Features[/b][/size]\n[list][*]Console[*]Lua[/list]\n\nSee [url=https://wiki]the wiki[/url].",
            "category":{"id":"5","name":"Scripting"},
            "primary_image":{"url":"https://staticdelivery.nexusmods.com/mods/1111/images/107/107-1.png",
            "large_thumbnail_url":"https://staticdelivery.nexusmods.com/mods/1111/images/thumbnails/107/107-1-large.png"},
            "stats":{"downloads":4000000,"unique_downloads":1,"views":2,"endorsements":12345},
            "status":"published","tags":[{"id":"7","name":"Scripting &amp; Tools"}],
            "adult_content":false}}"#,
        )
        .unwrap();
        assert_eq!(page.game_id, Some(1111));
        assert_eq!(page.name.as_deref(), Some("Example Mod"));
        assert_eq!(page.author.as_deref(), Some("Example & Team"));
        assert_eq!(page.uploader.as_deref(), Some("example-uploader"));
        assert_eq!(page.category.as_deref(), Some("Scripting"));
        assert_eq!(
            page.description.as_deref(),
            Some("Features\n\n• Console\n• Lua\n\nSee the wiki.")
        );
        assert!(
            page.picture_url
                .as_deref()
                .is_some_and(|url| url.ends_with("107-1-large.png"))
        );
        assert_eq!(
            (page.endorsements, page.downloads, page.unique_downloads),
            (Some(12345), Some(4_000_000), Some(1))
        );
        assert_eq!(page.status.as_deref(), Some("published"));
        assert_eq!(page.tags, ["Scripting & Tools"]);
        assert!(!page.adult);
    }
}
