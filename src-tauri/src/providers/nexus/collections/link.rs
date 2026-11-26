use url::Url;

use crate::domain::source::ProviderId;
use crate::error::AppError;
use crate::providers::CollectionLink;
use crate::providers::link::is_slug;
use crate::providers::nexus::ID;
use crate::providers::nexus::link::split_nxm;

/// Reads a website URL or an `nxm://` collection link.
pub fn parse_collection_link(url: &str) -> Result<CollectionLink, AppError> {
    let bad = || AppError::user("collection.bad_link").with("url", url);

    if let Some(parts) = split_nxm(url) {
        let domain = parts.host();
        let segments = parts.segments();
        let (slug, revision) = match segments.as_slice() {
            ["collections", slug] => (*slug, None),
            ["collections", slug, "revisions", revision] => (*slug, parse_revision(revision)?),
            _ => return Err(bad()),
        };

        if !is_slug(slug, true) {
            return Err(bad());
        }

        return Ok(link(domain, slug, slug, revision));
    }

    let url = Url::parse(url.trim()).map_err(|_| bad())?;

    if url.scheme() != "https" {
        return Err(bad());
    }

    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();

    if !matches!(
        host.as_str(),
        "www.nexusmods.com" | "nexusmods.com" | "next.nexusmods.com"
    ) {
        return Err(bad());
    }

    let segments: Vec<&str> = url
        .path()
        .trim_end_matches('/')
        .trim_start_matches('/')
        .split('/')
        .collect();
    let (domain, slug, rest) = match segments.as_slice() {
        ["games", domain, "collections", slug, rest @ ..]
        | [domain, "collections", slug, rest @ ..] => (*domain, *slug, rest),
        _ => return Err(bad()),
    };

    if !is_slug(domain, false) || !is_slug(slug, true) {
        return Err(bad());
    }

    let revision = match rest {
        [] | ["mods" | "comments" | "changelog" | "bugs"] => query_revision(&url),
        ["revisions", revision] => parse_revision(revision)?,
        _ => return Err(bad()),
    };
    let domain = domain.to_ascii_lowercase();
    let slug = slug.to_ascii_lowercase();
    Ok(link(&domain, &slug, &slug, revision))
}

/// The collection link both forms name. The game domain is lowercased for the key and the page URL, and `slug` is
/// lowercased for the slug field. `page_slug` goes into the URL verbatim: the nxm form passes its original slug, while
/// the website form passes the slug it has already lowercased.
fn link(domain: &str, slug: &str, page_slug: &str, revision: Option<u32>) -> CollectionLink {
    let domain = domain.to_ascii_lowercase();

    CollectionLink {
        provider: ProviderId::new(ID),
        page_url: Some(format!(
            "https://www.nexusmods.com/games/{domain}/collections/{page_slug}"
        )),
        game_key: domain,
        slug: slug.to_ascii_lowercase(),
        revision,
    }
}

fn query_revision(url: &Url) -> Option<u32> {
    url.query_pairs()
        .find(|(name, _)| name == "revision")
        .and_then(|(_, value)| value.parse().ok())
}

fn parse_revision(text: &str) -> Result<Option<u32>, AppError> {
    if text.eq_ignore_ascii_case("latest") {
        return Ok(None);
    }

    let revision: u32 = text
        .parse()
        .map_err(|_| AppError::user("collection.bad_link"))?;
    Ok((revision > 0).then_some(revision))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_collection_website_url_names_the_game_and_slug() {
        let link = parse_collection_link(
            "https://www.nexusmods.com/games/cyberpunk2077/collections/n0nymh",
        )
        .unwrap();
        assert_eq!(link.game_key, "cyberpunk2077");
        assert_eq!(link.slug, "n0nymh");
        assert_eq!(link.revision, None);
        assert_eq!(
            link.page_url.as_deref(),
            Some("https://www.nexusmods.com/games/cyberpunk2077/collections/n0nymh")
        );
    }

    #[test]
    fn the_short_website_url_and_a_revision_are_read() {
        let link = parse_collection_link(
            "https://www.nexusmods.com/cyberpunk2077/collections/n0nymh/revisions/49",
        )
        .unwrap();
        assert_eq!(link.revision, Some(49));
        let query = parse_collection_link(
            "https://next.nexusmods.com/cyberpunk2077/collections/n0nymh?revision=48",
        )
        .unwrap();
        assert_eq!(query.revision, Some(48));
    }

    #[test]
    fn an_nxm_collection_link_is_read() {
        let link = parse_collection_link("nxm://cyberpunk2077/collections/n0nymh/revisions/latest")
            .unwrap();
        assert_eq!(link.game_key, "cyberpunk2077");
        assert_eq!(link.slug, "n0nymh");
        assert_eq!(link.revision, None);
    }

    #[test]
    fn an_nxm_page_url_keeps_the_slug_spelling() {
        let link = parse_collection_link("nxm://Cyberpunk2077/collections/N0nYmH").unwrap();
        assert_eq!(link.game_key, "cyberpunk2077");
        assert_eq!(link.slug, "n0nymh");
        assert_eq!(
            link.page_url.as_deref(),
            Some("https://www.nexusmods.com/games/cyberpunk2077/collections/N0nYmH")
        );
    }

    #[test]
    fn another_host_or_a_mod_link_is_refused() {
        assert!(
            parse_collection_link("https://example.com/cyberpunk2077/collections/n0nymh").is_err()
        );
        assert!(parse_collection_link("https://www.nexusmods.com/cyberpunk2077/mods/107").is_err());
        assert!(parse_collection_link("nxm://cyberpunk2077/mods/107/files/1").is_err());
    }
}
