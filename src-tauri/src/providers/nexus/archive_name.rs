//! Names of archives downloaded from Nexus Mods, for example `Remove Mods Menu 35686 1.2.0 2026-10-23T17-21Z Q6xF6llIa.zip`.
//! Two formats: the new `<name> <mod id> <version> <timestamp> [<token>]` and the older `<name>-<id>-<version>-<unix time>`.
//! The name comes from the mod author and is not always the mod's name. The id is the number from the page address:
//! `nexusmods.com/<game>/mods/<id>`.
//!
//! The import reads the name once (`providers::archive_name_from`) and the library keeps the result on the archive, so
//! the library does not read the name again.

use crate::domain::archive::{is_number, strip_extension};

/// Data taken from the name of a Nexus archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveName {
    /// The name given by the author, for example `CET 1.38.0 - Game version 2.5`.
    pub name: String,
    pub mod_id: u32,
    pub version: String,
}

/// Recognises a Nexus archive name in both formats. Accepts the name with an archive extension (`.zip`, `.7z`, `.rar`) or without it. Other names give `None`.
pub fn parse(file_name: &str) -> Option<ArchiveName> {
    parse_spaced(file_name).or_else(|| parse_dashed(file_name))
}

/// New format: `<name> <id> <version> <timestamp> [<token>]`, for example `redscript 1511 0.5.32 2026-10-22T18-29Z aI50IAAGE.zip`.
/// The timestamp is required, because without it a plain name such as `Foo 12 1.0` would look like a Nexus one.
pub fn parse_spaced(file_name: &str) -> Option<ArchiveName> {
    let words: Vec<&str> = strip_extension(file_name).split_whitespace().collect();
    let at = words.iter().position(|word| is_timestamp(word))?;
    // Before the timestamp: version, id and at least one word of the name. After it: at most one token.
    if at < 3 || words.len() - at > 2 {
        return None;
    }
    let id = words[at - 2];
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(ArchiveName {
        name: words[..at - 2].join(" "),
        mod_id: id.parse().ok()?,
        version: words[at - 1].to_string(),
    })
}

/// The shape `2026-10-23T17-21Z`: digits, hyphens, a capital `T` and `Z`. Nexus writes the time with hyphens.
fn is_timestamp(word: &str) -> bool {
    const SHAPE: &[u8] = b"dddd-dd-ddTdd-ddZ";
    word.len() == SHAPE.len()
        && word.bytes().zip(SHAPE).all(|(b, shape)| match *shape {
            b'd' => b.is_ascii_digit(),
            _ => b == *shape,
        })
}

/// Older format: `<name>-<id>-<version>-<unix time>`, for example `Sort Everything-1234-1-1-1584112564.zip`.
/// Dots in the version are written as hyphens, so a version is the parts with a digit: `1-0-1` or `2-0b`.
/// The name may also contain hyphens and numbers, so the id is the first numeric part, followed only by the version.
pub fn parse_dashed(file_name: &str) -> Option<ArchiveName> {
    let mut parts: Vec<&str> = strip_extension(file_name).split('-').collect();
    let stamp = parts.pop()?;
    if !(9..=11).contains(&stamp.len()) || !stamp.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let at = (1..parts.len().saturating_sub(1)).find(|&i| {
        is_number(parts[i]) && parts[i + 1..].iter().all(|part| is_version_part(part))
    })?;
    let name = parts[..at].join("-").trim().to_string();
    if name.is_empty() {
        return None;
    }
    Some(ArchiveName {
        name,
        mod_id: parts[at].parse().ok()?,
        version: parts[at + 1..].join("."),
    })
}

/// A version part: letters, digits and dots, with at least one digit. `1`, `0b` and `1.0` match, `beta` does not.
fn is_version_part(part: &str) -> bool {
    part.bytes().any(|b| b.is_ascii_digit())
        && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    // The helper mirrors the shape of `parse`, which returns None for names it cannot read.
    #[allow(
        clippy::unnecessary_wraps,
        reason = "the helper mirrors `parse`'s Option so the assertions compare like with like"
    )]
    fn archive(name: &str, mod_id: u32, version: &str) -> Option<ArchiveName> {
        Some(ArchiveName {
            name: name.into(),
            mod_id,
            version: version.into(),
        })
    }

    #[test]
    fn reads_name_id_and_version_from_the_nexus_file_name() {
        assert_eq!(
            parse("Remove Mods Menu 35686 1.2.0 2026-10-23T17-21Z Q6xF6llIa.zip"),
            archive("Remove Mods Menu", 35686, "1.2.0")
        );
        assert_eq!(
            parse("redscript 1511 0.5.32 2026-10-22T18-29Z aI50IAAGE.zip"),
            archive("redscript", 1511, "0.5.32")
        );
        assert_eq!(
            parse("redscript 1511 0.5.32 2026-10-22T18-29Z aI50IAAGE.7z"),
            archive("redscript", 1511, "0.5.32")
        );
        assert_eq!(
            parse("redscript 1511 0.5.32 2026-10-22T18-29Z aI50IAAGE.rar"),
            archive("redscript", 1511, "0.5.32")
        );
    }

    #[test]
    fn the_name_chosen_by_the_author_can_hold_dashes_and_versions() {
        assert_eq!(
            parse("CET 1.38.0 - Game version 2.5 107 1.38.0 2026-10-22T12-57Z Q6xF6lly7.zip"),
            archive("CET 1.38.0 - Game version 2.5", 107, "1.38.0")
        );
    }

    #[test]
    fn the_token_and_the_extension_are_optional() {
        assert_eq!(
            parse("redscript 1511 0.5.32 2026-10-22T18-29Z"),
            archive("redscript", 1511, "0.5.32")
        );
        assert_eq!(
            parse("redscript 1511 0.5.32 2026-10-22T18-29Z.zip"),
            archive("redscript", 1511, "0.5.32")
        );
    }

    #[test]
    fn a_polish_name_is_kept_whole() {
        assert_eq!(
            parse("Crème Brûlée 35686 1.0 2026-10-23T17-21Z abc.zip"),
            archive("Crème Brûlée", 35686, "1.0")
        );
    }

    #[test]
    fn reads_the_older_dashed_format() {
        assert_eq!(
            parse("Sort Everything-1234-1-1-1584112564.zip"),
            archive("Sort Everything", 1234, "1.1")
        );
        assert_eq!(
            parse("Immersive-Wolf-Overhaul-2254-2-2-4-1600000000.zip"),
            archive("Immersive-Wolf-Overhaul", 2254, "2.2.4")
        );
        assert_eq!(
            parse("Mod-2-Fix-1234-1-0-1600000000"),
            archive("Mod-2-Fix", 1234, "1.0")
        );
        assert_eq!(
            parse("Name-55-2-0b-1600000000.zip"),
            archive("Name", 55, "2.0b")
        );
    }

    #[test]
    fn dashed_names_without_a_nexus_shape_are_left_alone() {
        for name in [
            "Sort-Everything-1234.zip",
            "Name-1234-1-0-12.zip",
            "Name-1234-beta-1600000000.zip",
            "Name-1234-1.zip",
            "Name-1234-1-0-1600000000x.zip",
        ] {
            assert_eq!(parse(name), None, "{name}");
        }
    }

    #[test]
    fn other_archive_names_are_not_nexus_ones() {
        for name in [
            "Sort Everything-1234.zip",
            "Foo-1.zip",
            "Immersive Overhaul 2.2.4.zip",
            "Remove Mods Menu 35686 1.2.0.zip",
            "Name abc 1.0 2026-10-23T17-21Z.zip",
            "1511 1.0 2026-10-23T17-21Z.zip",
            "Name 35686 1.0 2026-10-23T17-21Z one two.zip",
            "",
        ] {
            assert_eq!(parse(name), None, "{name}");
        }
    }
}
