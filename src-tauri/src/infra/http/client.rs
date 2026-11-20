//! A request client with the retry and rate-limit policy every provider shares. The caller supplies the URL, headers
//! and query, then maps the final status to its own error codes. A transient status (429 or 5xx) and a failed
//! connection are retried with `Retry-After` or an exponential backoff; a caller that must not repeat a request uses
//! `get_once`.

use std::fmt;
use std::time::{Duration, Instant};

use super::{RateLimit, Trace, USER_AGENT, agent, trace, trace_failure, trace_response};
use std::time::UNIX_EPOCH;

/// The longest wait a `Retry-After` may ask for. A longer answer fails and the caller reports it.
const MAX_RETRY_DELAY: Duration = Duration::from_secs(30);
/// How many times a request is tried unless the caller says otherwise.
const MAX_ATTEMPTS: u32 = 3;
/// The first backoff step; each later attempt doubles it.
const BACKOFF_BASE: Duration = Duration::from_millis(250);

/// One HTTP answer with the body read.
pub struct Response {
    pub status: u16,
    pub body: String,
    /// The provider's remaining request budget, when it sent one.
    pub rate_limit: Option<RateLimit>,
}

/// The remaining budget and reset time from the standard rate-limit headers. A header that is missing or malformed
/// is not a reason to fail the request; it only means we have no budget to track for that answer.
fn rate_limit(headers: &ureq::http::HeaderMap) -> Option<RateLimit> {
    let remaining = headers
        .get("x-ratelimit-remaining")?
        .to_str()
        .ok()?
        .parse::<u64>()
        .ok()?;
    let reset = headers
        .get("x-ratelimit-reset")?
        .to_str()
        .ok()?
        .parse::<u64>()
        .ok()?;
    Some(RateLimit {
        remaining,
        reset: UNIX_EPOCH + Duration::from_secs(reset),
    })
}

/// A request that never produced an answer, or whose body did not read.
#[derive(Debug)]
pub enum ClientError {
    Transport(ureq::Error),
    Body(ureq::Error),
}

impl fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => write!(formatter, "{error}"),
            Self::Body(error) => write!(formatter, "the body did not read: {error}"),
        }
    }
}

impl std::error::Error for ClientError {}

/// A client for one provider operation: the shared headers, an optional rate limit and the retry policy.
pub struct Client {
    service: String,
    headers: Vec<(String, String)>,
    attempts: u32,
}

impl Client {
    /// A client that traces under `service` and sends the shared `User-Agent`.
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            headers: Vec::new(),
            attempts: MAX_ATTEMPTS,
        }
    }

    /// Adds a header sent with every request of this client.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// A GET that retries a transient answer.
    pub fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Response, ClientError> {
        self.send("GET", url, query, None, true)
    }

    /// A GET that returns the first answer, also a 429 or a 5xx. Used when repeating the request has a side effect.
    pub fn get_once(&self, url: &str, query: &[(&str, &str)]) -> Result<Response, ClientError> {
        self.send("GET", url, query, None, false)
    }

    /// A JSON POST that retries a transient answer.
    pub fn post_json(&self, url: &str, body: &str) -> Result<Response, ClientError> {
        self.send("POST", url, &[], Some(body), true)
    }

    fn send(
        &self,
        method: &str,
        url: &str,
        query: &[(&str, &str)],
        body: Option<&str>,
        retry: bool,
    ) -> Result<Response, ClientError> {
        let mut attempt = 0_u32;

        loop {
            let started = Instant::now();
            // Each outcome says whether a retry may help and how long the server asked to wait.
            let (outcome, retryable, wait) = match self.send_once(url, query, body) {
                Ok(response) => {
                    let status = response.status().as_u16();
                    let retry_after = retry_after(response.headers());
                    let budget = rate_limit(response.headers());

                    match response.into_body().read_to_string() {
                        Ok(text) => {
                            trace_response(
                                &self.service,
                                method,
                                url,
                                status,
                                Some(text.len() as u64),
                                started,
                            );
                            let response = Response {
                                status,
                                body: text,
                                rate_limit: budget,
                            };
                            (Ok(response), transient(status), retry_after)
                        }
                        Err(error) => {
                            trace(Trace {
                                service: &self.service,
                                method,
                                url,
                                status: Some(status),
                                bytes: None,
                                elapsed: started.elapsed(),
                                error: Some(format!("the body did not read: {error}")),
                            });
                            (Err(ClientError::Body(error)), true, retry_after)
                        }
                    }
                }
                Err(error) => {
                    trace_failure(&self.service, method, url, started, &error);
                    (Err(ClientError::Transport(error)), true, None)
                }
            };

            if retry && retryable && attempt + 1 < self.attempts {
                std::thread::sleep(backoff(attempt, wait));
                attempt += 1;
                continue;
            }

            return outcome;
        }
    }

    fn send_once(
        &self,
        url: &str,
        query: &[(&str, &str)],
        body: Option<&str>,
    ) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
        body.map_or_else(
            || {
                let mut request = agent().get(url).header("User-Agent", USER_AGENT);

                for (name, value) in &self.headers {
                    request = request.header(name, value);
                }

                if !query.is_empty() {
                    request = request.query_pairs(query.iter().copied());
                }

                request.call()
            },
            |body| {
                let mut request = agent().post(url).header("User-Agent", USER_AGENT);

                for (name, value) in &self.headers {
                    request = request.header(name, value);
                }

                if !query.is_empty() {
                    request = request.query_pairs(query.iter().copied());
                }

                request
                    .header("Content-Type", "application/json")
                    .send(body)
            },
        )
    }
}

/// The `Retry-After` header in seconds, capped so a check does not sleep for minutes.
fn retry_after(headers: &ureq::http::HeaderMap) -> Option<Duration> {
    let seconds = headers
        .get("retry-after")?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(Duration::from_secs(seconds).min(MAX_RETRY_DELAY))
}

/// Whether a status may pass on another try.
const fn transient(status: u16) -> bool {
    matches!(status, 429 | 500..=599)
}

/// How long to wait before the next attempt: the server's `Retry-After`, or an exponential backoff.
fn backoff(attempt: u32, retry_after: Option<Duration>) -> Duration {
    retry_after
        .unwrap_or_else(|| BACKOFF_BASE * (1 << attempt.min(4)))
        .min(MAX_RETRY_DELAY)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(value: &str) -> ureq::http::HeaderMap {
        let mut headers = ureq::http::HeaderMap::new();
        headers.insert("retry-after", value.parse().unwrap());
        headers
    }

    #[test]
    fn a_retry_after_is_read_in_seconds_and_capped() {
        assert_eq!(retry_after(&headers("7")), Some(Duration::from_secs(7)));
        assert_eq!(retry_after(&headers("600")), Some(MAX_RETRY_DELAY));
        assert_eq!(retry_after(&headers("soon")), None);
        assert_eq!(retry_after(&ureq::http::HeaderMap::new()), None);
    }

    #[test]
    fn a_backoff_grows_and_stops_at_the_cap() {
        assert_eq!(backoff(0, None), BACKOFF_BASE);
        assert_eq!(backoff(1, None), BACKOFF_BASE * 2);
        assert_eq!(backoff(8, None), BACKOFF_BASE * 16);
        assert_eq!(
            backoff(0, Some(Duration::from_secs(2))),
            Duration::from_secs(2)
        );
    }

    #[test]
    fn a_budget_is_read_from_response_headers() {
        let mut headers = ureq::http::HeaderMap::new();
        headers.insert("x-ratelimit-remaining", "7".parse().unwrap());
        headers.insert("x-ratelimit-reset", "90".parse().unwrap());
        let parsed = rate_limit(&headers).unwrap();
        assert_eq!(parsed.remaining, 7);
        assert_eq!(parsed.reset, UNIX_EPOCH + Duration::from_secs(90));
        assert!(rate_limit(&ureq::http::HeaderMap::new()).is_none());
    }

    #[test]
    fn only_a_transient_status_is_retried() {
        assert!(transient(429));
        assert!(transient(500));
        assert!(transient(599));
        assert!(!transient(400));
        assert!(!transient(404));
        assert!(!transient(200));
    }
}
