//! Path keys. Windows ignores letter case and uses `\`, Linux does not. We always compare keys.

use std::ops::Range;
use std::path::{Path, PathBuf};

/// Turns a path into a key: `/` as the separator, no `.` or empty segments, lowercase.
pub fn path_key(path: &str) -> String {
    let mut key = String::with_capacity(path.len());
    key_with_into(path, '/', &mut key);
    key
}

/// A path in the form Windows compares it: lower case, `\` as the only separator, no trailing separator.
pub fn windows_key(path: &str) -> String {
    let mut key = String::with_capacity(path.len());
    key_with_into(path, '\\', &mut key);
    key
}

/// Expands a `~` at the start of a typed path to `home`: `~` alone, or `~/` or `~\` before the rest. Any other text is
/// kept as it is, and so is every path when `home` is empty, because `~/x` without a home would become `/x`.
pub fn expand_home(text: &str, home: &Path) -> String {
    let Some(rest) = text
        .strip_prefix('~')
        .filter(|_| !home.as_os_str().is_empty())
    else {
        return text.to_string();
    };

    match rest.strip_prefix(['/', '\\']) {
        Some(tail) => home.join(tail).display().to_string(),
        None if rest.is_empty() => home.display().to_string(),
        None => text.to_string(),
    }
}

/// Appends the key of `path` to `out` and returns the range it occupies. A caller that builds many keys can keep them
/// in one string instead of allocating one `String` per path.
pub fn path_key_into(path: &str, out: &mut String) -> Range<usize> {
    let start = out.len();
    key_with_into(path, '/', out);
    start..out.len()
}

/// Builds a key with one separator and lower case, skipping `.` and empty segments. The segment search uses
/// `memchr`, and an ASCII segment that is already lower case is copied as one slice instead of one byte at a time.
fn key_with_into(path: &str, separator: char, key: &mut String) {
    let bytes = path.as_bytes();
    let mut start = 0;
    let mut first = true;

    loop {
        let end =
            memchr::memchr2(b'/', b'\\', &bytes[start..]).map_or(bytes.len(), |at| start + at);
        let segment = &path[start..end];

        if !segment.is_empty() && segment != "." {
            if !first {
                key.push(separator);
            }

            first = false;
            push_lower(key, segment);
        }

        if end == bytes.len() {
            break;
        }

        start = end + 1;
    }
}

/// Appends one path segment in lower case. Almost every game path is ASCII and already lower case, so the common
/// case is one copy; the Unicode branch stays for names that need it.
fn push_lower(key: &mut String, segment: &str) {
    if segment.is_ascii() {
        if segment.bytes().any(|byte| byte.is_ascii_uppercase()) {
            key.reserve(segment.len());

            for byte in segment.bytes() {
                key.push(byte.to_ascii_lowercase() as char);
            }
        } else {
            key.push_str(segment);
        }
    } else {
        for c in segment.chars() {
            key.extend(c.to_lowercase());
        }
    }
}

/// Folds a name the way every path key is folded: ASCII is lowered byte by byte when it needs it, and names that are
/// not ASCII fall back to full Unicode lowercasing. Use this for names compared with the app's path keys; when many
/// keys are built into one buffer, `push_lower` appends a segment without allocating a separate string.
pub fn fold_case(text: &str) -> String {
    let mut folded = String::with_capacity(text.len());
    fold_case_into(text, &mut folded);
    folded
}

/// Clears `out` and folds `text` into it with the same rule as `fold_case`, so a caller comparing many names can reuse
/// one buffer instead of allocating a string per name.
pub fn fold_case_into(text: &str, out: &mut String) {
    out.clear();
    push_lower(out, text);
}

/// Whether `path` is `root` or lies below it. Windows compares ignoring case; Linux compares the paths as they
/// are. This is the one containment rule for profile data, user data and the game folder.
pub fn is_inside(root: &Path, path: &Path) -> bool {
    // An unset game folder is not a root that contains every path. Without this, an empty key is the prefix of every
    // Windows path and a game with no saved folder looks running in diagnostics and in the state lock.
    if root.as_os_str().is_empty() {
        return false;
    }

    if cfg!(windows) {
        let root = windows_key(&root.to_string_lossy());
        let path = windows_key(&path.to_string_lossy());
        path == root
            || path
                .strip_prefix(root.as_str())
                .is_some_and(|rest| rest.starts_with('\\'))
    } else {
        path.starts_with(root)
    }
}

/// Whether `text` starts with `prefix`, ignoring case. The game definitions use ASCII prefixes, and the byte check for
/// them avoids allocating a lowercased copy; a non-ASCII prefix falls back to the full Unicode lowercasing.
pub fn starts_with_ignore_ascii_case(text: &str, prefix: &str) -> bool {
    if prefix.is_ascii() {
        text.as_bytes()
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()))
    } else {
        text.to_lowercase().starts_with(&prefix.to_lowercase())
    }
}

/// Whether `text` ends with `suffix`, ignoring ASCII case.
pub fn ends_with_ascii_case(text: &str, suffix: &str) -> bool {
    text.len() >= suffix.len()
        && text
            .as_bytes()
            .get(text.len() - suffix.len()..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix.as_bytes()))
}

/// `path` relative to `base`, when it is `base` or lies below it. Windows compares ignoring case; Linux compares the
/// paths as they are. The result keeps the spelling of `path`.
pub fn relative_to(base: &Path, path: &Path) -> Option<PathBuf> {
    relative_to_for(cfg!(windows), base, path)
}

/// `relative_to` with the platform given, so the Windows and Linux rules can both be tested on one machine. This is
/// the only place that decides how the two platforms compare path components.
fn relative_to_for(is_windows: bool, base: &Path, path: &Path) -> Option<PathBuf> {
    if is_windows {
        let base_key = windows_key(&base.to_string_lossy());
        let path_key = windows_key(&path.to_string_lossy());
        let rest = path_key.strip_prefix(&base_key)?;

        if !rest.is_empty() && !rest.starts_with('\\') {
            return None;
        }

        Some(path.components().skip(base.components().count()).collect())
    } else {
        path.strip_prefix(base).ok().map(Path::to_path_buf)
    }
}

/// Whether two paths name the same file or folder, following the platform's case rule. Links are resolved when both
/// exist, so a store's path and the game folder it points at compare equal; paths that cannot be resolved are compared
/// as they are.
pub fn same_path(first: &Path, second: &Path) -> bool {
    match (first.canonicalize(), second.canonicalize()) {
        (Ok(first), Ok(second)) => {
            relative_to(&first, &second).is_some_and(|relative| relative.as_os_str().is_empty())
        }
        _ => relative_to(first, second).is_some_and(|relative| relative.as_os_str().is_empty()),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn ignores_case_and_separators() {
        assert_eq!(
            path_key("Mods\\modHoods/content"),
            path_key("mods/modhoods/./CONTENT")
        );
        assert_eq!(path_key("a/b"), "a/b");
        assert_ne!(path_key("a/b"), path_key("a/bb"));
    }

    #[test]
    fn the_windows_key_uses_backslashes_and_no_trailing_separator() {
        assert_eq!(
            windows_key("D:/Games/Example Game/"),
            r"d:\games\example game"
        );
    }

    #[test]
    fn a_typed_home_path_expands_and_other_tildes_stay() {
        let home = Path::new("/home/slot");
        assert_eq!(expand_home("~", home), "/home/slot");
        assert_eq!(
            expand_home("~/Games/Example's Game", home),
            home.join("Games/Example's Game").display().to_string()
        );
        assert_eq!(expand_home("~user/Games", home), "~user/Games");
        assert_eq!(expand_home("/games/~/x", home), "/games/~/x");
        // Without a home folder the text stays as typed, instead of becoming a path at the root.
        assert_eq!(expand_home("~/Games", Path::new("")), "~/Games");
    }

    #[test]
    fn fold_case_folds_ascii_and_unicode_names() {
        assert_eq!(fold_case("Archive NAME.zip"), "archive name.zip");
        assert_eq!(fold_case("ÜberÄrger"), "überärger");
        assert_eq!(fold_case(""), "");
    }

    #[test]
    fn a_prefix_match_ignores_case_for_ascii_and_unicode_prefixes() {
        assert!(starts_with_ignore_ascii_case("ModRemove", "mod"));
        assert!(starts_with_ignore_ascii_case("modRemove", "mod"));
        assert!(!starts_with_ignore_ascii_case("mo", "mod"));
        assert!(starts_with_ignore_ascii_case("Üben", "üb"));
        assert!(!starts_with_ignore_ascii_case("Other", "üb"));
        assert!(starts_with_ignore_ascii_case("anything", ""));
    }

    #[test]
    fn a_suffix_match_ignores_ascii_case() {
        assert!(ends_with_ascii_case(".dll", ".dll"));
        assert!(ends_with_ascii_case("Mod/DLL.DLL", ".dll"));
        assert!(!ends_with_ascii_case("d", ".dll"));
        assert!(!ends_with_ascii_case("Mod/other.exe", ".dll"));
        assert!(ends_with_ascii_case("anything", ""));
    }

    #[test]
    fn containment_requires_a_whole_component() {
        assert!(is_inside(Path::new("a"), Path::new("a/b")));
        assert!(is_inside(Path::new("a"), Path::new("a")));
        assert!(!is_inside(Path::new("a"), Path::new("ab/b")));
    }

    #[test]
    fn an_empty_root_contains_nothing() {
        assert!(!is_inside(Path::new(""), Path::new("a/b")));
        #[cfg(windows)]
        assert!(!is_inside(Path::new(""), Path::new(r"C:\Games")));
    }

    #[cfg(windows)]
    #[test]
    fn containment_ignores_case_on_windows() {
        assert!(is_inside(
            Path::new(r"C:\Games"),
            Path::new(r"c:\games\Mods")
        ));
    }

    #[test]
    fn same_path_matches_equal_paths_and_spelling_differences() {
        let path = Path::new("games/example/game");

        assert!(same_path(path, Path::new("games/example/game")));
        assert!(same_path(path, Path::new("games/example/game/")));
        assert!(same_path(path, Path::new("games/example/./game")));
        assert!(!same_path(path, Path::new("games/example/other")));
    }

    #[test]
    fn both_case_rules_can_be_checked_on_one_machine() {
        let upper = Path::new("/Games/ExampleGame");
        let lower = Path::new("/games/examplegame");

        assert!(relative_to_for(false, upper, lower).is_none());
        assert!(
            relative_to_for(true, upper, lower)
                .is_some_and(|relative| relative.as_os_str().is_empty())
        );
    }

    #[cfg(windows)]
    #[test]
    fn same_path_ignores_case_on_windows() {
        assert!(same_path(
            Path::new(r"C:\Games\ExampleGame"),
            Path::new(r"c:\games\examplegame")
        ));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// A key has one separator, no empty or `.` segment and no trailing separator, whatever the source path.
        #[test]
        fn a_path_key_has_no_empty_or_dot_segments(path in any::<String>()) {
            let key = path_key(&path);
            prop_assert!(!key.contains('\\'));
            prop_assert!(!key.starts_with('/'));
            prop_assert!(!key.ends_with('/'));
            prop_assert!(
                key.is_empty()
                    || key
                        .split('/')
                        .all(|segment| !segment.is_empty() && segment != ".")
            );
        }

        /// Keying a key again changes nothing: lower case and `/` are already the key's form.
        #[test]
        fn a_path_key_is_stable(path in any::<String>()) {
            prop_assert_eq!(path_key(&path_key(&path)), path_key(&path));
        }
    }
}
