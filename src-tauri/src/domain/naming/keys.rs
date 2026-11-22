//! Keys derived from display names: a clean name, a local slug, an MO2 match key and a compact cache component.

use crate::domain::pathkey::starts_with_ignore_ascii_case;

/// Keeps ASCII letters, digits, `_` and `-`. An empty name becomes `Import`.
pub fn clean(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if clean.is_empty() {
        "Import".to_string()
    } else {
        clean
    }
}

/// A lowercase key for local mod names, used for a family id without a provider source and for a pack's id. Unicode
/// letters and digits are kept; runs of other characters become one `-`, and an empty name becomes `mod`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;

    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "mod".to_string()
    } else {
        out
    }
}

/// The name in the form the two managers may write it: letters and digits only, in lower case. Matching a Mod Organizer
/// 2 mod-list name against a library folder ignores case, spacing and punctuation, so no separator is kept.
pub fn match_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// A path component for a cache key: ASCII letters, digits, `-` and `_` are kept and every other byte becomes `_`; an
/// empty component becomes `unknown`. This is a cache-key policy and must stay stable across versions — changing it
/// orphans the cached plans and archives of existing collections.
pub fn compact_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());

    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
            out.push(char::from(byte));
        } else {
            out.push('_');
        }
    }

    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

/// The folder under a slot, for example `modExampleMod`. A name that already has the prefix keeps it.
pub fn deploy_name(prefix: &str, display_name: &str, folder: &str) -> String {
    let base = if display_name.trim().is_empty() {
        folder
    } else {
        display_name
    };

    if prefix.is_empty() || starts_with_ignore_ascii_case(base, prefix) {
        clean(base)
    } else {
        format!("{prefix}{}", clean(base))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_names_take_the_prefix_once() {
        assert_eq!(deploy_name("mod", "Example Mod", "x"), "modExampleMod");
        assert_eq!(deploy_name("mod", "modHoods", "x"), "modHoods");
        assert_eq!(deploy_name("", "Some Modlet", "folder"), "SomeModlet");
        assert_eq!(deploy_name("mod", "", "Archive Name"), "modArchiveName");
    }

    #[test]
    fn slugs_collapse_unicode_and_separators() {
        // (name, expected key).
        let cases = [
            ("Mod Name (v1.2)", "mod-name-v1-2"),
            ("  A  B  ", "a-b"),
            ("Über Mod", "über-mod"),
            ("trailing---", "trailing"),
            ("...", "mod"),
            ("", "mod"),
        ];

        for (name, expected) in cases {
            assert_eq!(slug(name), expected, "name={name}");
        }
    }

    #[test]
    fn match_keys_ignore_case_spacing_and_punctuation() {
        // (name, expected key).
        let cases = [
            ("Mod Name (v1.2)", "modnamev12"),
            ("modname v12", "modnamev12"),
            ("Mod A", "moda"),
            ("Mod B", "modb"),
        ];

        for (name, expected) in cases {
            assert_eq!(match_key(name), expected, "name={name}");
        }

        assert_eq!(match_key("Mod Name (v1.2)"), match_key("modname v12"));
        assert_ne!(match_key("Mod A"), match_key("Mod B"));
    }

    #[test]
    fn compact_components_are_stable_cache_keys() {
        // (text, expected component).
        let cases = [
            ("nexus", "nexus"),
            ("game-key_2", "game-key_2"),
            ("../escape", "___escape"),
            ("a b/c", "a_b_c"),
            ("Ünïcode", "__n__code"),
            ("", "unknown"),
        ];

        for (text, expected) in cases {
            assert_eq!(compact_component(text), expected, "text={text}");
        }
    }
}
