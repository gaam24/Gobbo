//! Downloading one known provider file: the single transfer path every feature uses, including the deep-link queue.
//! A link's short-lived key stands in for the saved one; the meter and its view stay with the caller.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::{SourceDownloadRequest, SourceDownloaded, SourceProvider, require, required_key};
use crate::domain::source::SourceRef;
use crate::error::{AppError, DOWNLOAD_CANCELLED, io_at};
use crate::infra::tasks::{cancelled, stop_if};

/// One provider file to fetch.
pub struct Fetch<'a> {
    pub source: SourceRef,
    /// The provider's key for the game, from the game definition. `None` becomes `source.no_game_key`.
    pub game_key: Option<&'a str>,
    /// The game's downloads folder; it is created when missing.
    pub directory: &'a Path,
    /// The archive name the caller expects, when the provider has to pick a file by name.
    pub archive_name: Option<&'a str>,
    /// The short-lived key and expiry from a deep link. When present it authorizes the download, so no saved key is
    /// needed.
    pub download_key: Option<(&'a str, u64)>,
}

/// The key precondition: a deep link carries its own short-lived key, so only a request without one needs the
/// provider's saved key. The provider itself reads the saved key; this only turns a missing one into `source.key_missing`.
fn check_key(provider: &dyn SourceProvider, link_key: bool) -> Result<(), AppError> {
    if !link_key {
        required_key(provider)?;
    }
    Ok(())
}

/// Resolves the provider and checks the stop flag and the key precondition.
fn prepare(
    request: &Fetch<'_>,
    cancel: Option<&AtomicBool>,
) -> Result<&'static dyn SourceProvider, AppError> {
    stop_if(cancel, DOWNLOAD_CANCELLED)?;
    let provider = require(request.source.provider.as_str())?;
    check_key(provider, request.download_key.is_some())?;
    Ok(provider)
}

/// Runs the one transfer behind every download. `on_bytes` hears raw `(received, total)` updates; a stop flag cuts the
/// transfer short and deletes a file that finished just as it was stopped.
fn transfer(
    provider: &dyn SourceProvider,
    request: Fetch<'_>,
    cancel: Option<&AtomicBool>,
    on_bytes: &mut dyn FnMut(u64, u64),
) -> Result<SourceDownloaded, AppError> {
    let game_key = request
        .game_key
        .ok_or_else(|| super::no_game_key(request.source.provider.as_str()))?;
    std::fs::create_dir_all(request.directory).map_err(io_at(request.directory))?;
    let downloaded = provider.download(
        SourceDownloadRequest {
            source: request.source,
            game_key,
            directory: request.directory,
            archive_name: request.archive_name,
            download_key: request.download_key,
        },
        &mut |received, total| {
            if cancelled(cancel) {
                return false;
            }
            on_bytes(received, total);
            true
        },
    );
    if cancelled(cancel) {
        if let Ok(downloaded) = &downloaded {
            let _ = crate::infra::sidecar::delete_download(&downloaded.path);
        }
        return Err(AppError::user(DOWNLOAD_CANCELLED));
    }
    downloaded
}

/// Downloads the file into the downloads folder. `on_percent` hears the progress from 0 to 100, once for each changed
/// percentage; the flag stops the transfer, and a file that finished after the stop is deleted again. A provider that
/// keeps a key needs a saved one, unless the request carries a key from a deep link.
pub fn fetch(
    request: Fetch<'_>,
    cancel: Option<&AtomicBool>,
    on_percent: &mut dyn FnMut(u32),
) -> Result<SourceDownloaded, AppError> {
    let provider = prepare(&request, cancel)?;
    let mut last = None;
    transfer(provider, request, cancel, &mut |received, total| {
        let percent = crate::domain::progress::percent(received, total).unwrap_or(0);
        if last != Some(percent) {
            on_percent(percent);
            last = Some(percent);
        }
    })
}

/// Downloads the file with raw byte progress for a caller that keeps its own meter. `on_bytes` hears every
/// `(received, total)` update; the flag stops the transfer, and a file that finished after the stop is deleted again.
/// A key from a deep link needs no saved provider key.
pub fn fetch_with_progress(
    request: Fetch<'_>,
    cancel: Option<&AtomicBool>,
    on_bytes: &mut dyn FnMut(u64, u64),
) -> Result<SourceDownloaded, AppError> {
    let provider = prepare(&request, cancel)?;
    transfer(provider, request, cancel, on_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::source::{ProviderId, SourceAuthKind, SourceCapabilities};
    use std::cell::RefCell;
    use std::sync::atomic::Ordering;

    /// A provider that writes a small archive and reports the given chunks. It keeps no network and no real key.
    struct FakeProvider {
        credential_account: Option<&'static str>,
        chunks: &'static [(u64, u64)],
        seen_key: std::sync::Mutex<Option<(String, u64)>>,
    }

    impl SourceProvider for FakeProvider {
        fn id(&self) -> ProviderId {
            ProviderId::new("fake")
        }

        fn capabilities(&self) -> SourceCapabilities {
            SourceCapabilities::new(SourceAuthKind::None)
        }

        fn credential_account(&self) -> Option<&'static str> {
            self.credential_account
        }

        fn download(
            &self,
            request: SourceDownloadRequest<'_>,
            progress: &mut dyn FnMut(u64, u64) -> bool,
        ) -> Result<SourceDownloaded, AppError> {
            *self.seen_key.lock().unwrap() = request
                .download_key
                .map(|(key, expires)| (key.to_string(), expires));
            let path = request.directory.join("fake.zip");
            std::fs::write(&path, "archive").map_err(io_at(&path))?;
            crate::infra::sidecar::write_source(&path, request.source.clone());
            for &(received, total) in self.chunks {
                if !progress(received, total) {
                    break;
                }
            }
            Ok(SourceDownloaded {
                path,
                source: request.source,
            })
        }
    }

    fn dir(name: &str) -> std::path::PathBuf {
        crate::test_support::scratch_in(module_path!(), name)
    }

    #[test]
    fn a_link_key_replaces_the_saved_key_precondition() {
        let provider = FakeProvider {
            credential_account: Some("gobbo-test-missing-key"),
            chunks: &[],
            seen_key: std::sync::Mutex::new(None),
        };
        // An account with no saved key is turned away when the request has no link key.
        assert!(check_key(&provider, false).is_err());
        // The short-lived key from a link authorizes the same provider on its own.
        assert!(check_key(&provider, true).is_ok());
    }

    #[test]
    fn a_link_key_skips_the_saved_key_precondition() {
        // Without a game key the fetch stops at `source.no_game_key`. A request with a link key reaches that check
        // even when no saved key is available; a request without one would be turned away at the key check first.
        let source = SourceRef::new("nexus", "1").with_file("2");
        let directory = dir("link-key");
        let Err(error) = fetch(
            Fetch {
                source,
                game_key: None,
                directory: &directory,
                archive_name: None,
                download_key: Some(("short-lived", 42)),
            },
            None,
            &mut |_| {},
        ) else {
            panic!("a request without a game key must not download");
        };
        assert_eq!(error.code, "source.no_game_key");
    }

    #[test]
    fn the_link_key_reaches_the_provider_and_raw_progress_reaches_the_hook() {
        let directory = dir("raw-progress");
        let provider = FakeProvider {
            credential_account: None,
            chunks: &[(5, 10), (10, 10)],
            seen_key: std::sync::Mutex::new(None),
        };
        let source = SourceRef::new("fake", "1").with_file("2");
        let seen = RefCell::new(Vec::new());
        let downloaded = transfer(
            &provider,
            Fetch {
                source: source.clone(),
                game_key: Some("game"),
                directory: &directory,
                archive_name: None,
                download_key: Some(("short-lived", 42)),
            },
            None,
            &mut |received, total| seen.borrow_mut().push((received, total)),
        )
        .unwrap();
        assert_eq!(downloaded.source, source);
        assert!(downloaded.path.is_file());
        assert_eq!(*seen.borrow(), vec![(5, 10), (10, 10)]);
        assert_eq!(
            *provider.seen_key.lock().unwrap(),
            Some(("short-lived".to_string(), 42))
        );
    }

    #[test]
    fn a_file_finishing_after_the_stop_is_deleted() {
        let directory = dir("stop-sweep");
        let provider = FakeProvider {
            credential_account: None,
            chunks: &[(5, 10), (10, 10)],
            seen_key: std::sync::Mutex::new(None),
        };
        let cancel = AtomicBool::new(false);
        let source = SourceRef::new("fake", "1").with_file("2");
        let Err(error) = transfer(
            &provider,
            Fetch {
                source,
                game_key: Some("game"),
                directory: &directory,
                archive_name: None,
                download_key: Some(("short-lived", 42)),
            },
            Some(&cancel),
            &mut |received, _| {
                if received == 10 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        ) else {
            panic!("a stopped transfer must not succeed");
        };
        assert_eq!(error.code, "download.cancelled");
        assert!(!directory.join("fake.zip").exists());
        assert!(
            !directory.join("fake.zip.source.json").exists(),
            "the sweep deletes the sidecar with the archive"
        );
    }
}
