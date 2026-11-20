//! Pack links: `gobbo://pack?url=<https>&sha256=<hex>` or an https URL with a `sha256` query. The link names one pack
//! file and its hash; the download refuses a size over the limit, a hash mismatch and a cancel. The pack format itself
//! does not change.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::Serialize;
use sha2::Sha256;

use crate::context::Data;
use crate::error::{AppError, io_at};
use crate::games;
use crate::infra::http;
use crate::infra::io::digest_file;
use crate::providers::download::free_download_name;

/// A pack file with its archives can be large; this is still a file a person chooses to download, not a disk image.
const MAX_PACK_LINK_BYTES: u64 = 32 * 1024 * 1024 * 1024;

/// A parsed pack link: the https address of the file, its required hash and the name to keep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackLink {
    /// The https address of the pack file.
    pub url: String,
    /// Lowercase SHA-256 the downloaded file must have.
    pub sha256: String,
    /// The name to keep, without a folder.
    pub name: String,
}

/// The pack file a link downloaded, with its local path, name and size.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PackLinkDownload {
    pub path: String,
    pub name: String,
    pub bytes: u32,
}

/// How far a pack-link download has got, in bytes and percent.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
pub struct PackLinkProgress {
    pub received: u32,
    pub total: u32,
    pub percent: u32,
}

/// Whether a link is a pack link, before its fields are checked. Used by the deep-link receiver to choose the queue.
pub fn is_pack_link(link: &str) -> bool {
    let link = link.trim();
    link.starts_with("gobbo://pack") || (link.starts_with("https://") && link.contains("sha256="))
}

/// Reads the fields of a link. A link without a 64-character SHA-256 or an https address is refused.
pub fn parse(link: &str) -> Result<PackLink, AppError> {
    let parsed = url::Url::parse(link.trim()).map_err(|_| invalid())?;
    let mut target = None;
    let mut hash = None;
    let mut name = None;

    if parsed.scheme() == "gobbo" && parsed.host_str() == Some("pack") {
        for (key, value) in parsed.query_pairs() {
            match key.as_ref() {
                "url" => target = Some(value.into_owned()),
                "sha256" => hash = Some(value.into_owned()),
                "name" => name = Some(value.into_owned()),
                _ => {}
            }
        }
    } else if parsed.scheme() == "https" {
        target = Some(link.trim().to_string());

        for (key, value) in parsed.query_pairs() {
            if key == "sha256" {
                hash = Some(value.into_owned());
            }
        }
    } else {
        return Err(invalid());
    }

    let target = target.ok_or_else(invalid)?;
    let address = url::Url::parse(&target).map_err(|_| invalid())?;

    if address.scheme() != "https" {
        return Err(AppError::user("pack.link_not_https"));
    }

    let hash = hash.ok_or_else(|| AppError::user("pack.link_hash_missing"))?;
    let hash = hash.to_ascii_lowercase();

    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::user("pack.link_hash_invalid"));
    }

    let name = name.unwrap_or_else(|| {
        address
            .path_segments()
            .and_then(|mut segments| segments.next_back())
            .unwrap_or("pack.gobbopack")
            .to_string()
    });
    let name = safe_name(&name);
    Ok(PackLink {
        url: target,
        sha256: hash,
        name,
    })
}

fn invalid() -> AppError {
    AppError::user("pack.link_invalid")
}

/// The pack-link format's own bound for the downloaded file's name. The shared rule
/// (`crate::domain::naming::FILE_NAME_LIMIT`, 150) governs library and archive names; a name that came from a link is
/// kept shorter for the pack-link format itself.
const PACK_LINK_NAME_LIMIT: usize = 120;

/// The file name without a folder or a control character, and with the pack extension. The shared naming rule keeps
/// Windows device names, trailing dots and invalid characters out of a name that came from an untrusted link.
fn safe_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("").trim();
    let base = if base.is_empty() {
        "pack".to_string()
    } else {
        crate::domain::naming::safe_file_name(base)
    };
    let base: String = base.chars().take(PACK_LINK_NAME_LIMIT).collect();

    if base.to_ascii_lowercase().ends_with(".gobbopack") {
        base
    } else {
        format!("{base}.gobbopack")
    }
}

/// Downloads and checks a pack link. A cancel or a file over the limit leaves nothing behind; a hash mismatch removes
/// the file too, so a tampered pack never reaches the importer.
pub fn download(
    root: &Data,
    game_id: &str,
    link: &str,
    progress: &mut dyn FnMut(PackLinkProgress),
    cancel: Option<&AtomicBool>,
) -> Result<PackLinkDownload, AppError> {
    games::load(game_id)?;
    let link = parse(link)?;
    let dir = root.game(game_id).downloads();
    fs::create_dir_all(&dir).map_err(io_at(&dir))?;
    // The same owner's naming rule as every other download: a safe name, numbered around a file that is already
    // there (the suffix keeps the `.gobbopack` extension). The file is transient; the hash checks what arrived.
    let path = free_download_name(&dir, &link.name, None);
    let mut too_large = false;
    let mut stopped = false;
    let result = http::download(&link.url, &path, &mut |received, total| {
        if received > MAX_PACK_LINK_BYTES {
            too_large = true;
            return false;
        }

        if crate::infra::tasks::cancelled(cancel) {
            stopped = true;
            return false;
        }

        let percent = crate::domain::progress::percent(received, total).unwrap_or(0);
        progress(PackLinkProgress {
            received: clamp(received),
            total: clamp(total),
            percent,
        });
        true
    });

    if let Err(error) = result {
        let _ = fs::remove_file(&path);

        if too_large {
            return Err(AppError::user("pack.link_too_large"));
        }

        if stopped {
            return Err(AppError::user("pack.link_cancelled"));
        }

        return Err(error);
    }

    let bytes = fs::metadata(&path).map_err(io_at(&path))?.len();

    if bytes > MAX_PACK_LINK_BYTES {
        let _ = fs::remove_file(&path);
        return Err(AppError::user("pack.link_too_large"));
    }

    if let Err(error) = verify(&path, &link.sha256) {
        let _ = fs::remove_file(&path);
        return Err(error);
    }

    Ok(PackLinkDownload {
        path: path.display().to_string(),
        name: link.name,
        bytes: clamp(bytes),
    })
}

const fn clamp(value: u64) -> u32 {
    if value > u32::MAX as u64 {
        u32::MAX
    } else {
        value as u32
    }
}

/// Checks a file against the hash a link names. The caller removes the file on a failure.
fn verify(path: &Path, expected: &str) -> Result<(), AppError> {
    let digest = digest_file::<Sha256>(path, &mut |_| {}).map_err(io_at(path))?;

    if digest.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(AppError::user("pack.link_hash")
            .with("expected", expected)
            .with("found", &digest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn a_pack_link_carries_an_https_url_and_a_hash() {
        let hash = "a".repeat(64);
        let link = parse(&format!(
            "gobbo://pack?url=https%3A%2F%2Fexample.com%2Fpack.gobbopack&sha256={hash}&name=pack.gobbopack"
        ))
        .unwrap();
        assert_eq!(link.url, "https://example.com/pack.gobbopack");
        assert_eq!(link.sha256, hash);
        assert_eq!(link.name, "pack.gobbopack");
        assert!(is_pack_link(&format!(
            "https://example.com/pack.gobbopack?sha256={hash}"
        )));
    }

    #[test]
    fn a_link_without_a_hash_or_with_http_is_refused() {
        assert_eq!(
            parse("gobbo://pack?url=https%3A%2F%2Fexample.com%2Fpack.gobbopack")
                .unwrap_err()
                .code,
            "pack.link_hash_missing"
        );
        assert_eq!(
            parse(&format!(
                "gobbo://pack?url=http%3A%2F%2Fexample.com%2Fpack.gobbopack&sha256={}",
                "a".repeat(64)
            ))
            .unwrap_err()
            .code,
            "pack.link_not_https"
        );
    }

    #[test]
    fn a_tampered_pack_is_refused_and_removed() {
        let base = scratch!("pack-link-tamper");
        let path = base.join("pack.gobbopack");
        fs::write(&path, b"not the pack").unwrap();
        let error = verify(&path, &"0".repeat(64)).unwrap_err();
        assert_eq!(error.code, "pack.link_hash");
    }

    #[test]
    fn a_name_cannot_leave_the_download_folder() {
        assert_eq!(safe_name("../evil.gobbopack"), "evil.gobbopack");
        assert_eq!(safe_name(""), "pack.gobbopack");
        assert_eq!(safe_name("pack"), "pack.gobbopack");
        assert_eq!(safe_name("CON"), "_CON.gobbopack");
        assert_eq!(safe_name("name."), "name.gobbopack");
        assert_eq!(safe_name("a/b"), "b.gobbopack");
    }
}
