//! The shape every provider deep link shares: the trimmed text parsed as a URL, the scheme and the absence of a port
//! checked, and the host and path split into parts. A provider adds its own shape on top of it. Also the slug check
//! that names a game domain or a community. Pure text rules; no request is made here.

use url::Url;

/// A deep link with the provider's scheme and no port. The provider reads its host, path and query from here.
pub struct LinkParts {
    /// The parsed link.
    pub url: Url,
}

impl LinkParts {
    /// The host as the parser keeps it. Empty when the link has no host.
    pub fn host(&self) -> &str {
        self.url.host_str().unwrap_or_default()
    }

    /// The path segments, without the leading and trailing `/`.
    pub fn segments(&self) -> Vec<&str> {
        self.url
            .path()
            .trim_end_matches('/')
            .trim_start_matches('/')
            .split('/')
            .collect()
    }
}

/// Parses `url` as a link of `scheme`. `None` when the text is not a URL, the scheme is another one, or the link names
/// a port: every provider refuses those the same way.
pub fn split_link(url: &str, scheme: &str) -> Option<LinkParts> {
    let url = Url::parse(url.trim()).ok()?;
    (url.scheme() == scheme && url.port().is_none()).then_some(LinkParts { url })
}

/// A game domain or a community: ASCII letters and digits and `-`, with `_` only when `allow_underscore` is set. At
/// least one character.
pub fn is_slug(text: &str, allow_underscore: bool) -> bool {
    !text.is_empty()
        && text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'-' || (allow_underscore && byte == b'_')
        })
}
