//! Commands of the import module that do not belong to one game: the dry run of a draft definition against an
//! archive. The editor calls it before a new game exists, so it cannot use the `game` of the usual command macro.

use std::path::Path;

use crate::commands::helpers::blocking;
use crate::error::AppError;

use super::preview::{self, DefinitionPreview};

/// Places the files of an archive with a draft definition and writes nothing. The window shows the result in the
/// editor, so a new definition can be checked before it is saved.
#[tauri::command]
#[specta::specta]
pub async fn preview_definition(text: String, path: String) -> Result<DefinitionPreview, AppError> {
    blocking(move || preview::preview(&text, Path::new(&path))).await
}
