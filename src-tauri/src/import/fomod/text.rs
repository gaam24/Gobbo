//! The text of the installer file: UTF-8, or UTF-16 with a byte order mark, as Windows tools write it.

use crate::error::{AppError, ErrorKind};

/// Decodes installer text as UTF-8 or UTF-16, using the byte order mark when present.
pub fn decode(bytes: &[u8]) -> Result<String, AppError> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Ok(String::from_utf8_lossy(&bytes[3..]).into_owned());
    }

    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16(&bytes[2..], true);
    }

    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16(&bytes[2..], false);
    }

    if bytes.contains(&0) {
        return decode_utf16(bytes, true);
    }

    String::from_utf8(bytes.to_vec()).map_err(|e| {
        AppError::new("fomod.bad_encoding")
            .with("detail", e)
            .kind(ErrorKind::Permanent)
    })
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, AppError> {
    let units = bytes.as_chunks::<2>().0.iter().map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    // Broken pairs are replaced with a replacement character: a partial installer is better than a refusal.
    let text: String = char::decode_utf16(units)
        .map(|decoded| decoded.unwrap_or('\u{FFFD}'))
        .collect();
    if text.trim().is_empty() {
        return Err(AppError::new("fomod.empty_xml").kind(ErrorKind::Permanent));
    }

    Ok(text)
}
