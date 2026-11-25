//! The engine of a game folder. A loader preset's `[engine]` table names the markers that all have to exist in the game
//! folder, the variant that fits, and, for an engine whose game keeps its mods below a project folder, the folder that
//! holds them. The rules are data: this file reads the folder for them, and the matching itself is in `domain::pattern`.

use std::path::Path;

use super::super::loaders::marker_alternatives;
use super::inspect::sorted_entries;
use super::{EngineSuggestion, LoaderEngineView, LoaderPresetView};
use crate::domain::pattern::{self, Entry};

/// The engine the game folder matches, among the presets that loaded cleanly. Only the folders the rules name are read,
/// and nothing is written.
pub(super) fn detect(folder: &Path, presets: &[LoaderPresetView]) -> Option<EngineSuggestion> {
    let mut list = |relative: &str| listing(folder, relative);
    let mut best: Option<(usize, EngineSuggestion)> = None;

    // The presets come in id order, so a tie keeps the earlier one.
    for preset in presets {
        let Some(engine) = &preset.engine else {
            continue;
        };

        if !preset.problems.is_empty() {
            continue;
        }

        if let Some((points, suggestion)) = match_preset(&mut list, preset, engine)
            && best.as_ref().is_none_or(|(most, _)| points > *most)
        {
            best = Some((points, suggestion));
        }
    }

    best.map(|(_, suggestion)| suggestion)
}

/// The suggestion for one preset, with the number of its markers that matched. `None` when a marker of the engine
/// itself has no match in the folder.
fn match_preset(
    list: &mut dyn FnMut(&str) -> Vec<Entry>,
    preset: &LoaderPresetView,
    engine: &LoaderEngineView,
) -> Option<(usize, EngineSuggestion)> {
    let mut points = count_matching(list, &engine.markers)?;
    let mut chosen = None;

    for candidate in &engine.variants {
        if let Some(count) = count_matching(list, &candidate.markers) {
            chosen = Some((candidate, count));
            break;
        }
    }

    let (variant, label) = if let Some((candidate, count)) = chosen {
        points += count;
        (Some(candidate.variant.clone()), candidate.label.clone())
    } else {
        let fallback = engine
            .variant
            .clone()
            .or_else(|| preset.default_variant.clone());
        let label = fallback
            .as_deref()
            .and_then(|name| engine.variants.iter().find(|entry| entry.variant == name))
            .and_then(|entry| entry.label.clone());
        (fallback, label)
    };

    let root = engine
        .root
        .as_deref()
        .and_then(|root_pattern| project_root(list, root_pattern, &engine.markers));

    Some((
        points,
        EngineSuggestion {
            loader_id: preset.id.clone(),
            loader_name: preset.name.clone(),
            engine_name: engine.name.clone(),
            variant,
            label,
            root,
        },
    ))
}

/// How many markers there are, when every one of them matches something in the folder. `None` when one has no match.
fn count_matching(list: &mut dyn FnMut(&str) -> Vec<Entry>, markers: &[String]) -> Option<usize> {
    for marker in markers {
        if !marker_matches(list, marker) {
            return None;
        }
    }

    Some(markers.len())
}

/// Whether one marker matches: one of its alternatives must exist in the folder (see `marker_alternatives`). A native
/// Linux build that names a file `.so` where Windows names it `.dll` matches the same marker.
fn marker_matches(list: &mut dyn FnMut(&str) -> Vec<Entry>, marker: &str) -> bool {
    marker_alternatives(marker).any(|alternative| !pattern::find(alternative, list).is_empty())
}

/// The project folder: the first segment of the first match of `root_pattern`, in name order. A folder that an engine
/// marker names is the engine's own, so it is never the project; `Engine` is skipped when a marker is `Engine/Binaries`.
fn project_root(
    list: &mut dyn FnMut(&str) -> Vec<Entry>,
    root_pattern: &str,
    markers: &[String],
) -> Option<String> {
    let engine_folders: Vec<&str> = markers
        .iter()
        .flat_map(|marker| marker_alternatives(marker))
        .filter_map(|alternative| alternative.split('/').next())
        .collect();

    pattern::find(root_pattern, list)
        .iter()
        .filter_map(|path| path.split('/').next())
        .find(|folder| {
            !engine_folders
                .iter()
                .any(|engine| pattern::segment_matches(engine, folder))
        })
        .map(str::to_string)
}

/// The entries of one folder of the game, by the relative path the rules use. A folder that cannot be read has none.
fn listing(folder: &Path, relative: &str) -> Vec<Entry> {
    let dir = if relative.is_empty() {
        folder.to_path_buf()
    } else {
        folder.join(relative)
    };

    sorted_entries(&dir)
        .iter()
        .map(|entry| Entry {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: entry.file_type().is_ok_and(|kind| kind.is_dir()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::presets::presets_from;
    use super::*;
    use crate::games::GameOrigin;
    use crate::test_support::{scratch, write};

    /// A preset with two engine markers, a variant that the folder shows by its own marker, and a project folder.
    const ENGINE_PRESET: &str = r#"
schema = 1
id = "fixture-engine"
name = "Fixture Engine"
default_variant = "plain"

[deploy]

[[deploy.slots]]
dir = "Mods"

[variants.plain]

[variants.extra]

[engine]
name = "Example Engine"
markers = ["Example.dll", "*_Data"]
variant = "plain"
root = "*/Project/Content"

[[engine.variants]]
variant = "extra"
label = "Extra"
markers = ["Extra.dll"]
"#;

    /// A preset whose engine marker is in an engine folder, and whose project folder keeps its paks.
    const PROJECT_PRESET: &str = r#"
schema = 1
id = "fixture-project"
name = "Fixture Project"

[deploy]

[[deploy.slots]]
dir = "Mods"

[engine]
name = "Example Project Engine"
markers = ["Engine/Binaries"]
root = "*/Content/Paks"
"#;

    /// The presets read from a fresh folder that holds one `<id>.toml` per pair.
    fn presets_in(name: &str, presets: &[(&str, &str)]) -> Vec<LoaderPresetView> {
        let dir = scratch!(name);

        for (id, text) in presets {
            write(&dir.join(format!("{id}.toml")), text);
        }

        presets_from(&[(dir, GameOrigin::Bundled)])
    }

    /// A game folder with the given files (relative paths with `/`), in a fresh scratch folder.
    fn game_folder(name: &str, files: &[&str]) -> PathBuf {
        let folder = scratch!(name);

        for file in files {
            write(&folder.join(file), "");
        }

        folder
    }

    #[test]
    fn the_variant_whose_markers_exist_is_chosen_and_the_default_otherwise() {
        let presets = presets_in(
            "engine-variant-presets",
            &[("fixture-engine", ENGINE_PRESET)],
        );

        let with_extra = game_folder(
            "engine-variant-extra",
            &["Example.dll", "Example_Data/a.dll", "Extra.dll"],
        );
        let found = detect(&with_extra, &presets).expect("the engine matches");
        assert_eq!(found.loader_id, "fixture-engine");
        assert_eq!(found.loader_name, "Fixture Engine");
        assert_eq!(found.engine_name, "Example Engine");
        assert_eq!(found.variant.as_deref(), Some("extra"));
        assert_eq!(found.label.as_deref(), Some("Extra"));
        assert_eq!(found.root, None, "the project folder is not there");

        let plain = game_folder(
            "engine-variant-plain",
            &["Example.dll", "Example_Data/a.dll"],
        );
        let found = detect(&plain, &presets).expect("the engine matches");
        assert_eq!(found.variant.as_deref(), Some("plain"));
        assert_eq!(found.label, None);
    }

    #[test]
    fn a_folder_missing_one_engine_marker_matches_nothing() {
        let presets = presets_in(
            "engine-missing-presets",
            &[("fixture-engine", ENGINE_PRESET)],
        );
        let folder = game_folder("engine-missing", &["Example.dll"]);
        assert!(detect(&folder, &presets).is_none());
    }

    #[test]
    fn the_project_folder_is_never_the_folder_an_engine_marker_names() {
        let presets = presets_in(
            "engine-root-presets",
            &[("fixture-project", PROJECT_PRESET)],
        );
        let folder = game_folder(
            "engine-root",
            &[
                "Engine/Binaries/engine.exe",
                "Engine/Content/Paks/engine.pak",
                "Project/Content/Paks/mod.pak",
            ],
        );
        let found = detect(&folder, &presets).expect("the engine matches");
        assert_eq!(found.root.as_deref(), Some("Project"));
        assert_eq!(
            found.variant, None,
            "a preset without variants suggests none"
        );
    }

    #[test]
    fn the_preset_with_the_most_matching_markers_wins_and_a_tie_keeps_the_earlier_one() {
        let one = "schema = 1\nid = \"a-engine\"\nname = \"A\"\n[deploy]\n[[deploy.slots]]\ndir = \"Mods\"\n\
                   [engine]\nname = \"One\"\nmarkers = [\"Shared.dll\"]\n";
        let two = "schema = 1\nid = \"b-engine\"\nname = \"B\"\n[deploy]\n[[deploy.slots]]\ndir = \"Mods\"\n\
                   [engine]\nname = \"Two\"\nmarkers = [\"Shared.dll\", \"Second.dll\"]\n";
        let same = "schema = 1\nid = \"b-engine\"\nname = \"B\"\n[deploy]\n[[deploy.slots]]\ndir = \"Mods\"\n\
                    [engine]\nname = \"Two\"\nmarkers = [\"Shared.dll\"]\n";

        let most = presets_in(
            "engine-most-presets",
            &[("a-engine", one), ("b-engine", two)],
        );
        let folder = game_folder("engine-most", &["Shared.dll", "Second.dll"]);
        assert_eq!(
            detect(&folder, &most).expect("a match").loader_id,
            "b-engine"
        );

        let tie = presets_in(
            "engine-tie-presets",
            &[("a-engine", one), ("b-engine", same)],
        );
        let folder = game_folder("engine-tie", &["Shared.dll"]);
        assert_eq!(
            detect(&folder, &tie).expect("a match").loader_id,
            "a-engine"
        );
    }

    /// A preset whose engine markers name one file two ways, as a Windows build and a native Linux build do. The
    /// variant marker lists alternatives too.
    const ALTERNATIVE_PRESET: &str = r#"
schema = 1
id = "fixture-alternative"
name = "Fixture Alternative"
default_variant = "plain"

[deploy]

[[deploy.slots]]
dir = "Mods"

[variants.plain]

[variants.extra]

[engine]
name = "Example Engine"
markers = ["Example.dll | Example.so", "*_Data"]
variant = "plain"

[[engine.variants]]
variant = "extra"
label = "Extra"
markers = ["Extra.dll|Extra.so"]
"#;

    #[test]
    fn a_marker_with_alternatives_matches_the_build_that_is_there() {
        let presets = presets_in(
            "engine-alternative-presets",
            &[("fixture-alternative", ALTERNATIVE_PRESET)],
        );

        let windows = game_folder(
            "engine-alternative-windows",
            &["Example.dll", "Example_Data/a.dll"],
        );
        assert!(
            detect(&windows, &presets).is_some(),
            "the Windows build matches"
        );

        let native = game_folder(
            "engine-alternative-native",
            &["Example.so", "Example_Data/a.so"],
        );
        let found = detect(&native, &presets).expect("the native build matches");
        assert_eq!(found.variant.as_deref(), Some("plain"));

        let other = game_folder(
            "engine-alternative-other",
            &["Other.so", "Example_Data/a.so"],
        );
        assert!(
            detect(&other, &presets).is_none(),
            "no alternative of the first marker exists"
        );
    }

    #[test]
    fn a_variant_marker_with_alternatives_chooses_its_variant_on_either_build() {
        let presets = presets_in(
            "engine-alternative-variant-presets",
            &[("fixture-alternative", ALTERNATIVE_PRESET)],
        );
        let native = game_folder(
            "engine-alternative-variant-native",
            &["Example.so", "Example_Data/a", "Extra.so"],
        );
        let found = detect(&native, &presets).expect("the engine matches");
        assert_eq!(found.variant.as_deref(), Some("extra"));
        assert_eq!(found.label.as_deref(), Some("Extra"));
    }

    #[test]
    fn the_project_folder_is_never_an_engine_folder_named_by_any_alternative() {
        const ALTERNATIVE_PROJECT: &str = r#"
schema = 1
id = "fixture-alternative-project"
name = "Fixture Alternative Project"

[deploy]

[[deploy.slots]]
dir = "Mods"

[engine]
name = "Example Project Engine"
markers = ["Engine/Binaries|Engine/Plugins"]
root = "*/Content/Paks"
"#;
        let presets = presets_in(
            "engine-alternative-root-presets",
            &[("fixture-alternative-project", ALTERNATIVE_PROJECT)],
        );
        let folder = game_folder(
            "engine-alternative-root",
            &[
                "Engine/Plugins/engine.dll",
                "Engine/Content/Paks/engine.pak",
                "Project/Content/Paks/mod.pak",
            ],
        );
        let found = detect(&folder, &presets).expect("the engine matches");
        assert_eq!(found.root.as_deref(), Some("Project"));
    }
}
