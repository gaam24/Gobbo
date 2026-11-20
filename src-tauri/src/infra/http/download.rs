//! The file download: the response headers, the body written to disk and the rules that reject a web page.

use std::time::Instant;

use crate::error::{AppError, ErrorKind, FileError};

use super::helpers::{USER_AGENT, download_agent, request_url, trace_failure, trace_response};
use super::retry::RateRetry;

/// The suffix of a file that is still downloading. A finished download is renamed to its real name.
pub const PART_SUFFIX: &str = ".part";

/// What a finished download says about itself, beyond the bytes on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadInfo {
    pub bytes: u64,
    pub content_type: Option<String>,
    pub content_disposition: Option<String>,
}

/// A unique temporary file for a download in `dir`, next to its final name.
fn temporary_download(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join(format!(
        ".gobbo-{}{PART_SUFFIX}",
        crate::infra::store::unique_suffix()
    ))
}

/// A finished download in a temporary part file next to its final place. Dropping it removes the file, so a failed or
/// refused download leaves nothing behind; `keep_as` moves it to its real name.
pub struct Part {
    path: std::path::PathBuf,
    pub info: DownloadInfo,
}

impl Part {
    /// Moves the file to `target`, replacing a file that is there.
    pub fn keep_as(self, target: &std::path::Path) -> Result<(), AppError> {
        crate::infra::store::replace_file_clean(&self.path, target)
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        // Gone already after `keep_as`; otherwise the download failed or was refused.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Why `download_part` kept no file.
#[derive(Debug)]
pub enum PartError {
    /// The request failed or was cancelled; the server's error as it came.
    Failed(AppError),
    /// The server answered with an empty file or a web page.
    Body(BodyProblem),
}

/// Downloads `url` into a temporary part file in `dir`. `before` runs before every try, including a retry after a 429
/// within `rate`'s budget, so a caller can pace its provider there. An empty answer or a web page is refused.
pub fn download_part(
    dir: &std::path::Path,
    url: &str,
    rate: &mut RateRetry,
    before: &mut dyn FnMut(),
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<Part, PartError> {
    std::fs::create_dir_all(dir)
        .map_err(|error| PartError::Failed(crate::error::io_at(dir)(error)))?;
    let path = temporary_download(dir);
    let downloaded = rate.run(|| {
        before();
        download_with_info(url, &path, progress)
    });
    let part = Part {
        info: match downloaded {
            Ok(info) => info,
            Err(error) => {
                let _ = std::fs::remove_file(&path);
                return Err(PartError::Failed(error));
            }
        },
        path,
    };
    body_problem(&part.info, &part.path).map_or(Ok(part), |problem| Err(PartError::Body(problem)))
}

/// Whether the response is a web page rather than the archive the caller asked for.
pub fn is_html_response(info: &DownloadInfo) -> bool {
    info.content_type.as_deref().is_some_and(|content_type| {
        let content_type = content_type.trim().to_ascii_lowercase();
        content_type.starts_with("text/html") || content_type.starts_with("application/xhtml")
    })
}

/// Why a finished download is not the archive the caller asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyProblem {
    /// The server sent no bytes.
    Empty,
    /// The server sent a web page, by header or by body.
    Html,
}

/// The one check that a finished download has bytes and is not a web page. A provider maps the answer to its own error
/// code; the rule itself is shared.
pub fn body_problem(info: &DownloadInfo, path: &std::path::Path) -> Option<BodyProblem> {
    if info.bytes == 0 {
        Some(BodyProblem::Empty)
    } else if is_html_response(info) || looks_like_html(path) {
        Some(BodyProblem::Html)
    } else {
        None
    }
}

/// Whether the first bytes of a file are a web page. A CDN can send an expired-link page with `200` and a generic
/// content type, so the body is checked as well.
pub fn looks_like_html(path: &std::path::Path) -> bool {
    use std::io::Read;

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut start = [0_u8; 512];
    let Ok(read) = file.read(&mut start) else {
        return false;
    };
    let text = String::from_utf8_lossy(&start[..read])
        .trim_start()
        .to_ascii_lowercase();
    text.starts_with("<!doctype html") || text.starts_with("<html") || text.starts_with("<?xml")
}

/// Downloads `url` into the file `target`. `progress` gets the bytes received and the total (0 while unknown), and
/// stops the download by answering `false`. A failed or stopped download leaves no file behind. A 429 is retried
/// with the shared [`RateRetry`] rule before the error reaches the caller.
pub fn download(
    url: &str,
    target: &std::path::Path,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), AppError> {
    RateRetry::default()
        .run(|| download_with_info(url, target, progress))
        .map(|_| ())
}

/// Downloads `url` into `target` and answers what the server said about the file. The response headers are read
/// before the body, so a caller can name the file from `Content-Disposition` and reject an HTML error page.
pub fn download_with_info(
    url: &str,
    target: &std::path::Path,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<DownloadInfo, AppError> {
    let url = request_url(url)?;
    let agent = download_agent();
    let started = Instant::now();
    let response = agent
        .get(&*url)
        .header("User-Agent", USER_AGENT)
        // A CDN-compressed archive would not be the file the importer expects.
        .header("Accept-Encoding", "identity")
        .call();
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            trace_failure("http", "GET", &url, started, &error);
            return Err(request_error("http", error));
        }
    };
    let status = response.status().as_u16();
    let content_type = header(&response, "content-type");
    let content_disposition = header(&response, "content-disposition");

    if status == 429 {
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(5)
            .min(300);
        trace_response("http", "GET", &url, status, None, started);
        return Err(AppError::new("http.rate_limited")
            .kind(ErrorKind::Retryable)
            .with("retry_after", retry_after));
    }

    if !(200..300).contains(&status) {
        trace_response("http", "GET", &url, status, None, started);
        return Err(request_error("http", ureq::Error::StatusCode(status)));
    }

    let body = response.into_body();
    let total = body.content_length().unwrap_or(0);

    if let Err(error) = save_body(body.into_reader(), target, total, progress) {
        let _ = std::fs::remove_file(target);
        return Err(error);
    }

    let bytes = std::fs::metadata(target).map_or(total, |meta| meta.len());
    trace_response("http", "GET", &url, status, Some(bytes), started);
    Ok(DownloadInfo {
        bytes,
        content_type,
        content_disposition,
    })
}

/// One response header as text. A header that is not ASCII is not useful to the importer.
fn header(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

/// Copies a download into a file, reporting the bytes done of `total` (0 when the server did not say).
fn save_body(
    mut reader: impl std::io::Read,
    target: &std::path::Path,
    total: u64,
    progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<(), AppError> {
    let io_failed = |e: std::io::Error| crate::error::AppError::from(FileError::new(target, e));
    let mut file = std::fs::File::create(target).map_err(io_failed)?;
    let mut buffer = vec![0_u8; 256 * 1024];
    let mut done = 0_u64;

    loop {
        let read = reader.read(&mut buffer).map_err(|e| network("http", e))?;

        if read == 0 {
            break;
        }

        std::io::Write::write_all(&mut file, &buffer[..read]).map_err(io_failed)?;
        done += read as u64;

        if !progress(done, total) {
            return Err(AppError::user(crate::error::DOWNLOAD_CANCELLED));
        }
    }

    Ok(())
}

/// The error of a request that failed, for the service named by `service` (its message codes start with it, for example
/// `nexus.unavailable`). A server or rate-limit status and a broken connection may work on a later try; other statuses
/// do not.
pub fn request_error(service: &str, error: ureq::Error) -> AppError {
    match error {
        ureq::Error::StatusCode(code) => status_error(service, code),
        other => network(service, other),
    }
}

/// The error of an answer whose status is not a success, for the service named by `service`. The same codes as
/// `request_error`, for a caller that has the status but no `ureq` error.
pub fn status_error(service: &str, code: u16) -> AppError {
    match code {
        429 | 500..=599 => {
            AppError::retryable(&format!("{service}.unavailable")).with("code", code)
        }
        _ => AppError::new(&format!("{service}.status"))
            .with("code", code)
            .kind(ErrorKind::Permanent),
    }
}

/// A connection that broke while data came in, or a body that could not be read: the next try may get through.
pub fn network(service: &str, detail: impl ToString) -> AppError {
    AppError::retryable(&format!("{service}.network")).with("detail", detail)
}

/// One request against a local server: the test reads the request and writes the fixed response. No new
/// dependency and no network beyond the loopback interface.
#[cfg(test)]
pub(super) fn serve(response: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let _ = stream.read(&mut request);
        stream.write_all(&response).unwrap();
        stream.flush().unwrap();
    });
    (format!("http://{address}/file.zip"), handle)
}

#[cfg(test)]
pub(super) fn response(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!("HTTP/1.1 {status}\r\n{headers}Connection: close\r\n\r\n").into_bytes();
    out.extend_from_slice(body);
    out
}

/// A local server that answers one request per fixed response, in order. A 429 followed by a 200 needs two
/// connections, so the single-answer `serve` cannot express it.
#[cfg(test)]
pub(super) fn serve_answers(answers: Vec<Vec<u8>>) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        for answer in answers {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request);
            stream.write_all(&answer).unwrap();
            stream.flush().unwrap();
        }
    });
    (format!("http://{address}/file.zip"), handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn a_body_problem_names_an_empty_or_html_answer() {
        let dir = scratch!("body-problem");
        std::fs::create_dir_all(&dir).unwrap();
        let empty = DownloadInfo {
            bytes: 0,
            content_type: None,
            content_disposition: None,
        };
        assert_eq!(
            body_problem(&empty, &dir.join("missing")),
            Some(BodyProblem::Empty)
        );

        let page = dir.join("page");
        std::fs::write(&page, "<!doctype html><html></html>").unwrap();
        let info = DownloadInfo {
            bytes: 24,
            content_type: None,
            content_disposition: None,
        };
        assert_eq!(body_problem(&info, &page), Some(BodyProblem::Html));
        assert_eq!(body_problem(&empty, &page), Some(BodyProblem::Empty));

        let archive = dir.join("archive");
        std::fs::write(&archive, b"PK\x03\x04").unwrap();
        assert_eq!(body_problem(&info, &archive), None);
    }

    #[test]
    fn a_download_reads_the_headers_and_the_whole_body() {
        let dir = scratch!("http-download");
        let target = dir.join("file.zip");
        let body = b"hello";
        let (url, server) = serve(response(
            "200 OK",
            "Content-Type: application/zip\r\nContent-Disposition: attachment; filename=\"mod.zip\"\r\nContent-Length: 5\r\n",
            body,
        ));
        let info = download_with_info(&url, &target, &mut |_, _| true).unwrap();
        server.join().unwrap();
        assert_eq!(info.bytes, 5);
        assert_eq!(info.content_type.as_deref(), Some("application/zip"));
        assert_eq!(
            info.content_disposition.as_deref(),
            Some("attachment; filename=\"mod.zip\"")
        );
        assert_eq!(std::fs::read(&target).unwrap(), body);
    }

    #[test]
    fn a_server_error_is_a_permanent_status_error() {
        let dir = scratch!("http-status");
        let target = dir.join("file.zip");
        let (url, server) = serve(response("404 Not Found", "Content-Length: 0\r\n", b""));
        let error = download_with_info(&url, &target, &mut |_, _| true).unwrap_err();
        server.join().unwrap();
        assert_eq!(error.code, "http.status");
        assert_eq!(error.kind, ErrorKind::Permanent);
        assert!(!target.exists());
    }

    #[test]
    fn a_stopped_download_removes_the_partial_file() {
        let dir = scratch!("http-cancel");
        let target = dir.join("file.zip");
        let body = vec![7_u8; 600 * 1024];
        let (url, server) = serve(response(
            "200 OK",
            "Content-Type: application/zip\r\nContent-Length: 614400\r\n",
            &body,
        ));
        let error = download_with_info(&url, &target, &mut |_, _| false).unwrap_err();
        server.join().unwrap();
        assert_eq!(error.code, "download.cancelled");
        assert!(!target.exists());
    }

    #[test]
    fn a_download_reports_its_progress_and_writes_the_whole_file() {
        let dir = scratch!("download");
        let target = dir.join("file.zip");
        let data = vec![7_u8; 600 * 1024];
        let mut seen = Vec::new();
        save_body(
            data.as_slice(),
            &target,
            data.len() as u64,
            &mut |done, total| {
                seen.push((done, total));
                true
            },
        )
        .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), data);
        assert_eq!(seen.last(), Some(&(data.len() as u64, data.len() as u64)));
        assert!(seen.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }
}
