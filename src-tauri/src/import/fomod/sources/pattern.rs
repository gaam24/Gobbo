/// A FOMOD source pattern prepared once. The installer can have hundreds of patterns and the archive thousands of
/// entries, so normalizing a pattern for every entry was the hot part of a FOMOD import.
pub struct SourcePattern {
    source: String,
    /// `source/` for a pattern without `*`; `None` for a wildcard or an empty pattern.
    prefix: Option<String>,
    wildcard: bool,
    /// For the common `prefix*suffix` pattern: the literal text on both sides of the only `*`.
    around: Option<(String, String)>,
}

impl SourcePattern {
    /// Prepares a pattern from `ModuleConfig.xml`. The entry passed to `matches` must be normalized with
    /// `normalize_path`.
    pub fn new(source: &str) -> Self {
        let source = normalize_path(source);
        let wildcard = source.contains('*');
        let prefix = (!source.is_empty() && !wildcard).then(|| format!("{source}/"));
        // Almost every FOMOD wildcard is `folder/*` or `folder/*.ext`. Remembering the two literal sides turns the
        // match into two slice comparisons instead of walking the whole pattern and entry.
        let stars = source.bytes().filter(|byte| *byte == b'*').count();
        let around = if stars == 1 {
            source
                .split_once('*')
                .map(|(head, tail)| (head.to_string(), tail.to_string()))
        } else {
            None
        };
        Self {
            source,
            prefix,
            wildcard,
            around,
        }
    }

    /// Whether a normalized archive entry matches the pattern. A pattern without `*` is a file or a folder (a folder
    /// takes everything in it); `*` is any string, also across separators.
    pub fn matches(&self, entry: &str) -> bool {
        if self.source.is_empty() {
            return false;
        }
        if let Some((head, tail)) = &self.around {
            return entry.len() >= head.len() + tail.len()
                && entry.starts_with(head.as_str())
                && entry.ends_with(tail.as_str());
        }
        if self.wildcard {
            wildcard(&self.source, entry)
        } else {
            entry == self.source
                || self
                    .prefix
                    .as_ref()
                    .is_some_and(|prefix| entry.starts_with(prefix.as_str()))
        }
    }
}

/// Does the archive entry match the FOMOD source? A convenience wrapper for one-off checks; a loop over many entries
/// should build `SourcePattern` values once.
#[cfg(test)]
pub fn matches_source(source: &str, entry: &str) -> bool {
    SourcePattern::new(source).matches(&normalize_path(entry))
}

/// Normalizes an archive entry the way FOMOD source patterns compare it: forward slashes, no `./` at the start,
/// lowercase. This is an in-place matching form, not a safety check and not a comparison key: it keeps `..` and does
/// not validate the path, so it stays separate from `crate::domain::naming::normalized_relative_path`
/// (safe-relative-path validation) and `crate::domain::pathkey` (comparison keys).
pub fn normalize_path(path: &str) -> String {
    let mut normalized = String::with_capacity(path.len());
    normalize_path_into(path, &mut normalized);
    normalized
}

/// `normalize_path` into a reusable buffer. A FOMOD import normalizes every archive entry; reusing one `String` avoids
/// one allocation per entry.
pub fn normalize_path_into(path: &str, normalized: &mut String) {
    normalized.clear();
    normalized.reserve(path.len());
    // Archive entries are usually already normalized. Checking first avoids the rewrite; the branches below keep the
    // exact result of the old `replace`/`to_lowercase` chain.
    let plain = path.is_ascii()
        && !path
            .bytes()
            .any(|byte| byte == b'\\' || byte.is_ascii_uppercase());
    if plain {
        normalized.push_str(path);
    } else if path.is_ascii() {
        for byte in path.bytes() {
            let byte = match byte {
                b'\\' => b'/',
                b'A'..=b'Z' => byte + (b'a' - b'A'),
                _ => byte,
            };
            normalized.push(byte as char);
        }
    } else {
        for c in path.chars() {
            if c == '\\' {
                normalized.push('/');
            } else {
                normalized.extend(c.to_lowercase());
            }
        }
    }
    let mut start = usize::from(normalized.starts_with("./")) * 2;
    while normalized.as_bytes().get(start) == Some(&b'/') {
        start += 1;
    }
    let mut end = normalized.len();
    while end > start && normalized.as_bytes().get(end - 1) == Some(&b'/') {
        end -= 1;
    }
    if start > 0 {
        normalized.drain(..start);
    }
    normalized.truncate(end.saturating_sub(start));
}

/// Matching with `*`: the prefix before the first asterisk must match, then the rest of the pattern is matched from
/// every possible position. This is the standard two-pointer wildcard walk; it does not recurse and does not allocate.
const fn wildcard(pattern: &str, text: &str) -> bool {
    let pattern = pattern.as_bytes();
    let text = text.as_bytes();
    let (mut pattern_at, mut text_at) = (0, 0);
    let mut star = None;
    let mut star_text_at = 0;
    while text_at < text.len() {
        if pattern_at < pattern.len() && pattern[pattern_at] == text[text_at] {
            pattern_at += 1;
            text_at += 1;
        } else if pattern_at < pattern.len() && pattern[pattern_at] == b'*' {
            star = Some(pattern_at);
            pattern_at += 1;
            star_text_at = text_at;
        } else if let Some(star_at) = star {
            pattern_at = star_at + 1;
            star_text_at += 1;
            text_at = star_text_at;
        } else {
            return false;
        }
    }
    while pattern_at < pattern.len() && pattern[pattern_at] == b'*' {
        pattern_at += 1;
    }
    pattern_at == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The recursive matcher this module replaced, kept as an oracle for the linear one.
    fn old_wildcard(pattern: &str, text: &str) -> bool {
        match pattern.split_once('*') {
            None => pattern == text,
            Some((prefix, suffix)) => {
                let Some(rest) = text.strip_prefix(prefix) else {
                    return false;
                };
                if suffix.is_empty() {
                    return true;
                }
                (0..=rest.len()).any(|skip| {
                    text.get(prefix.len() + skip..)
                        .is_some_and(|tail| old_wildcard(suffix, tail))
                })
            }
        }
    }

    #[test]
    fn the_linear_wildcard_matches_the_recursive_one() {
        let patterns = [
            "*", "a*", "*a", "a*b", "a**b", "*.ws", "a/*/c", "a*b*c", "", "no-star",
        ];
        let texts = [
            "", "a", "ab", "a/b", "a/b/c", "aXXb", "aXXbYYc", "file.ws", "no-star", "no-sta",
        ];
        for pattern in patterns {
            for text in texts {
                assert_eq!(
                    wildcard(pattern, text),
                    old_wildcard(pattern, text),
                    "{pattern:?} vs {text:?}"
                );
            }
        }
    }

    #[test]
    fn a_prepared_pattern_uses_a_normalized_entry() {
        let pattern = SourcePattern::new("Textures\\2K");
        assert!(pattern.matches(&normalize_path("textures/2k/a.nif")));
        assert!(!pattern.matches(&normalize_path("textures/4k/a.nif")));
        assert!(!SourcePattern::new("").matches("anything"));
    }
}
