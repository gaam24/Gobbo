//! The `nxm://` links of the "Mod Manager Download" button on Nexus.

use crate::error::AppError;
use crate::providers::link::{LinkParts, is_slug, split_link};

/// The URL scheme of the provider's mod-file deep links.
pub(super) const SCHEME: &str = "nxm";

/// A link from the "Mod Manager Download" button on Nexus:
/// `nxm://<game>/mods/<mod>/files/<file>?key=<key>&expires=<time>&user_id=<id>`. The key and its expiry let an account
/// without premium download that one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NxmLink {
    pub domain: String,
    pub mod_id: u32,
    pub file_id: u32,
    pub key: Option<(String, u64)>,
}

/// Why an `nxm://` link cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NxmError {
    /// Not an `nxm://` link to a mod file.
    BadLink,
    /// One of `key` and `expires` is present without the other.
    IncompleteKey,
    /// A collection link. Gobbo does not install collections.
    UnsupportedCollection,
}

impl NxmError {
    /// The error the window shows and the frontend translates.
    pub fn into_app(self) -> AppError {
        match self {
            Self::BadLink => AppError::user("nexus.bad_link"),
            Self::IncompleteKey => AppError::user("nexus.bad_link").with(
                "detail",
                "the link has a key without an expiry, or an expiry without a key",
            ),
            Self::UnsupportedCollection => AppError::user("nexus.unsupported_link"),
        }
    }
}

/// The scheme and game-domain check shared by both `nxm://` readers: the link parsed with the `nxm` scheme and no port,
/// and a host that is a Nexus game domain. `None` for anything else, which both readers refuse as a bad link. The
/// path segments are read with `LinkParts::segments`.
pub(super) fn split_nxm(url: &str) -> Option<LinkParts> {
    let link = split_link(url, SCHEME)?;
    is_slug(link.host(), false).then_some(link)
}

/// Reads an `nxm://` link. A URL parser decodes the query, so a key with `+`, `/` or `=` arrives intact.
pub fn parse_nxm(url: &str) -> Result<NxmLink, NxmError> {
    let link = split_nxm(url).ok_or(NxmError::BadLink)?;
    let domain = link.host();

    if link.url.path().starts_with("/collections/") {
        return Err(NxmError::UnsupportedCollection);
    }

    let parts = link.segments();
    let ["mods", mod_id, "files", file_id] = parts.as_slice() else {
        return Err(NxmError::BadLink);
    };
    let mod_id = parse_number(mod_id)?;
    let file_id = parse_number(file_id)?;

    let mut key = None;
    let mut expires = None;

    for (name, value) in link.url.query_pairs() {
        match name.as_ref() {
            "key" if !value.is_empty() => key = Some(value.into_owned()),
            "expires" => expires = value.parse::<u64>().ok(),
            _ => {}
        }
    }

    let key = match (key, expires) {
        (None, None) => None,
        (Some(key), Some(expires)) => Some((key, expires)),
        _ => return Err(NxmError::IncompleteKey),
    };

    Ok(NxmLink {
        domain: domain.to_ascii_lowercase(),
        mod_id,
        file_id,
        key,
    })
}

/// A numeric path segment of the link.
fn parse_number(text: &str) -> Result<u32, NxmError> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(NxmError::BadLink);
    }

    text.parse().map_err(|_| NxmError::BadLink)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_nxm_link_names_the_game_mod_file_and_key() {
        let link = parse_nxm(
            "nxm://examplegame/mods/7076/files/43911?key=AbC-1_2&expires=1735689600&user_id=12345",
        )
        .unwrap();
        assert_eq!(link.domain, "examplegame");
        assert_eq!((link.mod_id, link.file_id), (7076, 43_911));
        assert_eq!(link.key, Some(("AbC-1_2".to_string(), 1_735_689_600)));
        let premium = parse_nxm("nxm://othergame/mods/1/files/2").unwrap();
        assert_eq!(premium.key, None);
        let trailing = parse_nxm("nxm://othergame/mods/1/files/2/").unwrap();
        assert_eq!((trailing.mod_id, trailing.file_id), (1, 2));
    }

    #[test]
    fn a_key_is_percent_decoded() {
        let link =
            parse_nxm("nxm://examplegame/mods/1/files/2?key=AbC%2B1%2F2%3D&expires=1735689600")
                .unwrap();
        assert_eq!(link.key, Some(("AbC+1/2=".to_string(), 1_735_689_600)));
    }

    #[test]
    fn the_scheme_and_the_host_are_case_insensitive() {
        let link = parse_nxm("NXM://ExampleGame/mods/123/files/456").unwrap();
        assert_eq!(link.domain, "examplegame");
        assert_eq!((link.mod_id, link.file_id), (123, 456));
    }

    #[test]
    fn a_key_without_an_expiry_is_not_reported_as_premium() {
        assert_eq!(
            parse_nxm("nxm://examplegame/mods/1/files/2?key=abc"),
            Err(NxmError::IncompleteKey)
        );
        assert_eq!(
            parse_nxm("nxm://examplegame/mods/1/files/2?expires=1700000000"),
            Err(NxmError::IncompleteKey)
        );
    }

    #[test]
    fn a_collection_link_says_what_it_is() {
        assert_eq!(
            parse_nxm("nxm://examplegame/collections/abcdef/revisions/3"),
            Err(NxmError::UnsupportedCollection)
        );
    }

    #[test]
    fn other_links_are_not_source_links() {
        assert_eq!(
            parse_nxm("https://www.nexusmods.com/examplegame/mods/7076"),
            Err(NxmError::BadLink)
        );
        assert_eq!(
            parse_nxm("nxm://examplegame/mods/abc/files/2"),
            Err(NxmError::BadLink)
        );
        assert_eq!(
            parse_nxm("nxm://wit/cher/mods/1/files/2"),
            Err(NxmError::BadLink)
        );
        assert_eq!(parse_nxm("nxm://../mods/1/files/2"), Err(NxmError::BadLink));
    }
}
