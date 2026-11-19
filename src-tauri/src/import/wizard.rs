//! The FOMOD parts an import needs: finding `ModuleConfig.xml`, reading it, and turning the wizard choices into the
//! set of archive entries that go into the library, with the game path each one is installed to.

use crate::error::AppError;
use crate::error::ErrorKind;
use crate::import::fomod;
use crate::infra::archive;
use std::collections::{BTreeMap, BTreeSet};

/// Looks for `fomod/ModuleConfig.xml` in the archive (letter case and slashes do not matter). Gives its index.
pub(super) fn fomod_config_entry(reader: &archive::Archive) -> Option<usize> {
    reader.entries().iter().position(|entry| {
        !entry.is_dir && entry.name.eq_ignore_ascii_case("fomod/moduleconfig.xml")
    })
}

pub(super) fn read_config(reader: &archive::Archive, index: usize) -> Result<String, AppError> {
    // The installer definition is a small XML file. A cap keeps a damaged or malicious archive from making the
    // import allocate an unbounded buffer before it is rejected.
    const CONFIG_MAX: u64 = 16 * 1024 * 1024;
    let bytes = reader.read_bytes_limited(index, CONFIG_MAX)?;
    // ModuleConfig.xml can be UTF-16 (with a BOM or without), not only UTF-8.
    fomod::decode(&bytes)
}

/// The entries a FOMOD choice installs and where the installer wants each one.
#[derive(Debug, Default)]
pub(super) struct FomodSelection {
    /// Indexes of the archive entries the choice selects.
    pub(super) allowed: BTreeSet<usize>,
    /// Game path of a selected entry, taken from its pattern's `destination`. An entry without a usable destination
    /// is absent, so the import keeps its archive path.
    pub(super) virtual_paths: BTreeMap<usize, String>,
}

/// The set of archive entry indexes that go into the library according to the FOMOD wizard choices, together with the
/// virtual game path each entry installs to.
pub(super) fn fomod_allowed(
    reader: &archive::Archive,
    choices: &[fomod::Choice],
) -> Result<FomodSelection, AppError> {
    let Some(config_entry) = fomod_config_entry(reader) else {
        return Err(AppError::new("import.not_fomod").kind(ErrorKind::Permanent));
    };
    let text = read_config(reader, config_entry)?;
    let config = fomod::parse(&text)?;
    let sources = fomod::resolve_sources(&config, choices)?;
    // Normalize every pattern and every entry once. The old check did both for every pair, which is quadratic in
    // the number of patterns and entries.
    let patterns: Vec<fomod::SourcePattern> = sources
        .iter()
        .map(|pattern| fomod::SourcePattern::new(&pattern.source))
        .collect();
    let mut selection = FomodSelection::default();
    let mut name = String::new();
    for (index, entry) in reader.entries().iter().enumerate() {
        if entry.is_dir {
            continue;
        }
        fomod::normalize_path_into(&entry.name, &mut name);
        // Several patterns can match one entry. Only a single, unambiguous source and destination may move the file;
        // otherwise the archive path is the safer answer.
        let mut matched: Option<(&str, &str)> = None;
        let mut ambiguous = false;
        for (pattern, source) in patterns.iter().zip(&sources) {
            if !pattern.matches(&name) {
                continue;
            }
            let candidate = (source.source.as_str(), source.destination.as_str());
            match matched {
                None => matched = Some(candidate),
                Some(existing) if existing != candidate => {
                    ambiguous = true;
                    break;
                }
                Some(_) => {}
            }
        }
        if matched.is_none() {
            continue;
        }
        selection.allowed.insert(index);
        if !ambiguous
            && let Some((source, destination)) = matched
            && !destination.is_empty()
        {
            selection
                .virtual_paths
                .insert(index, virtual_game_path(&entry.name, source, destination));
        }
    }
    if selection.allowed.is_empty() {
        return Err(AppError::user("fomod.no_files_selected"));
    }
    Ok(selection)
}

/// The components of a path, in the case the installer wrote them. Both separators are accepted; `.` and empty parts
/// are dropped the same way `fomod::normalize_path` drops them before matching.
fn components(path: &str) -> Vec<&str> {
    path.split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != ".")
        .collect()
}

/// Where an entry ends relative to the source that matched it, as a component index into the entry. `None` means the
/// source and the entry do not line up, so the destination cannot be applied safely.
fn remainder_start(source: &[&str], entry: &[&str]) -> Option<usize> {
    let Some(first_star) = source.iter().position(|part| part.contains('*')) else {
        // A file or folder pattern: the entry must start with every source component. An exact match leaves no
        // remainder, so the destination is the full target path; a folder match leaves the folder's contents.
        if entry.len() < source.len()
            || !source
                .iter()
                .zip(entry)
                .all(|(known, part)| known.eq_ignore_ascii_case(part))
        {
            return None;
        }
        return Some(source.len());
    };
    // A wildcard stands for the components between the literal parts around it. Those literal parts must match for
    // the remainder to be the text the wildcard actually consumed.
    if entry.len() < first_star
        || !source[..first_star]
            .iter()
            .zip(entry)
            .all(|(known, part)| known.eq_ignore_ascii_case(part))
    {
        return None;
    }
    let last_star = source.iter().rposition(|part| part.contains('*'))?;
    let tail = &source[last_star + 1..];
    if tail.len() > entry.len() - first_star {
        return None;
    }
    let tail_at = entry.len() - tail.len();
    if !tail
        .iter()
        .zip(&entry[tail_at..])
        .all(|(known, part)| known.eq_ignore_ascii_case(part))
    {
        return None;
    }
    Some(first_star)
}

/// The game path a FOMOD pattern installs an entry to. The normalized match only says *which* pattern applies; the
/// path itself is built from the original entry (case preserved) and the pattern's destination:
///
/// - a specific file with no remainder takes the destination as its full path;
/// - a wildcard or a folder appends the entry components after the source prefix to the destination;
/// - a missing, ambiguous or unsafe destination falls back to the original archive path.
fn virtual_game_path(original: &str, source: &str, destination: &str) -> String {
    let fallback = || original.to_string();
    if !crate::domain::naming::is_safe_relative_path(destination) {
        return fallback();
    }
    let mut source_norm = String::new();
    let mut entry_norm = String::new();
    fomod::normalize_path_into(source, &mut source_norm);
    fomod::normalize_path_into(original, &mut entry_norm);
    let source_parts: Vec<&str> = source_norm
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    let entry_parts: Vec<&str> = entry_norm
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    let original_parts = components(original);
    if source_parts.is_empty()
        || entry_parts.len() != original_parts.len()
        || original_parts
            .iter()
            .any(|part| !crate::domain::naming::is_safe_component(part))
    {
        return fallback();
    }
    let Some(start) = remainder_start(&source_parts, &entry_parts) else {
        return fallback();
    };
    let mut game_path = components(destination).join("/");
    for part in &original_parts[start..] {
        game_path.push('/');
        game_path.push_str(part);
    }
    game_path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_specific_file_takes_the_destination_as_its_full_path() {
        let path = virtual_game_path(
            "fomod/parts/archive/pc/mod/Data.xl",
            "fomod/parts/archive/pc/mod/Data.xl",
            "archive/pc/mod/Data.xl",
        );
        assert_eq!(path, "archive/pc/mod/Data.xl");
    }

    #[test]
    fn a_wildcard_installs_the_matched_suffix_into_the_destination_folder() {
        let path = virtual_game_path("fomod/parts/fonts/Base.nif", "fomod/parts/fonts/*", "fonts");
        assert_eq!(path, "fonts/Base.nif");
    }

    #[test]
    fn a_folder_installs_its_contents_below_the_destination() {
        let path = virtual_game_path("fomod/colours/Red.esp", "fomod/colours", "Custom Folder");
        assert_eq!(path, "Custom Folder/Red.esp");
    }

    #[test]
    fn an_unsafe_destination_keeps_the_archive_path() {
        for destination in ["../outside", "/absolute", "C:\\game", "a/../b"] {
            let path = virtual_game_path("fomod/colours/Red.esp", "fomod/colours", destination);
            assert_eq!(path, "fomod/colours/Red.esp", "{destination}");
        }
    }

    #[test]
    fn a_source_that_does_not_cover_the_entry_keeps_the_archive_path() {
        let path = virtual_game_path(
            "fomod/parts/fonts/Base.nif",
            "fomod/parts/textures/*",
            "fonts",
        );
        assert_eq!(path, "fomod/parts/fonts/Base.nif");
    }
}
