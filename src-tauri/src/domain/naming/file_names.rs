//! File and folder names that Windows accepts, and the bounded variants Gobbo writes.

use super::hashes::short_hash;

/// The longest name Gobbo writes for a file or a library folder. The extension is kept when a name is shortened.
pub const FILE_NAME_LIMIT: usize = 150;

/// The file name at the end of a URL's path. The query and the fragment are not part of it; a caller whose links carry
/// them in the file name strips them before calling. `None` when the URL ends in a slash or is empty.
pub fn file_name_from_url(url: &str) -> Option<&str> {
    url.rsplit('/').next().filter(|name| !name.is_empty())
}

/// A name that Windows accepts, shortened to [`FILE_NAME_LIMIT`] with the extension kept.
pub fn safe_limited_file_name(name: &str) -> String {
    bounded_file_name(&safe_file_name(name), FILE_NAME_LIMIT)
}

/// Whether a character is not allowed in a file or folder name on Windows.
pub const fn is_invalid_name_char(c: char) -> bool {
    c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
}

/// A name that Windows accepts as a file or folder name. The visible part stays close to the original.
pub fn safe_file_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());

    for c in name.chars() {
        out.push(if is_invalid_name_char(c) { '-' } else { c });
    }

    let out = out.trim().trim_end_matches(['.', ' ']).to_string();
    let out = if out.is_empty() {
        "mod".to_string()
    } else {
        out
    };

    if is_reserved_file_name(&out) {
        format!("_{out}")
    } else {
        out
    }
}

/// Whether the stem (the part before the first dot) is a DOS device name on Windows.
fn is_reserved_file_name(name: &str) -> bool {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = name.split('.').next().unwrap_or_default();
    RESERVED.iter().any(|word| stem.eq_ignore_ascii_case(word))
}

/// Whether an archive or tool path component can be written as a file on this platform. On Windows a name such as
/// `CON`, a trailing dot or space, or one of `<>:"/\\|?*` would address a device or an invalid path.
pub fn is_safe_component(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }

    if cfg!(windows) {
        if name.ends_with(['.', ' ']) || is_reserved_file_name(name) {
            return false;
        }

        if name.chars().any(is_invalid_name_char) {
            return false;
        }
    }

    true
}

/// Keeps a name below a safe length. The extension is preserved when it fits; a long stem and then a long extension
/// are shortened around the short hash, so the result never exceeds `limit`.
pub fn bounded_file_name(name: &str, limit: usize) -> String {
    if name.chars().count() <= limit {
        return trim_name_end(name.to_string());
    }

    let hash = short_hash(name);
    // The dash and the hash take the tail of the name, so a limit too small to hold both gets a plain truncation.
    let suffix = hash.len() + 1;

    if limit <= suffix {
        return trim_name_end(name.chars().take(limit).collect());
    }

    let path = std::path::Path::new(name);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    let extension: String = extension.chars().take(limit - suffix).collect();
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("mod");
    let keep = limit.saturating_sub(extension.chars().count() + suffix);
    let stem: String = stem.chars().take(keep).collect();
    trim_name_end(format!("{stem}-{hash}{extension}"))
}

/// A shortened name may end in the middle of a run of spaces or dots. Windows refuses that ending.
fn trim_name_end(mut name: String) -> String {
    while name.ends_with(['.', ' ']) {
        name.pop();
    }

    if name.is_empty() {
        "mod".to_string()
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn safe_file_name_replaces_what_windows_refuses() {
        assert_eq!(safe_file_name("a:b*c"), "a-b-c");
        assert_eq!(safe_file_name("What?<|>"), "What----");
        assert_eq!(safe_file_name("tab\u{1}x"), "tab-x");
    }

    #[test]
    fn safe_file_name_drops_trailing_dots_and_spaces_and_guards_reserved_names() {
        assert_eq!(safe_file_name("  name. "), "name");
        assert_eq!(safe_file_name("   "), "mod");
        assert_eq!(safe_file_name(""), "mod");
        assert_eq!(safe_file_name("con"), "_con");
        assert_eq!(safe_file_name("NUL.txt"), "_NUL.txt");
        assert_eq!(safe_file_name("Console"), "Console");
    }

    #[cfg(windows)]
    #[test]
    fn windows_device_names_and_trailing_dots_are_not_safe_components() {
        assert!(!is_safe_component("CON"));
        assert!(!is_safe_component("NUL.txt"));
        assert!(!is_safe_component("name."));
        assert!(!is_safe_component("name "));
        assert!(!is_safe_component("a:b"));
        assert!(!is_safe_component("a?b"));
        assert!(is_safe_component("Console"));
        assert!(is_safe_component("name.txt"));
    }

    #[test]
    fn bounded_file_name_keeps_short_names_and_the_extension_of_long_ones() {
        assert_eq!(bounded_file_name("short.ws", 150), "short.ws");
        let long = format!("{}.ws", "a".repeat(200));
        assert_eq!(
            bounded_file_name(&long, 150),
            format!("{}-{}.ws", "a".repeat(138), short_hash(&long))
        );
        let no_extension = "b".repeat(30);
        assert_eq!(
            bounded_file_name(&no_extension, 20),
            format!("{}-{}", "b".repeat(11), short_hash(&no_extension))
        );
    }

    #[test]
    fn an_extension_longer_than_the_limit_is_shortened_too() {
        let name = format!("x.{}", "a".repeat(200));
        let limited = safe_limited_file_name(&name);
        assert_eq!(limited.chars().count(), FILE_NAME_LIMIT);
        assert!(limited.contains(&short_hash(&name)));
    }

    /// Names made of ordinary text plus every character Windows treats specially, and the exact device names the
    /// reserved rule exists for. Long names exercise the shortening branch.
    fn name_strategy() -> impl Strategy<Value = String> {
        const EDGE_NAMES: &[&str] = &[
            "", "   ", ".", "..", "CON", "con", "NUL.txt", "nul", "AUX", "aux.", "COM1", "LPT9.log",
        ];
        let ordinary = proptest::collection::vec(
            prop_oneof![
                12 => any::<char>(),
                1 => Just('.'),
                1 => Just(' '),
                1 => Just('\u{7}'),
                1 => Just(':'),
                1 => Just('\\'),
                1 => Just('/'),
                1 => Just('*'),
                1 => Just('?'),
                1 => Just('<'),
                1 => Just('>'),
                1 => Just('|'),
                1 => Just('"'),
            ],
            0..300,
        )
        .prop_map(|chars| chars.into_iter().collect::<String>());
        prop_oneof![
            9 => ordinary,
            1 => prop::sample::select(EDGE_NAMES).prop_map(str::to_string),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Whatever comes in, the visible part keeps no invalid character, no trailing dot or space, and no device
        /// name. The name is never empty: an empty source becomes `mod`.
        #[test]
        fn safe_file_name_meets_the_windows_rules(name in name_strategy()) {
            let safe = safe_file_name(&name);
            prop_assert!(!safe.is_empty());
            // Windows trims ASCII spaces and dots; Rust's `trim()` also strips Unicode whitespace such as NBSP, which
            // is a valid file-name character. Only the leading edge and the Windows-specific trailing rule apply.
            prop_assert_eq!(safe.as_str(), safe.trim_start());
            prop_assert!(!safe.ends_with(['.', ' ']));
            prop_assert!(!safe.chars().any(is_invalid_name_char));
            prop_assert!(!is_reserved_file_name(&safe));
        }

        /// The shortened name keeps every rule of `safe_file_name` and stays under the limit, even when the extension
        /// alone is longer than the room the limit leaves.
        #[test]
        fn safe_limited_file_name_meets_the_windows_rules_and_the_limit(name in name_strategy()) {
            let limited = safe_limited_file_name(&name);
            prop_assert!(!limited.is_empty());
            prop_assert_eq!(limited.as_str(), limited.trim_start());
            prop_assert!(!limited.ends_with(['.', ' ']));
            prop_assert!(!limited.chars().any(is_invalid_name_char));
            prop_assert!(!is_reserved_file_name(&limited));
            prop_assert!(limited.chars().count() <= FILE_NAME_LIMIT);
        }

        /// A name `safe_file_name` builds can be written as a folder on every platform, Windows included.
        #[test]
        fn safe_file_name_is_a_safe_component(name in name_strategy()) {
            prop_assert!(is_safe_component(&safe_file_name(&name)));
        }

        /// A component is non-empty and is not `.` or `..`; on Windows it also follows the device and character rules.
        #[test]
        fn a_safe_component_is_a_plain_name(name in name_strategy()) {
            if is_safe_component(&name) {
                prop_assert!(!name.is_empty());
                prop_assert_ne!(name.as_str(), ".");
                prop_assert_ne!(name.as_str(), "..");

                if cfg!(windows) {
                    prop_assert!(!name.ends_with(['.', ' ']));
                    prop_assert!(!name.chars().any(is_invalid_name_char));
                    prop_assert!(!is_reserved_file_name(&name));
                }
            }
        }
    }
}
