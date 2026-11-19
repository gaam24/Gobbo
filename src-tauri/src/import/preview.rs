//! A dry run of a draft definition against a real archive: the same placement rules as an import, with nothing
//! extracted and nothing written to the library. The editor shows the result so a user can see whether a new game
//! definition understands the mods they have before saving it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use specta::Type;

use crate::error::AppError;
use crate::import::archive_base_name;
use crate::infra::archive::Archive;

use super::install;

/// Where a draft definition would put the files of one archive.
#[derive(Debug, Serialize, Type)]
pub struct DefinitionPreview {
    pub mods: Vec<PreviewMod>,
    /// Files no rule took, for example a readme at the archive root.
    pub skipped: u32,
    /// The first skipped names, so the window can explain what was ignored.
    pub skipped_examples: Vec<String>,
}

/// One mod the archive would become.
#[derive(Debug, Serialize, Type)]
pub struct PreviewMod {
    /// The folder in the archive the files belong to, or the archive name for the files that belong to it as a whole.
    pub name: String,
    /// The slot folder, empty for files that keep a game path.
    pub slot: String,
    pub files: u32,
    /// A few placed files, from the archive path to the path under the deployment root.
    pub examples: Vec<PreviewFile>,
}

/// One placed file.
#[derive(Debug, Serialize, Type)]
pub struct PreviewFile {
    pub from: String,
    pub to: String,
}

/// Reads the archive and places its files with the rules of `text`. The definition must be valid; an invalid one is
/// refused with the same detail the editor shows.
pub fn preview(text: &str, path: &Path) -> Result<DefinitionPreview, AppError> {
    let game = crate::games::editor::merged(text)?;
    let archive = Archive::open(path)?;
    let base = archive_base_name(&path.to_string_lossy());
    let plan = install::plan(&archive, &game, &base, None)?;
    let mut mods: BTreeMap<String, PreviewMod> = BTreeMap::new();
    for item in &plan.placed {
        let entry = mods
            .entry(item.group.clone())
            .or_insert_with(|| PreviewMod {
                name: item.group.clone(),
                slot: item.slot.clone(),
                files: 0,
                examples: Vec::new(),
            });
        entry.files += 1;
        if entry.examples.len() < 12 {
            let from = archive
                .entries()
                .get(item.index)
                .map_or_else(String::new, |entry| entry.name.clone());
            let to = if item.slot.is_empty() {
                item.rel.clone()
            } else {
                format!("{}/{}", item.slot, item.rel)
            };
            entry.examples.push(PreviewFile { from, to });
        }
    }
    let skipped_examples = plan
        .skipped_files
        .iter()
        .filter(|file| file.reason == crate::domain::library::SkipReason::NoRule)
        .map(|file| file.path.clone())
        .take(8)
        .collect();
    Ok(DefinitionPreview {
        mods: mods.into_values().collect(),
        skipped: plan.skipped as u32,
        skipped_examples,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::games::SLOT;
    use crate::test_support::{make_zip, scratch};

    #[test]
    fn a_plan_shows_the_mods_the_files_and_the_skipped_entries() {
        let base = scratch!("definition-preview");
        let archive = make_zip(
            &base,
            "Example.zip",
            &[
                ("Mods/Alpha/content/a.txt", "a"),
                ("Mods/Alpha/content/b.txt", "b"),
                ("Mods/Beta/content/c.txt", "c"),
                ("readme.txt", "read me"),
            ],
        );
        let preview = preview(SLOT, &archive).expect("a valid preview");
        assert_eq!(preview.mods.len(), 2);
        assert_eq!(preview.skipped, 1);
        assert_eq!(preview.skipped_examples, ["readme.txt"]);
        let alpha = preview
            .mods
            .iter()
            .find(|item| item.name == "Alpha")
            .expect("the Alpha mod");
        assert_eq!(alpha.slot, "Mods");
        assert_eq!(alpha.files, 2);
        assert!(
            alpha
                .examples
                .iter()
                .any(|file| file.to == "Mods/content/a.txt")
        );
    }
}
