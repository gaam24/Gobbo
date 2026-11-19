//! The one rule every import path shares for the name and version stored in the library.

use crate::domain::library::ArchiveLabel;

/// The one rule every import path shares for the name and version stored in the library. A provider's label outranks
/// `info.json`; the provider's answer for the exact file outranks both the label and `info.json`. `info_json` is the
/// `modName` before the folder fallback: pass `None` when there is no real name, or pass the folder echo in `stem` so
/// it is recognised and ignored. The folder itself always comes from the archive file stem, never from a label or a
/// caller's display hint.
pub fn imported(
    label: Option<&ArchiveLabel>,
    api_version: Option<&str>,
    info_json: Option<&str>,
    info_json_version: Option<&str>,
    stem: &str,
) -> (String, Option<String>) {
    let name = label
        .and_then(|label| label.name.as_deref())
        .filter(|name| !name.is_empty())
        .or_else(|| info_json.filter(|name| !name.is_empty() && *name != stem))
        .unwrap_or(stem)
        .to_string();
    let version = api_version
        .or_else(|| label.map(|label| label.version.as_str()))
        .or(info_json_version)
        .filter(|version| !version.is_empty())
        .map(str::to_string);
    (name, version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_follows_one_name_and_version_precedence() {
        let labelled = ArchiveLabel {
            name: Some("Label Name".to_string()),
            version: "3.0".to_string(),
        };
        let nameless = ArchiveLabel {
            name: None,
            version: "2.0".to_string(),
        };
        // (label, provider answer, info.json name, info.json version, stem) -> (name, version).
        let cases = [
            (
                Some(&labelled),
                Some("9.0"),
                Some("Info Name"),
                Some("1.0"),
                "stem",
                ("Label Name", Some("9.0")),
            ),
            (
                Some(&labelled),
                None,
                Some("Info Name"),
                Some("1.0"),
                "stem",
                ("Label Name", Some("3.0")),
            ),
            (
                Some(&nameless),
                None,
                Some("Info Name"),
                Some("1.0"),
                "stem",
                ("Info Name", Some("2.0")),
            ),
            (
                Some(&nameless),
                None,
                None,
                Some("1.0"),
                "stem",
                ("stem", Some("2.0")),
            ),
            (
                Some(&nameless),
                None,
                Some("stem"),
                Some("1.0"),
                "stem",
                ("stem", Some("2.0")),
            ),
            (
                None,
                None,
                Some("Info Name"),
                Some("1.0"),
                "stem",
                ("Info Name", Some("1.0")),
            ),
            (None, None, None, Some("1.0"), "stem", ("stem", Some("1.0"))),
            (
                None,
                Some("9.0"),
                None,
                Some("1.0"),
                "stem",
                ("stem", Some("9.0")),
            ),
            (None, None, None, None, "stem", ("stem", None)),
        ];
        for (label, api, name, version, stem, expected) in cases {
            let (found_name, found_version) = imported(label, api, name, version, stem);
            assert_eq!(
                (found_name.as_str(), found_version.as_deref()),
                expected,
                "label={label:?}, api={api:?}, name={name:?}, version={version:?}, stem={stem}"
            );
        }
    }
}
