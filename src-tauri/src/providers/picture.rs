//! The page pictures of the providers: a picture is downloaded when a page is read, kept as a file of the game's
//! `images/` folder, and read back as a data URL for the panel. Each provider names its own files with a prefix
//! (`nexus-107`, `thunderstore-owner-name`), so two providers never write the same file.

use std::path::Path;

use crate::domain::naming::{is_plain_folder, is_safe_component};
use crate::error::{AppError, ErrorKind};

/// The largest picture saved: a page picture is a few hundred kilobytes, and nothing needs more.
const PICTURE_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// The kinds of page picture, by file extension, with the type the data URL names.
const PICTURE_TYPES: [(&str, &str); 5] = [
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("png", "image/png"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
];

/// Saves the picture at `url` as `<stem>.<extension>` in `images`, unless the same address is saved already. `None`
/// when it could not be saved: the panel then shows a placeholder, and the next page read tries again.
pub fn save_picture(
    images: &Path,
    stem: &str,
    url: &str,
    saved_url: Option<&str>,
) -> Option<String> {
    if !url.starts_with("https://") {
        return None;
    }
    let name = format!("{stem}.{}", extension_of(url));
    if !is_picture_name(&name) {
        return None;
    }
    let target = images.join(&name);
    if saved_url == Some(url) && target.is_file() {
        return Some(name);
    }
    std::fs::create_dir_all(images).ok()?;
    let part = crate::infra::http::temporary_download(images);
    let Some(info) = download_info(url, &part) else {
        let _ = std::fs::remove_file(&part);
        return None;
    };
    if crate::infra::http::body_problem(&info, &part).is_some() {
        let _ = std::fs::remove_file(&part);
        return None;
    }
    if crate::infra::store::replace_file_clean(&part, &target).is_err() {
        return None;
    }
    Some(name)
}

/// The stem of a provider's picture file: `<provider>-<mod id>`, with the id in lower case, so a name is the same file on
/// Windows and Linux. A Nexus id is a number (`nexus-107`); a Thunderstore id is `owner-name`.
pub fn stem(provider: &str, mod_id: &str) -> String {
    format!("{provider}-{}", mod_id.to_ascii_lowercase())
}

/// Downloads `url` into `part`, with the picture size limit.
fn download_info(url: &str, part: &Path) -> Option<crate::infra::http::DownloadInfo> {
    crate::infra::http::RateRetry::default()
        .run(|| {
            crate::infra::http::download_with_info(url, part, &mut |received, _| {
                received <= PICTURE_MAX_BYTES
            })
        })
        .ok()
}

/// A saved picture as a data URL, for the panel: the window takes no pictures from the network. `None` when the file
/// is gone.
pub fn read_picture(images: &Path, file: &str) -> Result<Option<String>, AppError> {
    use base64::Engine;
    let kind = file
        .rsplit_once('.')
        .and_then(|(_, extension)| PICTURE_TYPES.iter().find(|(known, _)| *known == extension));
    let (Some((_, mime)), true) = (kind, is_picture_name(file)) else {
        return Err(AppError::new("source.bad_picture").kind(ErrorKind::Permanent));
    };
    Ok(std::fs::read(images.join(file)).ok().map(|bytes| {
        format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }))
}

/// The extension a picture address names, when it is a known kind; `jpg` otherwise.
fn extension_of(url: &str) -> String {
    url.rsplit('/')
        .next()
        .and_then(|name| name.split('?').next())
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .filter(|extension| PICTURE_TYPES.iter().any(|(known, _)| known == extension))
        .unwrap_or_else(|| "jpg".to_string())
}

/// A file name that stays in the images folder on both platforms.
fn is_picture_name(name: &str) -> bool {
    is_plain_folder(name) && is_safe_component(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    #[test]
    fn the_extension_of_an_address_is_a_known_kind_or_jpg() {
        assert_eq!(
            extension_of("https://example.invalid/a/icon.PNG?v=2"),
            "png"
        );
        assert_eq!(extension_of("https://example.invalid/a/icon.bmp"), "jpg");
        assert_eq!(extension_of("https://example.invalid/a/"), "jpg");
    }

    #[test]
    fn a_stem_names_the_provider_and_the_lower_case_id() {
        assert_eq!(stem("nexus", "107"), "nexus-107");
        assert_eq!(
            stem("thunderstore", "Example_Owner-example_mod"),
            "thunderstore-example_owner-example_mod"
        );
    }

    #[test]
    fn a_saved_picture_comes_back_as_a_data_url() {
        let dir = scratch!("picture-read");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("thunderstore-a-b.png"), [0x89, b'P', b'N', b'G']).unwrap();
        let url = read_picture(&dir, "thunderstore-a-b.png").unwrap().unwrap();
        assert!(url.starts_with("data:image/png;base64,"), "{url}");
        assert_eq!(read_picture(&dir, "missing-x.png").unwrap(), None);
        for bad in ["../library.json", "nexus-107.exe", "", "a/b.png"] {
            assert_eq!(
                read_picture(&dir, bad).unwrap_err().code,
                "source.bad_picture",
                "{bad}"
            );
        }
    }

    #[test]
    fn a_saved_copy_at_the_same_address_is_kept_without_a_request() {
        let dir = scratch!("picture-keep");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("nexus-7.jpg"), b"picture").unwrap();
        // The address does not resolve: the copy is used, so no download is tried.
        let url = "https://example.invalid/nexus-7.jpg";
        assert_eq!(
            save_picture(&dir, "nexus-7", url, Some(url)).as_deref(),
            Some("nexus-7.jpg")
        );
    }

    #[test]
    fn an_address_that_is_not_https_or_a_name_outside_the_folder_saves_nothing() {
        let dir = scratch!("picture-refuse");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            save_picture(&dir, "nexus-7", "http://example.invalid/a.png", None),
            None
        );
        assert_eq!(
            save_picture(&dir, "../escape", "https://example.invalid/a.png", None),
            None
        );
    }
}
