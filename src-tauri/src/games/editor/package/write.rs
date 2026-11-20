//! Writing the files of a verified package into the user definition folders.

use std::fs;
use std::path::Path;

use crate::error::{AppError, io_at};
use crate::games::{forget_all_definitions, forget_definition, is_valid_id};
use crate::infra::store;

use super::read::read_package;
use super::{ImportedDefinitionPackage, PackageContext};

pub(super) fn import_with(
    path: &Path,
    overwrite: bool,
    context: &PackageContext,
) -> Result<ImportedDefinitionPackage, AppError> {
    let data = read_package(path, context)?;

    if !data.preview.problems.is_empty() {
        let detail = data
            .preview
            .problems
            .iter()
            .map(|problem| problem.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(AppError::user("games.package_invalid").with("detail", detail));
    }

    if !data.preview.overwrites.is_empty() && !overwrite {
        let detail = data.preview.overwrites.join(", ");
        return Err(AppError::user("games.package_exists").with("detail", detail));
    }

    if !is_valid_id(&data.preview.id) {
        return Err(
            AppError::user("games.package_invalid").with("detail", "the game id is not valid")
        );
    }

    // Loaders first: the game is the file the catalog reads, so it is written last.
    fs::create_dir_all(&context.user_loaders).map_err(io_at(&context.user_loaders))?;

    for (loader_id, text) in &data.loaders {
        let target = context.user_loaders.join(format!("{loader_id}.toml"));
        store::write_atomic(&target, text.as_bytes())?;
    }

    let game_folder = context.user_games.join(&data.preview.id);
    fs::create_dir_all(&game_folder).map_err(io_at(&game_folder))?;
    let game_path = game_folder.join("game.toml");
    store::write_atomic(&game_path, data.game_text.as_bytes())?;

    if let Some((icon_name, bytes)) = &data.icon {
        let icon = game_folder.join(icon_name);
        store::write_atomic(&icon, bytes)?;

        for other in ["icon.webp", "icon.png"] {
            if other != icon_name {
                let _ = fs::remove_file(game_folder.join(other));
            }
        }
    } else if overwrite {
        for other in ["icon.webp", "icon.png"] {
            let _ = fs::remove_file(game_folder.join(other));
        }
    }

    if data.loaders.is_empty() {
        forget_definition(&game_path);
    } else {
        forget_all_definitions();
    }

    Ok(ImportedDefinitionPackage {
        id: data.preview.id,
        name: data.preview.name,
    })
}
