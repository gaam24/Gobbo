//! The shared budget for retrying a rate-limited download.

use std::time::Duration;

use crate::error::AppError;

/// The 429 retry budget of one file download. Two extra tries are enough for a short server-side cooldown; the
/// budget is shared by every link of one download so a single operation cannot sleep for an unbounded number of
/// answers. It holds configuration, not game state.
pub struct RateRetry {
    left: u32,
}

impl Default for RateRetry {
    fn default() -> Self {
        Self { left: 2 }
    }
}

impl RateRetry {
    /// Runs `attempt`, retrying only a 429 while the budget lasts and sleeping for the server's `Retry-After` (five
    /// seconds when it gave none). Every other error, including a 429 once the budget runs out, is returned unchanged
    /// so the caller keeps the server's own answer. The caller owns the part file and the body checks; this knows
    /// only the shared rate-limit code.
    pub fn run<T>(
        &mut self,
        mut attempt: impl FnMut() -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        loop {
            match attempt() {
                Err(error) if error.code == "http.rate_limited" && self.left > 0 => {
                    // The download already normalized the server's answer, so this only reads the number it stored;
                    // a value from another producer still gets the same five-second fallback.
                    let seconds = error
                        .params
                        .get("retry_after")
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(5);
                    self.left -= 1;
                    std::thread::sleep(Duration::from_secs(seconds));
                }
                other => return other,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::http::download::{download_with_info, response, serve_answers};
    use crate::test_support::scratch;

    #[test]
    fn a_rate_limited_download_retries_and_succeeds_on_the_next_answer() {
        let dir = scratch!("http-429-retry");
        let target = dir.join("file.zip");
        let (url, server) = serve_answers(vec![
            response(
                "429 Too Many Requests",
                "Retry-After: 0\r\nContent-Length: 0\r\n",
                b"",
            ),
            response(
                "200 OK",
                "Content-Type: application/zip\r\nContent-Length: 5\r\n",
                b"hello",
            ),
        ]);
        let info = RateRetry::default()
            .run(|| download_with_info(&url, &target, &mut |_, _| true))
            .unwrap();
        server.join().unwrap();
        assert_eq!(info.bytes, 5);
        assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    }

    #[test]
    fn a_persisted_rate_limit_surfaces_the_servers_own_error() {
        let dir = scratch!("http-429-persist");
        let target = dir.join("file.zip");
        let limited = || {
            response(
                "429 Too Many Requests",
                "Retry-After: 0\r\nContent-Length: 0\r\n",
                b"",
            )
        };
        let (url, server) = serve_answers(vec![limited(), limited(), limited()]);
        let error = RateRetry::default()
            .run(|| download_with_info(&url, &target, &mut |_, _| true))
            .unwrap_err();
        server.join().unwrap();
        assert_eq!(error.code, "http.rate_limited");
        assert_eq!(
            error.params.get("retry_after").map(String::as_str),
            Some("0")
        );
        assert!(!target.exists());
    }
}
