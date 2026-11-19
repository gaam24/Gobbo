//! Version numbers: which of two versions is newer. Mods use versions that are not semver (`2.0b`, `1.2.3-rev2`), so
//! they are compared token by token.

use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cmp::Ordering;

/// How the newest version a provider lists compares with the version a profile selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    /// The check brought no version, or it failed.
    Unknown,
    /// The selected version is the newest one.
    Current,
    /// The provider's file chain or the version comparison says the newest version is newer.
    Newer,
    /// The provider lists a different version, but the version strings cannot say which one is newer. The user
    /// decides whether it is an update, a variant or a beta.
    Different,
    /// The user ignored this exact version. It stays hidden until the provider lists another one.
    Ignored,
}

impl UpdateState {
    /// Whether the newest version is an update the user can take: a newer version, or a different one the user decides
    /// on. This is the one rule for "an update is available"; the window reads it as `UpdateReport::has_update`.
    pub const fn is_update(self) -> bool {
        matches!(self, Self::Newer | Self::Different)
    }
}

/// One version the user chose not to be reminded about. The file id is kept when the provider names one, so a
/// re-upload with the same version string is a new version and not the ignored one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoredVersion {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
}

impl IgnoredVersion {
    /// Whether this ignored version is the one the provider now lists. Without a file id on either side the version
    /// string decides; a file id that appears where the ignored one had none counts as a new version.
    pub fn matches(&self, latest: Option<&str>, latest_file: Option<&str>) -> bool {
        let Some(latest) = latest else {
            return false;
        };
        match (self.file_id.as_deref(), latest_file) {
            (Some(ignored), Some(latest)) => ignored == latest,
            (None, Some(_)) => false,
            _ => self.version == latest,
        }
    }
}

/// The state of a provider's newest version against the selected one. The provider's file chain is exact when both
/// ends are named, so it decides first. Without both ids, a differing version is only `Different`: Nexus uses version
/// strings that do not sort (`1.1.3a` after `1.1.3`), and the user decides what such a version means.
pub fn update_state(
    latest: Option<&str>,
    latest_file: Option<&str>,
    current: Option<&str>,
    current_file: Option<&str>,
) -> UpdateState {
    let Some(latest) = latest else {
        return UpdateState::Unknown;
    };
    if let (Some(latest_file), Some(current_file)) = (latest_file, current_file) {
        return if latest_file == current_file {
            UpdateState::Current
        } else {
            UpdateState::Newer
        };
    }
    let same_version = current == Some(latest);
    if latest_file.is_some() || current_file.is_some() {
        return if same_version {
            UpdateState::Current
        } else {
            UpdateState::Different
        };
    }
    match current {
        Some(current) if current == latest => UpdateState::Current,
        Some(current) if is_newer(latest, current) => UpdateState::Newer,
        Some(_) | None => UpdateState::Different,
    }
}

/// Applies a user's ignore to a state. The current version is always current, and an unknown check stays unknown;
/// every other difference becomes ignored while the provider lists the same version.
pub fn with_ignored(
    state: UpdateState,
    latest: Option<&str>,
    latest_file: Option<&str>,
    ignored: Option<&IgnoredVersion>,
) -> UpdateState {
    if matches!(state, UpdateState::Unknown | UpdateState::Current) {
        return state;
    }
    match ignored {
        Some(ignored) if ignored.matches(latest, latest_file) => UpdateState::Ignored,
        _ => state,
    }
}

/// Is `candidate` newer than `current`?
///
/// Alphanumeric tokens are compared one by one: numbers numerically, text alphabetically. When the starts are equal,
/// the length difference decides: `1.2.3` > `1.2`, and `1.2` = `1.2.0` (trailing zeros do not make a newer version).
pub fn is_newer(candidate: &str, current: &str) -> bool {
    compare(candidate, current) == Ordering::Greater
}

/// Orders two installed versions the way the pickers show them: newer first, then the newer install, then the
/// folder name. Keys are optional because a version may have no version string at all.
pub fn compare_display(
    left: Option<&str>,
    left_installed: u32,
    left_folder: &str,
    right: Option<&str>,
    right_installed: u32,
    right_folder: &str,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => match compare(left, right) {
            Ordering::Greater => Ordering::Less,
            Ordering::Less => Ordering::Greater,
            Ordering::Equal => Ordering::Equal,
        },
        _ => Ordering::Equal,
    }
    .then_with(|| right_installed.cmp(&left_installed))
    .then_with(|| left_folder.cmp(right_folder))
}

/// Compares two versions without semver rules, because Nexus uses versions like `2.0b` or `1.2.3-rev2`.
/// The comparison walks both versions at once and allocates only when a version needs Unicode lowercasing.
pub fn compare(candidate: &str, current: &str) -> Ordering {
    let mut left = Tokens::new(candidate);
    let mut right = Tokens::new(current);
    loop {
        let order = match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            (Some(a), Some(b)) => a.cmp(&b),
            // A missing token is a zero, so `1.2` and `1.2.0` are equal.
            (Some(a), None) => a.cmp(&Token::Number(0)),
            (None, Some(b)) => Token::Number(0).cmp(&b),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Token<'a> {
    Number(u64),
    Text(&'a str),
}

impl PartialOrd for Token<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Token<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Number(a), Self::Number(b)) => a.cmp(b),
            (Self::Text(a), Self::Text(b)) => text_cmp(a, b),
            // A number is greater than text, so `2.0` is newer than `2.0b`: a letter marks an intermediate release.
            (Self::Number(_), Self::Text(_)) => Ordering::Greater,
            (Self::Text(_), Self::Number(_)) => Ordering::Less,
        }
    }
}

/// A tokenizer over one version. It walks the version without building a `Vec<Token>` first; a version with only
/// ASCII letters is borrowed, while Unicode lowercasing (which can change the number of characters) gets a `String`.
struct Tokens<'a> {
    text: Cow<'a, str>,
    at: usize,
}

impl<'a> Tokens<'a> {
    fn new(version: &'a str) -> Self {
        let trimmed = version.trim();
        let text = if trimmed.is_ascii() {
            Cow::Borrowed(trimmed)
        } else {
            // The old tokenizer lowercased the whole version before splitting it, which matters for characters
            // such as the Kelvin sign. Keep that behaviour for the rare non-ASCII version.
            Cow::Owned(trimmed.to_lowercase())
        };
        Self { text, at: 0 }
    }

    fn next(&mut self) -> Option<Token<'_>> {
        let bytes = self.text.as_bytes();
        while self.at < bytes.len() && !bytes[self.at].is_ascii_alphanumeric() {
            self.at += 1;
        }
        if self.at >= bytes.len() {
            return None;
        }
        let start = self.at;
        if bytes[self.at].is_ascii_digit() {
            let mut value = 0_u64;
            let mut overflow = false;
            while self.at < bytes.len() && bytes[self.at].is_ascii_digit() {
                let digit = u64::from(bytes[self.at] - b'0');
                match value
                    .checked_mul(10)
                    .and_then(|part| part.checked_add(digit))
                {
                    Some(next) => value = next,
                    None => overflow = true,
                }
                self.at += 1;
            }
            // `str::parse` returned 0 for an overflowing run; keep that exact result.
            Some(Token::Number(if overflow { 0 } else { value }))
        } else {
            while self.at < bytes.len() && bytes[self.at].is_ascii_alphabetic() {
                self.at += 1;
            }
            Some(Token::Text(&self.text[start..self.at]))
        }
    }
}

/// Compares two text tokens without allocating lowercased copies. The tokens contain ASCII letters only.
fn text_cmp(left: &str, right: &str) -> Ordering {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let common = left.len().min(right.len());
    for at in 0..common {
        let order = left[at]
            .to_ascii_lowercase()
            .cmp(&right[at].to_ascii_lowercase());
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_newer_or_different_version_is_an_update() {
        assert!(UpdateState::Newer.is_update());
        assert!(UpdateState::Different.is_update());
        assert!(!UpdateState::Current.is_update());
        assert!(!UpdateState::Ignored.is_update());
        assert!(!UpdateState::Unknown.is_update());
    }

    /// The tokenizer this module replaced. Kept as an oracle for the property test.
    fn old_tokens(version: &str) -> Vec<OldToken> {
        let mut result = Vec::new();
        let mut number = String::new();
        let mut text = String::new();
        for c in version.trim().to_lowercase().chars() {
            if c.is_ascii_digit() {
                if !text.is_empty() {
                    result.push(OldToken::Text(std::mem::take(&mut text)));
                }
                number.push(c);
            } else if c.is_ascii_alphabetic() {
                if !number.is_empty() {
                    result.push(OldToken::Number(number.parse().unwrap_or(0)));
                    number.clear();
                }
                text.push(c);
            } else if !number.is_empty() || !text.is_empty() {
                if !number.is_empty() {
                    result.push(OldToken::Number(number.parse().unwrap_or(0)));
                    number.clear();
                }
                if !text.is_empty() {
                    result.push(OldToken::Text(std::mem::take(&mut text)));
                }
            }
        }
        if !number.is_empty() {
            result.push(OldToken::Number(number.parse().unwrap_or(0)));
        }
        if !text.is_empty() {
            result.push(OldToken::Text(text));
        }
        result
    }

    #[derive(Debug, PartialEq, Eq)]
    enum OldToken {
        Number(u64),
        Text(String),
    }

    fn old_compare(candidate: &str, current: &str) -> Ordering {
        let left = old_tokens(candidate);
        let right = old_tokens(current);
        let length = left.len().max(right.len());
        for index in 0..length {
            let empty = OldToken::Number(0);
            let a = left.get(index).unwrap_or(&empty);
            let b = right.get(index).unwrap_or(&empty);
            let order = match (a, b) {
                (OldToken::Number(a), OldToken::Number(b)) => a.cmp(b),
                (OldToken::Text(a), OldToken::Text(b)) => a.cmp(b),
                (OldToken::Number(_), OldToken::Text(_)) => Ordering::Greater,
                (OldToken::Text(_), OldToken::Number(_)) => Ordering::Less,
            };
            if order != Ordering::Equal {
                return order;
            }
        }
        Ordering::Equal
    }

    #[test]
    fn newer_versions_win_on_numbers_and_letters() {
        assert!(is_newer("1.10", "1.9"));
        assert!(is_newer("2.0", "1.9.9"));
        assert!(is_newer("2.1", "2.0b"));
        assert!(!is_newer("2.0b", "2.0")); // a version with a letter is an intermediate release, not a revision
        assert!(!is_newer("1.9", "1.10"));
        assert!(!is_newer("1.2.3", "1.2.3"));
        assert!(!is_newer("1.2", "1.2.0"));
        assert!(!is_newer("", ""));
        assert!(is_newer("1.2", ""));
        assert!(is_newer("0.5.32", "0.5.9"));
        assert!(!is_newer("1.2.3-rev1", "1.2.3"));
    }

    #[test]
    fn tokens_split_numbers_letters_and_separators() {
        // Different separators are skipped, so they produce the same token stream.
        assert_eq!(compare("1.2.3", "1-2-3"), Ordering::Equal);
        assert_eq!(compare("1.2.3", "1_2_3"), Ordering::Equal);
        assert_eq!(compare("1.2.3", "1 2 3"), Ordering::Equal);

        // A letter run is one text token and the digits after it are one number token:
        // `rev2` is Text("rev") then 2, which sorts below Text("rev") then 10.
        assert!(!is_newer("1.2.3-rev2", "1.2.3-rev10"));
        assert_eq!(compare("rev2", "rev10"), Ordering::Less);
        assert_eq!(compare("item2", "item10"), Ordering::Less);

        // Numbers are parsed whole at a letter boundary: 2 < 10, not digit by digit.
        assert_eq!(compare("2b", "10a"), Ordering::Less);
        assert_eq!(compare("a2", "a10"), Ordering::Less);
    }

    #[test]
    fn a_file_chain_with_both_ids_decides_the_update() {
        assert_eq!(
            update_state(Some("1.1.3a"), Some("20"), Some("1.1.3"), Some("20")),
            UpdateState::Current
        );
        assert_eq!(
            update_state(Some("1.1.3a"), Some("20"), Some("1.1.3"), Some("10")),
            UpdateState::Newer
        );
    }

    #[test]
    fn without_a_file_chain_the_version_strings_decide_between_newer_and_different() {
        assert_eq!(
            update_state(Some("2.0"), None, Some("1.9"), None),
            UpdateState::Newer
        );
        assert_eq!(
            update_state(Some("1.1.3a"), None, Some("1.1.3"), None),
            UpdateState::Different
        );
        assert_eq!(
            update_state(Some("1.1.3a"), None, Some("1.1.3a"), None),
            UpdateState::Current
        );
    }

    #[test]
    fn a_partly_known_file_chain_is_a_difference_not_a_newer_version() {
        assert_eq!(
            update_state(Some("1.1.3a"), Some("20"), Some("1.1.3"), None),
            UpdateState::Different
        );
        assert_eq!(
            update_state(Some("1.1.3a"), None, Some("1.1.3"), Some("10")),
            UpdateState::Different
        );
        assert_eq!(
            update_state(Some("1.1.3a"), Some("20"), Some("1.1.3a"), None),
            UpdateState::Current
        );
    }

    #[test]
    fn an_ignored_version_matches_by_file_or_version() {
        let ignored = IgnoredVersion {
            version: "1.1.3a".into(),
            file_id: Some("20".into()),
        };
        assert!(ignored.matches(Some("1.1.3a"), Some("20")));
        assert!(!ignored.matches(Some("1.1.3a"), Some("21")));
        assert!(ignored.matches(Some("1.1.3a"), None));

        let without_file = IgnoredVersion {
            version: "1.1.3a".into(),
            file_id: None,
        };
        assert!(without_file.matches(Some("1.1.3a"), None));
        assert!(!without_file.matches(Some("1.1.3a"), Some("20")));
        assert!(!without_file.matches(Some("1.1.3b"), None));
        assert!(!without_file.matches(None, None));
    }

    #[test]
    fn an_ignore_hides_a_difference_but_not_a_current_version() {
        let ignored = IgnoredVersion {
            version: "1.1.3a".into(),
            file_id: Some("20".into()),
        };
        assert_eq!(
            with_ignored(
                UpdateState::Different,
                Some("1.1.3a"),
                Some("20"),
                Some(&ignored)
            ),
            UpdateState::Ignored
        );
        assert_eq!(
            with_ignored(
                UpdateState::Newer,
                Some("1.1.3a"),
                Some("20"),
                Some(&ignored)
            ),
            UpdateState::Ignored
        );
        assert_eq!(
            with_ignored(
                UpdateState::Current,
                Some("1.1.3a"),
                Some("20"),
                Some(&ignored)
            ),
            UpdateState::Current
        );
        assert_eq!(
            with_ignored(UpdateState::Different, Some("1.1.3b"), None, Some(&ignored)),
            UpdateState::Different
        );
    }

    #[test]
    fn the_new_comparator_matches_the_old_one() {
        // Deterministic pseudo-random strings: ASCII letters, digits, separators, Unicode and very long numbers.
        let pieces = [
            "",
            "0",
            "1",
            "2",
            "9",
            "10",
            "01",
            "a",
            "b",
            "rev",
            "Z",
            " ",
            ".",
            "-",
            "_",
            "é",
            "K",
            "99999999999999999999999999999999999999999999999999",
        ];
        let mut state = 0x9E37_79B9_u64;
        for _ in 0..4_000 {
            let mut make = || {
                let mut text = String::new();
                for _ in 0..=(state % 6) {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1);
                    text.push_str(pieces[(state as usize) % pieces.len()]);
                }
                text
            };
            let left = make();
            let right = make();
            assert_eq!(
                compare(&left, &right),
                old_compare(&left, &right),
                "{left:?} vs {right:?}"
            );
        }
    }
}
