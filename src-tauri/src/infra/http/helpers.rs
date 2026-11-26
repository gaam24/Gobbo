//! The shared agent, tracing and address rules the request client and the download path both use.
//!
//! Each agent names its TLS provider: `SChannel` through the Windows certificate store on Windows, the bundled rustls
//! elsewhere. ureq defaults to rustls even when it is built with native-tls, so an agent that names no provider panics
//! on its first `https` request.

use std::borrow::Cow;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::error::{AppError, ErrorKind};

/// The User-Agent header sent with every request, naming Gobbo and its version.
pub const USER_AGENT: &str = concat!(
    "Gobbo/",
    env!("CARGO_PKG_VERSION"),
    " (dev.gobbo.modsorganizer)"
);

/// Without a timeout a hung connection would block an update check forever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Cap for API requests: a check makes only a few attempts, so a wait beyond 30 s no longer keeps the window
/// responsive and the answer is clamped instead.
pub(super) const API_RETRY_CAP_SECS: u64 = 30;
/// Cap for downloads: a large transfer can be held back by a CDN's rate limit for minutes, so its worker waits up to
/// 300 s while an API request would already have failed.
pub(super) const DOWNLOAD_RETRY_CAP_SECS: u64 = 300;

/// One `Retry-After` answer in seconds, normalized the same way on every request path: a missing or malformed value
/// uses `default`, and the result cannot exceed `cap`. The cap belongs to the caller: the API client passes
/// [`API_RETRY_CAP_SECS`] and a download passes [`DOWNLOAD_RETRY_CAP_SECS`], so neither path can change the other's
/// wait by editing the shared rule. The client passes no default so it can fall back to its own backoff; the download
/// passes its five-second fallback and the retry budget reads the number it stored.
pub(super) fn retry_after_seconds(
    text: Option<&str>,
    default: Option<u64>,
    cap: u64,
) -> Option<u64> {
    text.and_then(|text| text.trim().parse::<u64>().ok())
        .or(default)
        .map(|seconds| seconds.min(cap))
}

/// The Windows TLS provider, named for the reason at the top of this module.
#[cfg(windows)]
fn tls_config() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build()
}

/// The rustls provider off Windows, named for the reason at the top of this module.
#[cfg(not(windows))]
fn tls_config() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .build()
}

/// One agent for every request, so an update check reuses connections and the TLS session
/// instead of paying for a new handshake for each mod.
pub fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            // The API client reads the status, the problem body and `Retry-After` itself; a 4xx is an answer, not a
            // transport error.
            .http_status_as_error(false)
            .timeout_global(Some(REQUEST_TIMEOUT))
            .tls_config(tls_config())
            .build()
            .into()
    })
}

/// Whether `GOBBO_HTTP_TRACE` is on. The setting is read once: a provider makes thousands of requests in one check and
/// the environment does not change while the app runs.
pub fn trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("GOBBO_HTTP_TRACE").is_ok_and(|value| !value.is_empty() && value != "off")
    })
}

/// One request as `GOBBO_HTTP_TRACE` prints it. `status` is `None` when no answer came; `error` says why.
pub struct Trace<'a> {
    /// The service that made the request, for example `nexus`.
    pub service: &'a str,
    /// The HTTP method.
    pub method: &'a str,
    /// The address, redacted before it is printed.
    pub url: &'a str,
    /// The status, when the server answered.
    pub status: Option<u16>,
    /// The body size, when the whole body was read.
    pub bytes: Option<u64>,
    /// How long the request took, including a body read.
    pub elapsed: Duration,
    /// Why the request failed before an answer.
    pub error: Option<String>,
}

/// Prints one line for a request when `GOBBO_HTTP_TRACE` is on. The URL and the error are redacted: a key or a signed
/// download link must never reach the log. A quiet run pays one atomic load and no allocation.
pub fn trace(event: Trace<'_>) {
    if !trace_enabled() {
        return;
    }

    let outcome = trace_outcome(event.status, event.error.as_deref());
    let size = event
        .bytes
        .map_or_else(String::new, |bytes| format!(" {bytes} B"));
    crate::infra::log::info(format!(
        "http: {} {} {} {}{} in {} ms",
        event.method,
        event.service,
        outcome,
        redact(event.url),
        size,
        event.elapsed.as_millis()
    ));
}

/// The outcome as it is printed. `ureq::Error::BadUri` can carry a whole signed address, so a failure message gets the
/// same redaction as the URL before either reaches the log.
fn trace_outcome(status: Option<u16>, error: Option<&str>) -> String {
    let outcome = match (status, error) {
        (Some(status), _) => status.to_string(),
        (None, Some(error)) => format!("failed: {error}"),
        (None, None) => "failed".to_string(),
    };
    redact(&outcome)
}

/// Reports a request that produced an answer.
pub fn trace_response(
    service: &str,
    method: &str,
    url: &str,
    status: u16,
    bytes: Option<u64>,
    started: Instant,
) {
    trace(Trace {
        service,
        method,
        url,
        status: Some(status),
        bytes,
        elapsed: started.elapsed(),
        error: None,
    });
}

/// Reports a request that failed before an answer.
pub fn trace_failure(
    service: &str,
    method: &str,
    url: &str,
    started: Instant,
    error: &ureq::Error,
) {
    trace(Trace {
        service,
        method,
        url,
        status: None,
        bytes: None,
        elapsed: started.elapsed(),
        error: Some(error.to_string()),
    });
}

/// The query names whose values carry a secret: an API key, a signed link's expiry or signature, and a download token.
/// The match ignores case, so `X-Amz-Signature=` is masked like `x-amz-signature=`. The API-key spellings are listed
/// explicitly: a name is only a match at a query boundary, so `key=` no longer reaches inside `apikey=` or `api_key=`.
const SECRET_PARAMS: [&str; 10] = [
    "apikey=",
    "api_key=",
    "key=",
    "expires=",
    "token=",
    "sig=",
    "signature=",
    "x-amz-signature=",
    "se=",
    "sp=",
];

/// Replaces the values of the query names that carry a secret in a URL or a diagnostic text. A name matches only at a
/// query boundary — the start of the text, `?`, `&` or `;` — so an ordinary name such as `case=` or `license=` keeps its
/// value. The boundary stays in the output and the value ends at the first `&`, space, newline or quote, as before.
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();

    for name in SECRET_PARAMS {
        let mut search = 0;

        loop {
            // Names are ASCII, so folding to lowercase keeps the byte positions used to edit `out`.
            let folded = out.to_ascii_lowercase();
            let Some(found) = folded[search..].find(name) else {
                break;
            };
            let at = search + found;
            if at != 0 && !matches!(folded.as_bytes()[at - 1], b'?' | b'&' | b';') {
                search = at + name.len();
                continue;
            }
            let value_start = at + name.len();
            let end = out[value_start..]
                .find(['&', ' ', '\n', '\r', '"', '\''])
                .map_or(out.len(), |end| value_start + end);
            out.replace_range(value_start..end, "***");
            search = value_start + 3;
        }
    }

    out
}

/// The agent for a file download: a large file takes longer than any request, so only connecting and each wait for
/// data have a limit.
pub(super) fn download_agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(REQUEST_TIMEOUT))
            .timeout_recv_response(Some(REQUEST_TIMEOUT))
            .timeout_per_read(Some(Duration::from_secs(60)))
            .tls_config(tls_config())
            .build()
            .into()
    })
}

/// The address as `ureq` accepts it. A service can leave a space or another character that a URI does not allow
/// unescaped, as older Nexus CDN links do with a file name; `ureq` refuses the whole request, while `url` escapes it
/// the way a browser does. An address that already parses is left untouched, so a signed link keeps its exact form.
pub(super) fn request_url(url: &str) -> Result<Cow<'_, str>, AppError> {
    if url.parse::<ureq::http::Uri>().is_ok() {
        return Ok(Cow::Borrowed(url));
    }

    url::Url::parse(url)
        .map(|parsed| Cow::Owned(parsed.to_string()))
        .map_err(|_| AppError::new("http.bad_url").kind(ErrorKind::Permanent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_with_a_space_is_escaped_before_the_request() {
        // A CDN can keep the archive's name in the address and leave its spaces unescaped.
        let raw = "https://cdn.example.invalid/cdn/3333/13429/Example Mod - Extra Text-1.0.zip?md5=x&expires=1";
        let url = request_url(raw).unwrap();
        assert_eq!(
            &*url,
            "https://cdn.example.invalid/cdn/3333/13429/Example%20Mod%20-%20Extra%20Text-1.0.zip?md5=x&expires=1"
        );
        assert!(url.parse::<ureq::http::Uri>().is_ok());
    }

    #[test]
    fn an_escaped_link_is_left_as_it_is() {
        let raw = "https://cdn.example.invalid/a/example%20mod-1.0.zip?md5=a-b_c&expires=1";
        assert_eq!(&*request_url(raw).unwrap(), raw);
    }

    #[test]
    fn an_address_that_cannot_be_a_url_is_a_permanent_error() {
        let error = request_url("not a url").unwrap_err();
        assert_eq!(error.code, "http.bad_url");
        assert_eq!(error.kind, ErrorKind::Permanent);
    }

    #[test]
    fn a_trace_never_keeps_a_key_or_an_expiry() {
        let url = "https://api.example.com/v3/mods/1?key=secret&expires=1735689600&page=2";
        let text = redact(url);
        assert_eq!(
            text,
            "https://api.example.com/v3/mods/1?key=***&expires=***&page=2"
        );
        let error = "GET https://cdn.example.com/a.zip?token=abc&user_id=7 failed";
        assert_eq!(
            redact(error),
            "GET https://cdn.example.com/a.zip?token=***&user_id=7 failed"
        );
        assert_eq!(redact("no secrets here"), "no secrets here");
    }

    #[test]
    fn signed_link_parameters_are_masked_and_an_ordinary_page_is_untouched() {
        let url = "https://cdn.example.com/a.zip?sig=abc123&X-Amz-Signature=deadbeef&page=2";
        assert_eq!(
            redact(url),
            "https://cdn.example.com/a.zip?sig=***&X-Amz-Signature=***&page=2"
        );
        // The same signature name in lowercase, and the other signed-link names, are masked too.
        assert_eq!(
            redact("?x-amz-signature=abc&signature=def&se=1&sp=r"),
            "?x-amz-signature=***&signature=***&se=***&sp=***"
        );
        assert_eq!(redact("?page=2&sort=name"), "?page=2&sort=name");
    }

    #[test]
    fn a_secret_name_needs_a_query_boundary() {
        let ordinary = "?case=1&license=MIT&base=foo";
        assert_eq!(redact(ordinary), ordinary);

        assert_eq!(
            redact("?key=one&se=two&X-Amz-Signature=three&apikey=four"),
            "?key=***&se=***&X-Amz-Signature=***&apikey=***"
        );
        // A semicolon is a boundary too, and the underscored API-key spelling needs its own name.
        assert_eq!(redact("?page=1;api_key=secret"), "?page=1;api_key=***");
    }

    #[test]
    fn a_failed_trace_masks_a_signed_url_in_the_error() {
        let error = Some("bad uri: https://cdn.example.com/a.zip?key=secret&expires=1");
        assert_eq!(
            trace_outcome(None, error),
            "failed: bad uri: https://cdn.example.com/a.zip?key=***&expires=***"
        );
        assert_eq!(trace_outcome(Some(200), None), "200");
        assert_eq!(trace_outcome(None, None), "failed");
    }

    /// ureq would otherwise keep its rustls default, which this Windows build does not enable, and panic on the first
    /// `https` request.
    #[test]
    #[cfg(windows)]
    fn every_agent_uses_native_tls_on_windows() {
        for agent in [agent(), download_agent()] {
            assert_eq!(
                agent.config().tls_config().provider(),
                ureq::tls::TlsProvider::NativeTls
            );
        }
    }

    #[test]
    #[cfg(not(windows))]
    fn every_agent_uses_rustls_off_windows() {
        for agent in [agent(), download_agent()] {
            assert_eq!(
                agent.config().tls_config().provider(),
                ureq::tls::TlsProvider::Rustls
            );
        }
    }
}
