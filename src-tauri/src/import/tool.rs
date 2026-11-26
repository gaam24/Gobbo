//! Unpacking a tool's archive into the game folder, where the tool's program is found afterwards.

use crate::domain::pathkey::starts_with_ignore_ascii_case;
use crate::error::AppError;
use crate::error::ErrorKind;
use crate::error::bad_archive;
use crate::error::io_at;
use crate::games::{Tool, is_game_relative};
use crate::infra::archive::Archive;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io;
use std::path::Path;

/// Unpacks the archive of a tool into the folder of its program, in the game folder. An archive that holds that folder
/// (`WitcherScriptMerger/...`) gives its files to it, and one that holds only the files gets them as they are. The
/// program must be in the folder afterwards.
pub fn unpack_tool(game_root: &Path, tool: &Tool, archive_path: &Path) -> Result<(), AppError> {
    if game_root.as_os_str().is_empty() {
        return Err(AppError::user("tools.no_game_folder"));
    }

    let program = tool.detect.first().ok_or_else(|| tool.missing_error())?;
    let folder = Path::new(program).parent().unwrap_or_else(|| Path::new(""));
    let target = game_root.join(folder);
    let wrapper = match folder.to_string_lossy().replace('\\', "/") {
        name if name.is_empty() => String::new(),
        name => format!("{name}/"),
    };
    let archive = Archive::open(archive_path)?;
    let all_wrapped = archive
        .entries()
        .iter()
        .all(|entry| entry.is_dir || starts_with_ignore_ascii_case(&entry.name, &wrapper));
    let strip = if all_wrapped { wrapper.len() } else { 0 };
    let wanted: BTreeSet<usize> = archive
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, entry)| !entry.is_dir)
        .map(|(index, _)| index)
        .collect();
    archive.visit::<AppError>(&wanted, |index, reader| {
        let name = &archive.entries()[index].name;
        // The prefix is ASCII and ends with `/`, so the cut is on a character boundary.
        let relative = &name[strip..];

        if !is_game_relative(relative)
            || !crate::domain::naming::is_safe_relative_path(
                relative,
                crate::platform::is_windows(),
            )
        {
            return Err(bad_archive(format!("unsafe path in the archive: {name}")));
        }

        let file = target.join(relative);

        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).map_err(io_at(parent))?;
        }

        let mut out = File::create(&file).map_err(io_at(&file))?;
        io::copy(reader, &mut out).map_err(io_at(&file))?;
        Ok(())
    })?;

    if !game_root.join(program).is_file() {
        return Err(AppError::new("tools.archive_missing_program")
            .with("program", program)
            .kind(ErrorKind::Permanent));
    }

    Ok(())
}
