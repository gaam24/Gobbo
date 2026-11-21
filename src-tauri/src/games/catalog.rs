use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::Serialize;

use crate::error::{AppError, ErrorKind, FileError, io_at};

use super::{
    GameDef, GameView, catalog_games_dir, definition_path, games_dir, is_valid_id, loaders_dir,
    read_cached, user_games_dir, user_loaders_dir,
};

/// Where a game definition came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum GameOrigin {
    /// Shipped with Gobbo in `games/`.
    Bundled,
    /// Downloaded from the official catalog into `<data>/catalog/games/`. Read-only, like a bundled definition.
    Official,
    /// Added by the user under `<data>/definitions/games/`.
    User,
}

/// A user definition that could not be loaded. The catalog keeps the other games; the Games page shows these paths.
#[derive(Debug, Serialize, specta::Type)]
pub struct DefinitionProblem {
    pub path: String,
    pub error: AppError,
}

#[derive(Debug, Serialize, specta::Type)]
pub struct GameCatalog {
    pub games: Vec<GameView>,
    /// Invalid or duplicate user definitions, with the reason and the file to fix.
    pub problems: Vec<DefinitionProblem>,
}

/// Every definition from the official, bundled and user folders, with invalid or duplicate definitions reported
/// instead of hiding the rest. The files are read on each call, so a new or edited definition appears without a
/// restart. An official file shadows a bundled file with the same id; a user file never replaces either.
type Definitions = (Vec<(GameDef, GameOrigin)>, Vec<DefinitionProblem>);

pub fn definitions() -> Result<Definitions, AppError> {
    definitions_from(
        games_dir(),
        catalog_games_dir().as_deref(),
        user_games_dir().as_deref(),
        loaders_dir(),
    )
}

pub fn definitions_from(
    shipped: &Path,
    catalog: Option<&Path>,
    user: Option<&Path>,
    loaders: &Path,
) -> Result<Definitions, AppError> {
    let mut games: Vec<(GameDef, GameOrigin)> = Vec::new();
    let mut problems = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut dirs = Vec::with_capacity(3);

    if let Some(catalog) = catalog {
        dirs.push((catalog.to_path_buf(), GameOrigin::Official));
    }

    dirs.push((shipped.to_path_buf(), GameOrigin::Bundled));

    if let Some(user) = user {
        dirs.push((user.to_path_buf(), GameOrigin::User));
    }

    for (dir, origin) in dirs {
        // A downloaded catalog file and a user file can both be fixed by the user; a broken bundled file is a bug.
        let fixable = matches!(origin, GameOrigin::Official | GameOrigin::User);
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io_at(&dir)(error)),
        };

        for entry in entries {
            let entry = entry.map_err(io_at(&dir))?;

            if entry.file_name().to_string_lossy().starts_with('_') {
                continue;
            }

            let path = entry.path().join("game.toml");

            if !path.is_file() {
                continue;
            }

            match read_cached(&path, loaders) {
                Ok(game) => {
                    let folder = entry.file_name().to_string_lossy().into_owned();

                    if fixable && game.id != folder {
                        problems.push(DefinitionProblem {
                            path: path.display().to_string(),
                            error: super::invalid_user_definition(format!(
                                "the folder {folder:?} does not have the game id {:?}",
                                game.id
                            ))
                            .with("path", path.display()),
                        });
                        continue;
                    }

                    if !seen.insert(game.id.clone()) {
                        // A user file with an official or bundled id can be fixed; the bundled copy an official file
                        // shadows is the designed layering, not a problem to report.
                        if origin == GameOrigin::User {
                            problems.push(DefinitionProblem {
                                path: path.display().to_string(),
                                error: AppError::user("games.duplicate_definition")
                                    .with("id", &game.id)
                                    .with("path", path.display()),
                            });
                        }
                        continue;
                    }

                    games.push(((*game).clone(), origin));
                }
                Err(error) => {
                    // The user can re-download a bad official file or fix a user one; a broken shipped file is a bug.
                    let error = if fixable && error.kind == ErrorKind::Bug {
                        error.kind(ErrorKind::UserAction)
                    } else {
                        error
                    };
                    problems.push(DefinitionProblem {
                        path: path.display().to_string(),
                        error,
                    });
                }
            }
        }
    }

    games.sort_by(|left, right| left.0.id.cmp(&right.0.id));
    Ok((games, problems))
}

/// The catalog in the form the window gets: views with their origin and test evidence, and the user files that failed.
pub fn catalog() -> Result<GameCatalog, AppError> {
    let (games, problems) = definitions()?;
    Ok(GameCatalog {
        games: games
            .iter()
            .map(|(game, origin)| game.view_with_origin(*origin))
            .collect(),
        problems,
    })
}

/// The game icon as a data URL for `<img src>`. WebP is preferred; PNG is accepted for older definitions.
/// `None` when the game folder has neither file.
pub fn icon(id: &str) -> Result<Option<String>, AppError> {
    let Some((path, mime)) = icon_file(id) else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(|error| FileError::new(&path, error))?;
    Ok(Some(format!(
        "data:{mime};base64,{}",
        BASE64_STANDARD.encode(bytes)
    )))
}

/// The first icon file of a game: user, then official catalog, then bundled.
pub fn icon_file(id: &str) -> Option<(PathBuf, &'static str)> {
    icon_file_from(
        user_games_dir().as_deref(),
        catalog_games_dir().as_deref(),
        games_dir(),
        id,
    )
}

/// The first icon file of a game in the explicit user, official, bundled folders. The caller owns the precedence, so
/// a test can pass scratch folders without touching the process-wide definition roots.
pub fn icon_file_from(
    user: Option<&Path>,
    catalog: Option<&Path>,
    bundled: &Path,
    id: &str,
) -> Option<(PathBuf, &'static str)> {
    if !is_valid_id(id) {
        return None;
    }

    let mut dirs = Vec::with_capacity(3);

    for dir in [user, catalog].into_iter().flatten() {
        dirs.push(dir.join(id));
    }

    dirs.push(bundled.join(id));

    for dir in dirs {
        for (name, mime) in [("icon.webp", "image/webp"), ("icon.png", "image/png")] {
            let path = dir.join(name);

            if path.is_file() {
                return Some((path, mime));
            }
        }
    }

    None
}

/// Writes a shareable zip with the definition, its icon and a user loader it names. A user can send the zip or unpack
/// it into another Gobbo's `definitions/` folder, or import it from the Games page.
pub fn export(id: &str, path: &Path) -> Result<(), AppError> {
    let definition = definition_path(id);

    if !definition.is_file() {
        return Err(AppError::user("games.unknown").with("id", id));
    }

    let text = std::fs::read(&definition).map_err(io_at(&definition))?;
    let icon = icon_file(id).map(|(icon, _)| icon);
    let loader = user_loader_for(&text);
    write_export(
        &definition,
        icon.as_deref(),
        loader
            .as_ref()
            .map(|(id, loader)| (id.as_str(), loader.as_path())),
        path,
    )
}

/// The user loader preset a definition names, when it has one. A bundled preset is not included: the receiving Gobbo
/// already ships it.
fn user_loader_for(definition: &[u8]) -> Option<(String, PathBuf)> {
    let text = std::str::from_utf8(definition).ok()?;
    let game = GameDef::parse(text).ok()?;
    let loader = game.loader.as_ref()?.id().to_string();

    if !is_valid_id(&loader) {
        return None;
    }

    let path = user_loaders_dir()?.join(format!("{loader}.toml"));
    path.is_file().then_some((loader, path))
}

pub fn write_export(
    definition: &Path,
    icon: Option<&Path>,
    loader: Option<(&str, &Path)>,
    path: &Path,
) -> Result<(), AppError> {
    let file = std::fs::File::create(path).map_err(io_at(path))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let text = std::fs::read(definition).map_err(io_at(definition))?;
    zip.start_file("game.toml", options)
        .map_err(AppError::unexpected)?;
    zip.write_all(&text).map_err(io_at(path))?;

    if let Some(icon) = icon {
        let name = icon.file_name().map_or_else(
            || "icon.webp".to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let bytes = std::fs::read(icon).map_err(io_at(icon))?;
        zip.start_file(&name, options)
            .map_err(AppError::unexpected)?;
        zip.write_all(&bytes).map_err(io_at(path))?;
    }

    if let Some((loader_id, loader)) = loader {
        let bytes = std::fs::read(loader).map_err(io_at(loader))?;
        zip.start_file(format!("loaders/{loader_id}.toml"), options)
            .map_err(AppError::unexpected)?;
        zip.write_all(&bytes).map_err(io_at(path))?;
    }

    zip.finish().map_err(AppError::unexpected)?;
    Ok(())
}
