//! The loader presets a game folder seems to have installed, scored from the rules the presets carry.

use std::path::Path;

use super::super::{LoaderPresetView, LoaderSuggestion};

pub(super) fn detected_loaders(
    folder: &Path,
    presets: &[LoaderPresetView],
) -> Vec<LoaderSuggestion> {
    let mut found = Vec::new();

    for preset in presets {
        if !preset.problems.is_empty() {
            continue;
        }

        let best = if preset.variants.is_empty() {
            score(folder, &preset.requires, &preset.places, &preset.root_files)
                .map(|points| (None, points))
        } else {
            preset
                .variants
                .iter()
                .map(|variant| {
                    (
                        Some(variant.name.clone()),
                        score(
                            folder,
                            &variant.requires,
                            &variant.places,
                            &variant.root_files,
                        )
                        .unwrap_or(0),
                    )
                })
                .max_by_key(|(_, points)| *points)
        };

        if let Some((variant, points)) = best
            && points > 0
        {
            found.push(LoaderSuggestion {
                id: preset.id.clone(),
                name: preset.name.clone(),
                variant,
            });
        }
    }

    found
}

/// How much of a loader's layout is in the folder. A required file is a stronger sign than a folder, because a game
/// may ship an empty `BepInEx` folder after an uninstall.
fn score(
    folder: &Path,
    requires: &[String],
    places: &[String],
    root_files: &[String],
) -> Option<u32> {
    let mut points = 0;

    for path in requires {
        if folder.join(path).exists() {
            points += 2;
        }
    }

    for path in places {
        if folder.join(path).is_dir() {
            points += 1;
        }
    }

    for path in root_files {
        if folder.join(path).exists() {
            points += 1;
        }
    }

    (points > 0).then_some(points)
}
