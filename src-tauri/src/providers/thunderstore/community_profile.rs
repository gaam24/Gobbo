//! The `mods.yml` file of an r2modman or Gale profile. It names Thunderstore packages and their versions, so the format is
//! Thunderstore knowledge; the profile import (`profiles/community.rs`) reads the file and matches the packages to the
//! library.

/// One package as the profile file names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunityMod {
    /// The full Thunderstore package name, for example `Owner-Package`.
    pub name: String,
    pub version: String,
    pub enabled: bool,
}

/// Parses the `mods.yml` of r2modman and Gale. The file is a list of entries with `name`, a nested `version` table and
/// `enabled`; other keys are ignored. A quoted name and a plain `version: "1.2.3"` are accepted too.
pub fn parse_mods(text: &str) -> Vec<CommunityMod> {
    let mut mods = Vec::new();
    let mut current: Option<Pending> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("- name:") {
            if let Some(pending) = current.take() {
                mods.push(pending.finish());
            }
            current = Some(Pending {
                name: unquote(name),
                ..Pending::default()
            });
            continue;
        }
        let Some(pending) = current.as_mut() else {
            continue;
        };
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let value = unquote(value);
        match key.trim() {
            "major" => pending.major = Some(value),
            "minor" => pending.minor = Some(value),
            "patch" => pending.patch = Some(value),
            "version" => pending.version = value,
            "enabled" => pending.enabled = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    if let Some(pending) = current {
        mods.push(pending.finish());
    }
    mods.retain(|item| !item.name.is_empty());
    mods
}

#[derive(Default)]
struct Pending {
    name: String,
    major: Option<String>,
    minor: Option<String>,
    patch: Option<String>,
    version: String,
    enabled: bool,
}

impl Pending {
    fn finish(self) -> CommunityMod {
        let version = if self.version.is_empty() {
            [self.major, self.minor, self.patch]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(".")
        } else {
            self.version
        };
        CommunityMod {
            name: self.name,
            version,
            enabled: self.enabled,
        }
    }
}

fn unquote(value: &str) -> String {
    value
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mods_file_names_versions_and_enabled_flags() {
        let mods = parse_mods(
            "- name: Owner-One\n  version:\n    major: 1\n    minor: 2\n    patch: 3\n  enabled: true\n\
             - name: 'Owner-Two'\n  version: \"2.0.0\"\n  enabled: false\n",
        );
        assert_eq!(
            mods,
            [
                CommunityMod {
                    name: "Owner-One".into(),
                    version: "1.2.3".into(),
                    enabled: true
                },
                CommunityMod {
                    name: "Owner-Two".into(),
                    version: "2.0.0".into(),
                    enabled: false
                }
            ]
        );
    }
}
