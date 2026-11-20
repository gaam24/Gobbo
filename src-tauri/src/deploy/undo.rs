//! Undo: the journal's entries are reverted in reverse order, and the settings sections Gobbo wrote are put back.

use std::fs;

use std::io;

use std::path::Path;

use crate::error::{FileError, Message};

use super::journal::{Entry, Journal, JournalRoots};
use super::{DeployError, overlay, settings};
use crate::context::data::JOURNAL_FILE;

/// Undoes the deployment recorded in the journal and returns warnings (for example a folder where someone added their own files).
/// Only the settings sections Gobbo wrote are put back, so changes made to the rest of the file stay. Every entry is
/// checked against the roots first: a corrupted or hand-edited journal must not delete or write outside them.
pub fn undo(roots: &JournalRoots) -> Result<Vec<Message>, DeployError> {
    let journal = Journal::new(roots.state.join(JOURNAL_FILE));
    let entries = journal.read()?;

    if entries.is_empty() {
        return Err(DeployError::NothingToUndo);
    }
    // Check every entry before the first change, so an unsafe line cannot remove the valid entries before it either.
    if let Some(path) = entries.iter().find_map(|entry| roots.unsafe_path(entry)) {
        return Err(DeployError::JournalUnsafe {
            path: path.to_path_buf(),
        });
    }

    // Reverse order: the file disappears before the folder that contains it. One entry that the disk no longer
    // matches (a file replaced by a folder, a locked file) must not leave the whole deployment stuck: record it, keep
    // going, and retire the journal so the next apply can start clean.
    let mut warnings = Vec::new();

    for entry in entries.iter().rev() {
        match entry {
            Entry::FileDeployed { path } => {
                if let Err(error) = remove_deployed_file(path, &mut warnings) {
                    warnings.push(entry_failed(path, error));
                }
            }
            Entry::DirCreated { path } => {
                if let Err(error) = remove_created_dir(path, &mut warnings) {
                    warnings.push(entry_failed(path, error));
                }
            }
            Entry::LayerPrepared { overwrite, .. } => overlay::finish(overwrite, &mut warnings),
            Entry::SettingsSections { .. } => {}
        }
    }

    if let Err(error) = undo_settings(&entries) {
        warnings.push(Message::new("undo.settings_failed").with("detail", error));
    }

    journal.remove()?;
    Ok(warnings)
}

fn entry_failed(path: &Path, error: impl std::fmt::Display) -> Message {
    Message::new("undo.entry_failed")
        .with("path", path.display())
        .with("detail", error)
}

/// Puts back the settings sections the journal recorded.
fn undo_settings(entries: &[Entry]) -> Result<(), DeployError> {
    let sections = entries.iter().find_map(|entry| match entry {
        Entry::SettingsSections {
            path,
            existed,
            touched,
        } => Some((path, *existed, touched)),
        _ => None,
    });

    if let Some((path, existed, touched)) = sections {
        revert_sections(path, existed, touched)?;
    }

    Ok(())
}

/// Puts back the sections Gobbo changed. A file that did not exist before is removed when nothing else is left in it.
/// A file that is gone now has nothing of Gobbo's left, so it is not created again.
fn revert_sections(
    path: &Path,
    existed: bool,
    touched: &[settings::Touched],
) -> Result<(), DeployError> {
    let Some(now) = settings::read(path)? else {
        return Ok(());
    };

    if touched.is_empty() {
        return Ok(());
    }

    let text = settings::revert(&now, touched);

    if !existed && text.is_empty() {
        return crate::infra::fs::remove_file_if_exists(path)
            .map_err(|error| FileError::new(path, error).into());
    }

    if text == now {
        return Ok(());
    }

    settings::write(path, &text)
}

/// Removes one file the journal recorded. A file that is already gone is not an error. A folder where a deployed file
/// used to be belongs to the user now; it is reported and left alone rather than removed recursively.
fn remove_deployed_file(path: &Path, warnings: &mut Vec<Message>) -> Result<(), DeployError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => {
            warnings.push(Message::new("undo.file_is_folder").with("path", path.display()));
            Ok(())
        }
        Ok(_) => crate::infra::fs::remove_file_if_exists(path)
            .map_err(|error| FileError::new(path, error).into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(FileError::new(path, error).into()),
    }
}

/// Removes one folder the journal created. A file where the folder used to be is left alone; it is not Gobbo's.
fn remove_created_dir(path: &Path, warnings: &mut Vec<Message>) -> Result<(), DeployError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => remove_dir_if_empty(path, warnings),
        Ok(_) => {
            warnings.push(Message::new("undo.folder_is_file").with("path", path.display()));
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(FileError::new(path, error).into()),
    }
}

/// Removes only an empty folder. If someone added their own files there, it leaves the folder and adds a warning.
pub(super) fn remove_dir_if_empty(
    path: &Path,
    warnings: &mut Vec<Message>,
) -> Result<(), DeployError> {
    match crate::infra::fs::ignore_not_found(fs::remove_dir(path)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::DirectoryNotEmpty => {
            warnings.push(Message::new("undo.dir_not_empty").with("path", path.display()));
            Ok(())
        }
        Err(e) => Err(FileError::new(path, e).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::scratch;

    fn roots(base: &Path) -> JournalRoots {
        JournalRoots::new(
            base.join("state"),
            base.join("game"),
            Some(base.join("settings/mods.settings")),
        )
    }

    #[test]
    fn a_valid_journal_is_undone() {
        let base = scratch!("undo-valid");
        let dir = base.join("game/Mods/modA");
        let file = dir.join("x.ws");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(base.join("state")).unwrap();
        fs::write(&file, "x").unwrap();
        let journal = Journal::new(base.join("state").join(JOURNAL_FILE));
        journal
            .append(&Entry::DirCreated { path: dir.clone() })
            .unwrap();
        journal
            .append(&Entry::FileDeployed { path: file.clone() })
            .unwrap();

        let warnings = undo(&roots(&base)).unwrap();
        assert!(warnings.is_empty());
        assert!(!file.exists());
        assert!(!dir.exists());
        assert!(!base.join("state").join(JOURNAL_FILE).exists());
    }

    #[test]
    fn a_valid_overlay_step_journal_is_undone() {
        let base = scratch!("undo-overlay-step");
        let state = base.join("state");
        let mods = state.join("mods");
        let overwrite = state.join("overwrite");
        let output = overwrite.join("cache/final.bin");
        fs::create_dir_all(&mods).unwrap();
        fs::create_dir_all(output.parent().unwrap()).unwrap();
        fs::write(&output, "generated").unwrap();
        let journal = Journal::new(state.join(JOURNAL_FILE));
        journal
            .append(&Entry::LayerPrepared {
                mods: mods.clone(),
                overwrite: overwrite.clone(),
            })
            .unwrap();
        journal
            .append(&Entry::FileDeployed {
                path: output.clone(),
            })
            .unwrap();

        let _warnings = undo(&roots(&base)).unwrap();
        assert!(!output.exists(), "the step output is removed");
        assert!(mods.is_dir(), "the mod layer stays for the next apply");
        assert!(overwrite.is_dir(), "the overlay writes folder stays");
        assert!(!state.join(JOURNAL_FILE).exists());
    }

    #[test]
    fn missing_entries_are_not_errors() {
        let base = scratch!("undo-missing");
        let path = base.join("missing");
        let mut warnings = Vec::new();
        remove_deployed_file(&path, &mut warnings).unwrap();
        remove_created_dir(&path, &mut warnings).unwrap();
        remove_dir_if_empty(&path, &mut warnings).unwrap();
        crate::infra::fs::remove_file_if_exists(&path).unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_non_empty_folder_is_left_with_a_warning() {
        let base = scratch!("undo-non-empty");
        let dir = base.join("game/Mods/modA");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("user.txt"), "user").unwrap();

        let mut warnings = Vec::new();
        remove_dir_if_empty(&dir, &mut warnings).unwrap();
        assert!(dir.is_dir());
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "undo.dir_not_empty");
    }

    #[test]
    fn a_settings_file_is_removed_only_when_it_did_not_exist_before_the_apply() {
        let base = scratch!("undo-settings-existence");
        let path = base.join("settings/mods.settings");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "[modA]\nEnabled=1\nPriority=1\n").unwrap();
        let touched = [settings::Touched {
            folder: "modA".into(),
            before: None,
            blank_before: false,
        }];

        revert_sections(&path, false, &touched).unwrap();
        assert!(!path.exists(), "a file Gobbo created is removed");

        fs::write(&path, "[modA]\nEnabled=1\nPriority=1\n").unwrap();
        revert_sections(&path, true, &touched).unwrap();
        assert!(path.is_file(), "a file that existed before stays");
    }

    #[test]
    fn an_unsafe_entry_stops_undo_before_any_file_changes() {
        let base = scratch!("undo-unsafe");
        let state = base.join("state");
        let inside = base.join("game/Mods/modA/x.ws");
        let outside = base.join("outside.txt");
        fs::create_dir_all(&state).unwrap();
        fs::create_dir_all(inside.parent().unwrap()).unwrap();
        fs::write(&inside, "deployed").unwrap();
        fs::write(&outside, "user file").unwrap();
        let journal = Journal::new(state.join(JOURNAL_FILE));
        journal
            .append(&Entry::FileDeployed {
                path: inside.clone(),
            })
            .unwrap();
        journal
            .append(&Entry::FileDeployed {
                path: outside.clone(),
            })
            .unwrap();

        let error = undo(&roots(&base)).unwrap_err();
        assert!(
            matches!(&error, DeployError::JournalUnsafe { path } if path == &outside),
            "{error}"
        );
        assert!(inside.is_file(), "the valid entry is not touched first");
        assert!(outside.is_file(), "the unsafe path is not touched");
        assert!(
            state.join(JOURNAL_FILE).is_file(),
            "the journal is kept for diagnosis"
        );
    }
}
