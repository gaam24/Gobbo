//! The `mods.settings` file: `[folder]` sections with `Enabled` and `Priority`. We change only the sections of mods from the plan.
//! The rest of the file (other sections, order, line endings) stays the same.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::deploy::DeployError;
use crate::domain::resolver::SettingsRow;
use crate::error::FileError;
use crate::infra::store;

/// `None` when the file does not exist. A file that is not UTF-8 is an error, so it is not damaged.
pub fn read(path: &Path) -> Result<Option<String>, DeployError> {
    match fs::read(path) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| DeployError::NotUtf8 {
                path: path.to_path_buf(),
            }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(FileError::new(path, e).into()),
    }
}

/// Written through a temporary file, so an interrupted write does not leave a truncated file.
pub fn write(path: &Path, content: &str) -> Result<(), DeployError> {
    Ok(store::write_atomic(path, content.as_bytes())?)
}

/// A section Gobbo changed in the settings file. Undo puts back only these sections, so changes made to the rest of the
/// file (by the game or by hand) stay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Touched {
    pub folder: String,
    /// The section's lines before the change, without the blank lines after it. `None` when the section did not exist.
    pub before: Option<Vec<String>>,
    /// Whether a blank line was added before a new section, so undo can remove it again.
    pub blank_before: bool,
}

/// Inserts `Enabled` and `Priority` for each row. New mods get a new section at the end. Returns the new text and the
/// sections it changed.
pub fn merge(current: Option<&str>, rows: &[SettingsRow]) -> (String, Vec<Touched>) {
    let text = current.unwrap_or("");
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let mut touched = Vec::new();

    for row in rows {
        upsert(&mut lines, row, &mut touched);
    }

    (join(&lines, eol), touched)
}

/// The text with the sections in `touched` put back as they were. Every other section is left as it is.
pub fn revert(current: &str, touched: &[Touched]) -> String {
    let eol = if current.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines: Vec<String> = current.lines().map(str::to_string).collect();

    for change in touched.iter().rev() {
        match (section(&lines, &change.folder), &change.before) {
            (Some((start, end)), Some(before)) => {
                lines.splice(start..end, before.iter().cloned());
            }
            (Some((start, end)), None) => {
                lines.drain(start..end);
                // The blank line Gobbo added before the section goes too; any blank line the game left stays.
                if change.blank_before && start > 0 && lines[start - 1].trim().is_empty() {
                    lines.remove(start - 1);
                }
            }
            (None, Some(before)) => {
                // The game removed the section; put it back at the end.
                if lines.last().is_some_and(|line| !line.trim().is_empty()) {
                    lines.push(String::new());
                }

                lines.extend(before.iter().cloned());
            }
            (None, None) => {}
        }
    }

    join(&lines, eol)
}

pub(super) fn join(lines: &[String], eol: &str) -> String {
    let mut out = String::new();

    for line in lines {
        out.push_str(line);
        out.push_str(eol);
    }

    out
}

fn upsert(lines: &mut Vec<String>, row: &SettingsRow, touched: &mut Vec<Touched>) {
    let enabled = if row.enabled { "1" } else { "0" };
    let priority = row.priority.to_string();

    if let Some((start, end)) = section(lines, &row.folder) {
        touched.push(Touched {
            folder: row.folder.clone(),
            before: Some(lines[start..end].to_vec()),
            blank_before: false,
        });
        set_key(lines, &row.folder, "Enabled", enabled);
        set_key(lines, &row.folder, "Priority", &priority);
        return;
    }

    let blank_before = lines.last().is_some_and(|line| !line.trim().is_empty());
    touched.push(Touched {
        folder: row.folder.clone(),
        before: None,
        blank_before,
    });

    if blank_before {
        lines.push(String::new());
    }

    lines.push(format!("[{}]", row.folder));
    lines.push(format!("Enabled={enabled}"));
    lines.push(format!("Priority={priority}"));
}

/// Index of the section header and index of the first line after its keys. The blank lines after the keys are not part of
/// the section, so they stay when the section is replaced or removed.
fn section(lines: &[String], folder: &str) -> Option<(usize, usize)> {
    let start = lines
        .iter()
        .position(|line| header(line).is_some_and(|name| name.eq_ignore_ascii_case(folder)))?;
    let mut end = lines[start + 1..]
        .iter()
        .position(|line| header(line).is_some())
        .map_or(lines.len(), |i| start + 1 + i);
    while end > start + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }

    Some((start, end))
}

fn set_key(lines: &mut Vec<String>, folder: &str, key: &str, value: &str) {
    let Some((start, end)) = section(lines, folder) else {
        return;
    };
    let found = (start + 1..end).find(|&i| {
        lines[i]
            .split_once('=')
            .is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
    });

    match found {
        Some(i) => lines[i] = format!("{key}={value}"),
        None => lines.insert(start + 1, format!("{key}={value}")),
    }
}

fn header(line: &str) -> Option<&str> {
    line.trim_start_matches('\u{feff}')
        .trim()
        .strip_prefix('[')?
        .strip_suffix(']')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(folder: &str, enabled: bool, priority: u32) -> SettingsRow {
        SettingsRow {
            folder: folder.into(),
            enabled,
            priority,
        }
    }

    #[test]
    fn keeps_other_sections_and_line_endings() {
        let before =
            "[modOld]\r\nEnabled=1\r\nPriority=7\r\n\r\n[modHoods]\r\nEnabled=0\r\nPriority=2\r\n";
        let (after, _) = merge(
            Some(before),
            &[row("modHoods", true, 1), row("modNew", false, 2)],
        );
        assert_eq!(
            after,
            "[modOld]\r\nEnabled=1\r\nPriority=7\r\n\r\n[modHoods]\r\nEnabled=1\r\nPriority=1\r\n\r\n[modNew]\r\nEnabled=0\r\nPriority=2\r\n"
        );
    }

    #[test]
    fn section_names_match_ignoring_case() {
        let before = "[modhoods]\nEnabled=0\nPriority=9\n";
        let (after, _) = merge(Some(before), &[row("modHoods", true, 1)]);
        assert_eq!(after, "[modhoods]\nEnabled=1\nPriority=1\n");
    }

    #[test]
    fn revert_puts_back_what_merge_changed_and_nothing_else() {
        let before = "[modOld]\nEnabled=1\nPriority=7\n\n[modHoods]\nEnabled=0\nPriority=2\n";
        let (merged, touched) = merge(
            Some(before),
            &[row("modHoods", true, 1), row("modNew", false, 2)],
        );
        assert_eq!(revert(&merged, &touched), before);
    }

    #[test]
    fn revert_keeps_sections_the_game_changed_meanwhile() {
        let before = "[modOld]\nEnabled=1\nPriority=7\n";
        let (merged, touched) = merge(Some(before), &[row("modNew", true, 2)]);
        let meanwhile = format!("{merged}\n[modGame]\nEnabled=1\nPriority=3\n");
        assert_eq!(
            revert(&meanwhile, &touched),
            "[modOld]\nEnabled=1\nPriority=7\n\n[modGame]\nEnabled=1\nPriority=3\n"
        );
    }

    #[test]
    fn revert_of_a_file_that_was_emptied_gives_an_empty_file() {
        let (merged, touched) = merge(None, &[row("modNew", true, 1)]);
        assert_eq!(merged, "[modNew]\nEnabled=1\nPriority=1\n");
        assert_eq!(revert("", &touched), "");
    }

    #[test]
    fn a_removed_new_section_takes_its_added_blank_line_with_it() {
        let merged = "[modA]\nEnabled=1\n\n[modB]\nEnabled=1\n";
        let touched = [Touched {
            folder: "modB".into(),
            before: None,
            blank_before: true,
        }];
        assert_eq!(revert(merged, &touched), "[modA]\nEnabled=1\n");

        // The first section has no line above it, so the guard must not index before the start.
        let first = [Touched {
            folder: "modB".into(),
            before: None,
            blank_before: true,
        }];
        assert_eq!(revert("[modB]\nEnabled=1\n", &first), "");
    }

    #[test]
    fn a_restored_section_gets_a_blank_line_after_a_non_empty_one() {
        let touched = [Touched {
            folder: "modB".into(),
            before: Some(vec!["[modB]".into(), "Enabled=1".into()]),
            blank_before: false,
        }];
        assert_eq!(
            revert("[modA]\nEnabled=1\n", &touched),
            "[modA]\nEnabled=1\n\n[modB]\nEnabled=1\n"
        );
    }

    #[test]
    fn a_section_ends_before_the_next_header_and_its_trailing_blanks() {
        // modA ends before the two blank lines ahead of modB; modB ends right at modC's header. Changing both sections
        // must leave the blanks, the untouched modC section and the CRLF endings in place, and revert restores it all.
        let before = "[modA]\r\nEnabled=0\r\nPriority=1\r\n\r\n\r\n[modB]\r\nEnabled=0\r\nPriority=2\r\n[modC]\r\nEnabled=1\r\nPriority=3\r\n";
        let (merged, touched) = merge(Some(before), &[row("modA", true, 7), row("modB", true, 8)]);
        assert_eq!(
            merged,
            "[modA]\r\nEnabled=1\r\nPriority=7\r\n\r\n\r\n[modB]\r\nEnabled=1\r\nPriority=8\r\n[modC]\r\nEnabled=1\r\nPriority=3\r\n"
        );
        assert_eq!(revert(&merged, &touched), before);
    }
}
