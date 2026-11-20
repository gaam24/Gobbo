//! The shipped icons are one 128x128 WebP each, so the interface and the installer keep one size and one format.

use super::*;

const ICON_SIZE: u32 = 128;
const MAX_BYTES: u64 = 12 * 1024;

#[test]
fn every_shipped_icon_is_a_128_webp_within_the_size_limit() {
    let dir = Path::new(DEV_GAMES_DIR);

    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();

        if !path.is_dir() {
            continue;
        }

        let png = path.join("icon.png");
        assert!(
            !png.exists(),
            "{}: icon.png is legacy; use a 128x128 icon.webp",
            png.display()
        );
        let icon = path.join("icon.webp");

        if !icon.exists() {
            continue;
        }

        let bytes = std::fs::read(&icon).unwrap();
        assert!(
            bytes.len() as u64 <= MAX_BYTES,
            "{}: {} bytes, limit {}",
            icon.display(),
            bytes.len(),
            MAX_BYTES
        );
        let size =
            webp_size(&bytes).unwrap_or_else(|| panic!("{}: not a WebP file", icon.display()));
        assert_eq!(
            size,
            (ICON_SIZE, ICON_SIZE),
            "{}: {:?}, expected {}x{}",
            icon.display(),
            size,
            ICON_SIZE,
            ICON_SIZE
        );
    }
}

/// The canvas size of a WebP file: VP8X (extended), VP8L (lossless) or VP8 (lossy).
fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 30 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }

    let chunk = &bytes[12..16];

    if chunk == b"VP8X" {
        return Some((
            1 + u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]),
            1 + u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]),
        ));
    }

    if chunk == b"VP8L" {
        let bits = u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);
        return Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1));
    }

    if chunk == b"VP8 " {
        return Some((
            u32::from(u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3fff),
            u32::from(u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3fff),
        ));
    }

    None
}
