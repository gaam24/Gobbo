//! The file names of archives: the extensions Gobbo reads and the small helpers the name readers share. Each provider
//! reads the names it writes; the Nexus reader is in `providers::nexus::archive_name`.

use std::path::Path;

/// The archive extensions Gobbo reads, without the dot and in lowercase.
pub const ARCHIVE_EXTENSIONS: [&str; 3] = ["zip", "7z", "rar"];

/// The supported archive extension of a name, in lowercase and without the dot. Other names give `None`.
pub fn extension_of(file_name: &str) -> Option<&'static str> {
    let (_, extension) = file_name.rsplit_once('.')?;
    ARCHIVE_EXTENSIONS
        .iter()
        .find(|known| extension.eq_ignore_ascii_case(known))
        .copied()
}

/// A name without a supported archive extension, in any letter case. Other names come back unchanged.
pub fn strip_extension(file_name: &str) -> &str {
    match file_name.rsplit_once('.') {
        Some((stem, _)) if extension_of(file_name).is_some() => stem,
        _ => file_name,
    }
}

/// Splits a file name into its stem and its extension with the dot, read the way `Path` reads it. A name without an
/// extension gives the whole name and an empty extension, so `.hidden` is all stem.
pub fn split_name(name: &str) -> (&str, &str) {
    let Some(extension) = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
    else {
        return (name, "");
    };
    let cut = name.len() - extension.len() - 1;
    (&name[..cut], &name[cut..])
}

/// Whether a string is a non-empty run of ASCII digits.
pub fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::split_name;

    #[test]
    fn splits_the_stem_from_the_extension() {
        assert_eq!(split_name("Mod 1.2.zip"), ("Mod 1.2", ".zip"));
        assert_eq!(split_name("Mod.7Z"), ("Mod", ".7Z"));
        assert_eq!(split_name("Mod"), ("Mod", ""));
        assert_eq!(split_name(".hidden"), (".hidden", ""));
    }
}
