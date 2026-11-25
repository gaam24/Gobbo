//! Folder patterns for the loader engine rules, such as `*_Data/Managed` or `*/Content/Paks`. A `/` separates the
//! segments, and a `*` in a segment matches any run of characters in that segment, none included. Names are compared
//! with the path key rule, so a pattern finds the same folders on Windows and on Linux. Nothing here reads the disk:
//! `find` asks a listing function for the entries of each folder it walks.

use super::naming::is_game_relative;
use super::pathkey::fold_case;

/// One entry of a folder, as the caller lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The name as the folder spells it.
    pub name: String,
    /// Whether the entry is a folder. A segment before the last one can only match a folder.
    pub is_dir: bool,
}

/// Whether `pattern` is a folder pattern the rules can use: a relative path with `/` separators, no `\`, no drive, and
/// no empty, `.` or `..` segment. A `*` may stand in any segment.
pub fn is_valid(pattern: &str) -> bool {
    is_game_relative(pattern)
        && !pattern.contains('\\')
        && pattern
            .split('/')
            .all(|segment| !segment.trim().is_empty() && segment != "." && segment != "..")
}

/// Whether `name` matches one segment of a pattern. `*` matches any run of characters, none included. Case is ignored
/// as every path key ignores it, so `*_data` matches `Foo_Data`.
pub fn segment_matches(segment: &str, name: &str) -> bool {
    let segment = fold_case(segment);
    let name = fold_case(name);
    let parts: Vec<&str> = segment.split('*').collect();

    let Some((first, rest)) = parts.split_first() else {
        return false;
    };

    let Some((last, middle)) = rest.split_last() else {
        return name == *first;
    };

    if name.len() < first.len() + last.len() || !name.starts_with(*first) || !name.ends_with(*last)
    {
        return false;
    }

    let mut remaining = &name[first.len()..name.len() - last.len()];

    for part in middle {
        match remaining.find(*part) {
            Some(at) => remaining = &remaining[at + part.len()..],
            None => return false,
        }
    }

    true
}

/// The relative paths that `pattern` matches, with `/` separators and spelled the way the listing spells them. `list`
/// returns the entries of one folder, given as a relative path (`""` is the folder the pattern starts in). Only the
/// folders the pattern walks are listed, one per segment, and the results keep the order of the listing.
pub fn find(pattern: &str, list: &mut dyn FnMut(&str) -> Vec<Entry>) -> Vec<String> {
    let segments: Vec<&str> = pattern.split('/').collect();
    let last_index = segments.len().saturating_sub(1);
    let mut found = vec![String::new()];

    for (index, segment) in segments.iter().enumerate() {
        let mut next = Vec::new();

        for base in &found {
            for entry in list(base.as_str()) {
                let fits_here = index == last_index || entry.is_dir;

                if fits_here && segment_matches(segment, &entry.name) {
                    next.push(join(base, &entry.name));
                }
            }
        }

        found = next;

        if found.is_empty() {
            break;
        }
    }

    found
}

/// `name` below `base`, with `/` between them; the root has no prefix.
fn join(base: &str, name: &str) -> String {
    if base.is_empty() {
        name.to_string()
    } else {
        format!("{base}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// A folder tree for the tests: each path is a file, and every folder above it exists. Listing a folder returns its
    /// direct entries, and an entry is a folder when another path lies below it.
    fn tree(files: &[&str]) -> impl FnMut(&str) -> Vec<Entry> {
        let mut paths = BTreeSet::new();

        for file in files {
            let mut path = String::new();

            for part in file.split('/') {
                if !path.is_empty() {
                    path.push('/');
                }

                path.push_str(part);
                paths.insert(path.clone());
            }
        }

        move |dir: &str| {
            paths
                .iter()
                .filter_map(|path| {
                    let (parent, name) = path.rsplit_once('/').unwrap_or(("", path.as_str()));

                    if parent != dir {
                        return None;
                    }

                    let below = format!("{path}/");
                    Some(Entry {
                        name: name.to_string(),
                        is_dir: paths.iter().any(|other| other.starts_with(&below)),
                    })
                })
                .collect()
        }
    }

    #[test]
    fn a_segment_matches_its_wildcards_and_ignores_case() {
        assert!(segment_matches("*_Data", "Example_Data"));
        assert!(segment_matches("*", "Any Folder"));
        assert!(segment_matches("Pal*", "Pal"));
        assert!(segment_matches("engine", "Engine"));
        assert!(segment_matches("a*b*c", "abc"));
        assert!(segment_matches("*x*", "axb"));
        assert!(segment_matches("Zażółć*", "zażółć gęślą"));
        assert!(!segment_matches("*_Data", "Example_Data_Old"));
        assert!(!segment_matches("a*b*c", "acb"));
        assert!(!segment_matches("*x*", "abc"));
        assert!(!segment_matches("Engine", "Engines"));
    }

    #[test]
    fn a_pattern_must_be_a_relative_path_of_whole_segments() {
        for good in [
            "UnityPlayer.dll",
            "*_Data/Managed",
            "*/Content/Paks",
            "Engine/Binaries",
        ] {
            assert!(is_valid(good), "{good}");
        }

        for bad in [
            "",
            "/Managed",
            "../Managed",
            "a//b",
            "a/./b",
            "a/../b",
            "C:/Managed",
            "a\\b",
            "a/ /b",
            "a/",
        ] {
            assert!(!is_valid(bad), "{bad:?}");
        }
    }

    #[test]
    fn find_returns_only_the_paths_the_pattern_names() {
        let files = [
            "Example_Data/Managed/Assembly-CSharp.dll",
            "UnityPlayer.dll",
            "Other/readme.txt",
        ];
        assert_eq!(
            find("*_Data/Managed", &mut tree(&files)),
            ["Example_Data/Managed"]
        );
        assert_eq!(find("*_Data", &mut tree(&files)), ["Example_Data"]);
        assert_eq!(
            find("UnityPlayer.dll", &mut tree(&files)),
            ["UnityPlayer.dll"]
        );
        assert_eq!(
            find("*/readme.txt", &mut tree(&files)),
            ["Other/readme.txt"]
        );
        assert!(find("Missing", &mut tree(&files)).is_empty());
    }

    #[test]
    fn a_file_is_never_a_folder_in_the_middle_of_a_pattern() {
        let files = ["UnityPlayer.dll"];
        assert!(find("UnityPlayer.dll/x", &mut tree(&files)).is_empty());
    }

    #[test]
    fn find_lists_one_folder_per_segment_and_no_more() {
        let files = [
            "Example_Data/Managed/Assembly-CSharp.dll",
            "Example_Data/Plugins/a.dll",
            "Other/x.dll",
        ];
        let mut listed = Vec::new();
        let found = find("*_Data/Managed", &mut |dir: &str| {
            listed.push(dir.to_string());
            tree(&files)(dir)
        });
        assert_eq!(found, ["Example_Data/Managed"]);
        assert_eq!(listed, ["", "Example_Data"]);
    }
}
