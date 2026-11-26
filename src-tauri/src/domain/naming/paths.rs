//! Relative path rules: which paths stay inside a root and how an archive path is normalized.

use super::file_names::is_safe_component;

/// Whether the text is one folder name, not a path: non-empty, not `.`/`..`, no separator.
pub fn is_plain_folder(folder: &str) -> bool {
    !folder.is_empty() && folder != "." && folder != ".." && !folder.contains(['/', '\\'])
}

/// Whether a path is relative and stays inside the game (or another root): no drive, no leading separator, no `..`.
pub fn is_game_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with(['/', '\\'])
        && !path.contains(':')
        && !path.split(['/', '\\']).any(|part| part == "..")
}

/// Whether every component of a relative path can be written. `..`, an absolute path and a drive letter are always
/// refused; the Windows device and character rules are applied when `is_windows` is true.
pub fn is_safe_relative_path(path: &str, is_windows: bool) -> bool {
    is_game_relative(path)
        && path
            .split(['/', '\\'])
            .filter(|part| !part.is_empty() && *part != ".")
            .all(|part| is_safe_component(part, is_windows))
}

/// A path inside an archive or a recipe, with `/` separators, or `None` when it is unsafe. Backslashes become `/`, and
/// the outer slashes go: a leading one names the archive's root, not the disk's. An empty part (`a//b`), a `.` part
/// and every path `is_safe_relative_path` refuses return `None`. The `is_windows` rule is passed through to that check.
///
/// This is the safe-relative-path validation of a stored path. It is not a comparison key (see `domain::pathkey`) and
/// not the in-place normalizing form a FOMOD source pattern uses: those are separate concerns and are deliberately not
/// merged.
pub fn normalized_relative_path(path: &str, is_windows: bool) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let trimmed = normalized.trim_matches('/');

    if trimmed.is_empty()
        || trimmed
            .split('/')
            .any(|part| part.is_empty() || part == ".")
    {
        return None;
    }

    is_safe_relative_path(trimmed, is_windows).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn a_relative_path_cannot_leave_its_root() {
        for is_windows in [false, true] {
            assert!(is_safe_relative_path("mods/modA/content/x.ws", is_windows));
            assert!(is_safe_relative_path(
                "mods\\modA\\content\\x.ws",
                is_windows
            ));
            assert!(!is_safe_relative_path("", is_windows));
            assert!(!is_safe_relative_path("../outside", is_windows));
            assert!(!is_safe_relative_path("/absolute", is_windows));
            assert!(!is_safe_relative_path(r"C:\outside", is_windows));
        }
    }

    #[test]
    fn normalized_relative_path_uses_slashes_and_refuses_unsafe_paths() {
        for is_windows in [false, true] {
            assert_eq!(
                normalized_relative_path(r"a\b", is_windows),
                Some("a/b".to_string())
            );
            assert_eq!(
                normalized_relative_path("a/b/", is_windows),
                Some("a/b".to_string())
            );
            assert_eq!(normalized_relative_path("", is_windows), None);
            assert_eq!(
                normalized_relative_path("/x", is_windows),
                Some("x".to_string())
            );
            assert_eq!(
                normalized_relative_path(r"\x\", is_windows),
                Some("x".to_string())
            );
            assert_eq!(normalized_relative_path("/", is_windows), None);
            assert_eq!(normalized_relative_path(r"C:\x", is_windows), None);
            assert_eq!(normalized_relative_path("a/../b", is_windows), None);
            assert_eq!(normalized_relative_path("./a", is_windows), None);
            assert_eq!(normalized_relative_path("a//b", is_windows), None);
        }
    }

    #[test]
    fn a_windows_device_path_is_refused_only_on_windows() {
        assert!(!is_safe_relative_path("mods/CON/x.ws", true));
        assert!(is_safe_relative_path("mods/CON/x.ws", false));
        assert_eq!(normalized_relative_path("mods/CON/x.ws", true), None);
        assert_eq!(
            normalized_relative_path("mods/CON/x.ws", false),
            Some("mods/CON/x.ws".to_string())
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// A game-relative path never leaves its root: no drive, no leading separator, no `..` component.
        #[test]
        fn a_game_relative_path_stays_inside_its_root(path in any::<String>()) {
            if is_game_relative(&path) {
                prop_assert!(!path.is_empty());
                prop_assert!(!path.starts_with(['/', '\\']));
                prop_assert!(!path.contains(':'));
                prop_assert!(!path.split(['/', '\\']).any(|part| part == ".."));
            }
        }
    }
}
