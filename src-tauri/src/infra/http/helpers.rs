//! The shared agent, tracing and address rules the request client and the download path both use.

use std::borrow::Cow;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::error::{AppError, ErrorKind};

pub const USER_AGENT: &str = concat!(
    "Gobbo/",
    env!("CARGO_PKG_VERSION"),
    " (dev.gobbo.modsorganizer)"
);

/// Without a timeout a hung connection would block an update check forever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// The TLS provider the platform dependency enables: `SChannel` through the Windows certificate store, or the bundled
/// rustls elsewhere. ureq defaults to rustls even when it is built with native-tls, so an agent that names no provider
/// panics on its first `https` request.
#[cfg(windows)]
fn tls_config() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build()
}

/// The TLS provider the platform dependency enables: `SChannel` through the Windows certificate store, or the bundled
/// rustls elsewhere. ureq defaults to rustls even when it is built with native-tls, so an agent that names no provider
/// panics on its first `https` request.
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
    let outcome = match (event.status, &event.error) {
        (Some(status), _) => format!("{status}"),
        (None, Some(error)) => format!("failed: {error}"),
        (None, None) => "failed".to_string(),
    };
    let size = event
        .bytes
        .map_or_else(String::new, |bytes| format!(" {bytes} B"));
    eprintln!(
        "gobbo http: {} {} {} {}{} in {} ms",
        event.method,
        event.service,
        outcome,
        redact(event.url),
        size,
        event.elapsed.as_millis()
    );
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

/// Replaces the values of the query names that carry a secret in a URL or a diagnostic text. `key=` also covers
/// `apikey=` and `api_key=`.
pub fn redact(text: &str) -> String {
    let mut out = text.to_string();
    for name in ["key=", "expires=", "token="] {
        let mut search = 0;
        while let Some(found) = out[search..].find(name) {
            let at = search + found + name.len();
            let end = out[at..]
                .find(['&', ' ', '\n', '\r', '"', '\''])
                .map_or(out.len(), |end| at + end);
            out.replace_range(at..end, "***");
            search = at + 3;
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
        // An older Nexus file keeps its name in the CDN address, and Nexus leaves the spaces unescaped.
        let raw = "https://cf-files.nexusmods.com/cdn/3333/13429/Nova City - Night Vision-13429-1-0-0nv-1715479909.zip?md5=x&expires=1";
        let url = request_url(raw).unwrap();
        assert_eq!(
            &*url,
            "https://cf-files.nexusmods.com/cdn/3333/13429/Nova%20City%20-%20Night%20Vision-13429-1-0-0nv-1715479909.zip?md5=x&expires=1"
        );
        assert!(url.parse::<ureq::http::Uri>().is_ok());
    }

    #[test]
    fn an_escaped_link_is_left_as_it_is() {
        let raw = "https://cf-files.nexusmods.com/a/CET%201.38.1-107.zip?md5=a-b_c&expires=1";
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
