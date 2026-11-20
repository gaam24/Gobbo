//! The stage 0 handover: the variable that names its file, the file's name and the grammar of its lines. The launcher
//! writes the original bytes of the patched loader stubs and the address of the stage 0 configuration; the hook reads
//! them back before it installs its detours. Plain logic with no Windows calls, so the format is tested everywhere.

use std::fmt::Write;

/// The variable that names the handover file, set for the launched program and read by the hook.
pub const ENV: &str = "GOBBO_OVERLAY_EARLY";

/// The name of the handover file in the session's state folder, written by the launcher.
pub const FILE_NAME: &str = "early-redirect.txt";

/// One line of the handover: the stage 0 configuration's address, or a loader stub and the bytes it had before the patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// The address of the stage 0 configuration, so the hook can switch the redirect off.
    Config(usize),
    /// The bytes one patched loader stub had before the patch.
    Stub {
        /// The address of the stub in the launched process.
        address: usize,
        /// The bytes at that address before the launcher patched it.
        original: Vec<u8>,
    },
}

impl Line {
    /// The handover text of this line: its keyword, the address, lowercase two-digit hex bytes and a trailing newline.
    pub fn to_text(&self) -> String {
        let mut text = String::new();

        match self {
            Self::Config(address) => {
                let _ = writeln!(text, "config {address}");
            }
            Self::Stub { address, original } => {
                let _ = write!(text, "stub {address} ");
                for byte in original {
                    let _ = write!(text, "{byte:02x}");
                }
                text.push('\n');
            }
        }

        text
    }
}

/// Reads one handover line, or `None` when it is not a line this grammar writes. A stub without bytes is written with a
/// space after the address; that space is the only way such a line can have no hex run.
pub fn parse_line(line: &str) -> Option<Line> {
    let mut parts = line.split_whitespace();

    match (parts.next(), parts.next(), parts.next()) {
        (Some("config"), Some(address), None) => address.parse().ok().map(Line::Config),
        (Some("stub"), Some(address), hex) if parts.next().is_none() => {
            let original = match hex {
                Some(hex) => parse_hex(hex)?,
                None if line.ends_with(' ') || line.ends_with(" \n") || line.ends_with(" \r\n") => {
                    Vec::new()
                }
                None => return None,
            };
            Some(Line::Stub {
                address: address.parse().ok()?,
                original,
            })
        }
        _ => None,
    }
}

/// The bytes of one even-length run of hex pairs, or `None` when a pair is not a byte.
fn parse_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }

    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let pair = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(pair, 16).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_written_line_parses_back() {
        let lines = [
            Line::Config(0),
            Line::Config(usize::MAX),
            Line::Stub {
                address: 0x1000,
                original: Vec::new(),
            },
            Line::Stub {
                address: 0x2000,
                original: vec![0x4c, 0x8b, 0xd1, 0xb8],
            },
        ];

        for line in lines {
            let text = line.to_text();
            assert_eq!(parse_line(&text), Some(line), "{text:?}");
        }
    }

    #[test]
    fn a_line_has_the_keyword_the_address_and_lower_case_hex() {
        assert_eq!(Line::Config(42).to_text(), "config 42\n");
        assert_eq!(
            Line::Stub {
                address: 16,
                original: vec![0x00, 0xab],
            }
            .to_text(),
            "stub 16 00ab\n"
        );
    }

    #[test]
    fn invalid_lines_are_refused() {
        for text in [
            "",
            "   ",
            "config",
            "config x",
            "config 1 2",
            "stub 1",
            "stub 1 zz",
            "stub 1 abc",
            "other 1",
            "config -1",
        ] {
            assert_eq!(parse_line(text), None, "{text:?}");
        }
    }
}
