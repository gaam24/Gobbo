//! The problems a check collects, and the small rules that say whether one value is well formed.

use super::super::definition::Requirement;
use super::super::layout::is_game_relative;

/// The problems found so far. Each check adds its own; the list is the result of `validate`.
#[derive(Default)]
pub(super) struct Problems(pub(super) Vec<String>);

impl Problems {
    pub(super) fn push(&mut self, problem: impl Into<String>) {
        self.0.push(problem.into());
    }

    /// `min_gobbo`: a release number such as `0.3.0`, no newer than this app. A newer floor refuses the file, so an
    /// old app never serves a definition that depends on a later fix.
    pub(super) fn min_gobbo(&mut self, what: &str, min: &str) {
        let app = env!("CARGO_PKG_VERSION");

        if !is_release_version(min) {
            self.push(format!(
                "{what} min_gobbo must be a release number such as 0.3.0: {min:?}"
            ));
        } else if crate::domain::version::compare(min, app) == std::cmp::Ordering::Greater {
            self.push(format!("{what} needs Gobbo {min} or newer; this is {app}"));
        }
    }

    /// Every path must be relative and stay inside the game (or the mod): no drive, no leading `/`, no `..`.
    pub(super) fn relative<'a>(&mut self, what: &str, paths: impl IntoIterator<Item = &'a String>) {
        for path in paths {
            if !is_game_relative(path) {
                self.push(format!(
                    "{what} is not a relative path inside the game: {path:?}"
                ));
            }
        }
    }

    /// A path template: not empty and never leaving its folder through a `..` segment. A name that only contains two
    /// dots, such as `Team..Saves`, is an ordinary folder name.
    pub(super) fn template(&mut self, what: &str, template: &str) {
        if template.trim().is_empty() || template.split(['/', '\\']).any(|part| part == "..") {
            self.push(format!("{what} is not a safe path template: {template:?}"));
        }
    }

    /// Requirement metadata: a name, a relative path and, when present, the HTTPS page that offers the file.
    pub(super) fn requirements(&mut self, what: &str, list: &[Requirement]) {
        for requirement in list {
            if requirement.name.trim().is_empty() {
                self.push(format!("{what} requirement name is empty"));
            }

            if !is_game_relative(&requirement.path) {
                self.push(format!(
                    "{what} requirement path is not relative inside the game: {:?}",
                    requirement.path
                ));
            }

            if let Some(url) = &requirement.url
                && !url.starts_with("https://")
            {
                self.push(format!("{what} requirement URL is not HTTPS: {url:?}"));
            }
        }
    }

    /// Extensions as the code compares them: lowercase, with the dot.
    pub(super) fn extensions<'a>(
        &mut self,
        what: &str,
        extensions: impl IntoIterator<Item = &'a String>,
    ) {
        for extension in extensions {
            let well_formed = extension.len() > 1
                && extension.starts_with('.')
                && *extension == extension.to_lowercase();
            if !well_formed {
                self.push(format!(
                    "{what}: extension must be lowercase and start with a dot: {extension:?}"
                ));
            }
        }
    }
}

/// A loader variant: lowercase letters, digits, `-`, `_` or `.`, with no path and no relative segment.
pub(in crate::games) fn is_variant(text: &str) -> bool {
    !text.is_empty()
        && text != "."
        && text != ".."
        && text
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

/// What a provider needs to address a game: letters, digits, `-`, `_` or `.`, with no path and no relative segment.
pub(super) fn is_provider_key(text: &str) -> bool {
    !text.is_empty()
        && text != "."
        && text != ".."
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// 128 lowercase hex digits: a SHA-512 as `sha2` writes it.
pub(super) fn is_sha512(text: &str) -> bool {
    text.len() == 128
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A release number: three dot-separated groups of digits, such as `0.3.0`.
pub(in crate::games) fn is_release_version(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}
