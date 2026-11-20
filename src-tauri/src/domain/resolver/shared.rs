//! The shared kinds of file: the game reads a file of one of these kinds from one mod only, so mods that bring one meet
//! in the plan.

use crate::domain::deploy::Shared;
use crate::domain::pathkey::path_key;

/// Whether `text` ends with `suffix`, ignoring ASCII case. Both are paths, so the suffix is ASCII in practice.
fn ends_with_ignore_ascii_case(text: &str, suffix: &str) -> bool {
    text.len() >= suffix.len()
        && text
            .as_bytes()
            .get(text.len() - suffix.len()..)
            .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix.as_bytes()))
}

/// The shared kinds of file with their folder keys prepared once, so the check of one file does not rebuild them.
pub struct SharedRules(Vec<(String, Vec<String>)>);

impl SharedRules {
    pub fn new(rules: &[Shared]) -> Self {
        Self(
            rules
                .iter()
                .map(|rule| {
                    let dir = path_key(&rule.dir);
                    let prefix = if dir.is_empty() {
                        dir
                    } else {
                        format!("{dir}/")
                    };
                    (
                        prefix,
                        rule.extensions.iter().map(|e| e.to_lowercase()).collect(),
                    )
                })
                .collect(),
        )
    }

    /// Whether a file of a mod folder is of a shared kind. Case and separators are ignored.
    pub fn matches(&self, rel: &str) -> bool {
        if self.0.is_empty() {
            return false;
        }
        // Check the extension on the raw path first: most files have a different kind, and building the full
        // path key for every one of them was the expensive part of a status/build plan.
        let mut key: Option<String> = None;

        for (prefix, extensions) in &self.0 {
            if !extensions.is_empty()
                && !extensions
                    .iter()
                    .any(|extension| ends_with_ignore_ascii_case(rel, extension))
            {
                continue;
            }

            if prefix.is_empty() {
                return true;
            }

            let key = key.get_or_insert_with(|| path_key(rel));

            if key.starts_with(prefix.as_str()) {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_kinds_match_their_folder_and_extension() {
        let rules = SharedRules::new(&[Shared {
            dir: "Content".into(),
            extensions: vec![".ws".into()],
        }]);
        assert!(rules.matches("content/scripts/a.ws"));
        assert!(rules.matches("Content\\Scripts\\A.WS"));
        assert!(!rules.matches("content/a.xml"));
        assert!(!rules.matches("contentx/a.ws"));
        assert!(!rules.matches("DLC/a.ws"));
        let anywhere = SharedRules::new(&[Shared {
            dir: String::new(),
            extensions: vec![".ws".into()],
        }]);
        assert!(anywhere.matches("anywhere/a.ws"));
    }
}
