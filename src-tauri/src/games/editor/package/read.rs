//! Reading and verifying a package: parsing the archive, checking the definition and describing what would be written.

use std::fs;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

use crate::error::{AppError, io_at};
use crate::games::definition::GameDef;
use crate::games::editor::DefinitionProblemView;
use crate::games::editor::check::{check_with, problem};
use crate::games::loaders::LoaderDef;
use crate::games::view::GameLoaderView;
use crate::infra::archive::Archive;

use super::{DefinitionPackagePreview, PackageContext, PackageData};

const MAX_GAME_BYTES: u64 = 1024 * 1024;
const MAX_LOADER_BYTES: u64 = 256 * 1024;
const MAX_ICON_BYTES: u64 = 12 * 1024;
const MAX_LOADERS: usize = 32;

pub(super) fn read_package(path: &Path, context: &PackageContext) -> Result<PackageData, AppError> {
    let archive = Archive::open(path)?;
    let (prefix, game_index) =
        find_game(&archive).ok_or_else(|| AppError::user("games.package_empty"))?;
    let game_entry = archive
        .entries()
        .get(game_index)
        .ok_or_else(|| AppError::user("games.package_empty"))?;
    let game_text = read_text(&archive, game_index, game_entry, MAX_GAME_BYTES)?;
    let mut problems = Vec::new();
    let parsed = GameDef::parse(&game_text);
    let game = match &parsed {
        Ok(game) => Some(game),
        Err(error) => {
            problems.push(problem("source", error.to_string()));
            None
        }
    };
    let id = game.map_or_else(String::new, |game| game.id.clone());
    let name = game.map_or_else(String::new, |game| game.name.clone());
    let mut warnings = Vec::new();
    let icon = read_icon(&archive, &prefix, &mut warnings);
    let (loaders, mut loader_problems) = read_loaders(&archive, &prefix);
    problems.append(&mut loader_problems);

    // Validate with the loaders from the package first, then the recipient's user and bundled presets.
    let staged = tempfile::tempdir().map_err(AppError::unexpected)?;

    for (loader_id, text) in &loaders {
        if let Err(error) = fs::write(staged.path().join(format!("{loader_id}.toml")), text) {
            return Err(io_at(staged.path())(error));
        }
    }

    let mut loader_dirs = vec![staged.path().to_path_buf()];
    loader_dirs.push(context.user_loaders.clone());
    loader_dirs.push(context.bundled_loaders.clone());
    let check = check_with(&game_text, &loader_dirs);
    problems.extend(check.problems.iter().cloned());

    let mut files = vec!["game.toml".to_string()];

    if let Some((icon_name, _)) = &icon {
        files.push(icon_name.clone());
    }

    files.extend(
        loaders
            .iter()
            .map(|(loader_id, _)| format!("loaders/{loader_id}.toml")),
    );

    let mut overwrites = Vec::new();
    let game_exists = !id.is_empty() && context.user_games.join(&id).join("game.toml").is_file();

    if game_exists {
        overwrites.push(format!("game {id}"));
    }

    if !id.is_empty() && context.bundled_games.join(&id).join("game.toml").is_file() {
        problems.push(problem(
            "general",
            format!("the id {id} belongs to a bundled game"),
        ));
    }

    for (loader_id, _) in &loaders {
        if context
            .user_loaders
            .join(format!("{loader_id}.toml"))
            .is_file()
        {
            overwrites.push(format!("loader {loader_id}"));
        } else if context
            .bundled_loaders
            .join(format!("{loader_id}.toml"))
            .is_file()
        {
            // A user preset shadows the bundled one for every game. Treat it as an explicit replacement.
            overwrites.push(format!("bundled loader {loader_id}"));
        }
    }

    let loader = loader_of(&check, game, &loaders);
    let preview = DefinitionPackagePreview {
        id,
        name,
        icon: icon.as_ref().map(|(_, bytes)| data_url(bytes)),
        loader,
        files,
        problems,
        warnings,
        overwrites,
    };
    Ok(PackageData {
        preview,
        game_text,
        icon,
        loaders,
    })
}

/// The `game.toml` entry, or the single folder that wraps it. A root file wins over a wrapped one.
fn find_game(archive: &Archive) -> Option<(String, usize)> {
    if let Some(index) = archive
        .entries()
        .iter()
        .position(|entry| !entry.is_dir && entry.name == "game.toml")
    {
        return Some((String::new(), index));
    }

    let mut found = None;

    for (index, entry) in archive.entries().iter().enumerate() {
        if entry.is_dir {
            continue;
        }

        let parts: Vec<&str> = entry.name.split('/').collect();

        if parts.len() == 2 && parts[0] != ".." && !parts[0].is_empty() && parts[1] == "game.toml" {
            if found.is_some() {
                return None;
            }

            found = Some((format!("{}/", parts[0]), index));
        }
    }

    found
}

fn read_text(
    archive: &Archive,
    index: usize,
    entry: &crate::infra::archive::Entry,
    limit: u64,
) -> Result<String, AppError> {
    if entry.size > limit {
        return Err(AppError::user("games.package_too_large").with("file", &entry.name));
    }

    let bytes = archive
        .read_bytes_limited(index, limit)
        .map_err(AppError::from)?;
    String::from_utf8(bytes).map_err(|error| {
        AppError::user("games.package_invalid").with("detail", format!("{}: {error}", entry.name))
    })
}

fn read_icon(
    archive: &Archive,
    prefix: &str,
    warnings: &mut Vec<String>,
) -> Option<(String, Vec<u8>)> {
    for (file_name, mime) in [("icon.webp", "webp"), ("icon.png", "png")] {
        let name = format!("{prefix}{file_name}");
        let Some((index, entry)) = archive
            .entries()
            .iter()
            .enumerate()
            .find(|(_, entry)| !entry.is_dir && entry.name == name)
        else {
            continue;
        };

        if entry.size > MAX_ICON_BYTES {
            warnings.push(format!("{file_name} is larger than 12 KB and was skipped"));
            continue;
        }

        let Ok(bytes) = archive.read_bytes_limited(index, MAX_ICON_BYTES) else {
            warnings.push(format!("{file_name} could not be read and was skipped"));
            continue;
        };

        if !icon_matches(&bytes, mime) {
            warnings.push(format!(
                "{file_name} is not a valid {mime} image and was skipped"
            ));
            continue;
        }

        return Some((file_name.to_string(), bytes));
    }

    None
}

fn icon_matches(bytes: &[u8], mime: &str) -> bool {
    if mime == "png" {
        return bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    }

    bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

fn read_loaders(
    archive: &Archive,
    prefix: &str,
) -> (Vec<(String, String)>, Vec<DefinitionProblemView>) {
    let mut loaders = Vec::new();
    let mut problems = Vec::new();

    for (index, entry) in archive.entries().iter().enumerate() {
        if entry.is_dir || loaders.len() >= MAX_LOADERS {
            continue;
        }

        let Some(rest) = entry.name.strip_prefix(&format!("{prefix}loaders/")) else {
            continue;
        };
        let Some(id) = rest.strip_suffix(".toml") else {
            continue;
        };

        if id.is_empty() || id.contains('/') {
            continue;
        }

        if entry.size > MAX_LOADER_BYTES {
            problems.push(problem(
                "loader",
                format!("loaders/{id}.toml is larger than 256 KB"),
            ));
            continue;
        }

        let Ok(bytes) = archive.read_bytes_limited(index, MAX_LOADER_BYTES) else {
            problems.push(problem(
                "loader",
                format!("loaders/{id}.toml cannot be read"),
            ));
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            problems.push(problem("loader", format!("loaders/{id}.toml is not UTF-8")));
            continue;
        };
        let loader = match LoaderDef::parse(&text) {
            Ok(loader) => loader,
            Err(error) => {
                problems.push(problem(
                    "loader",
                    format!("loaders/{id}.toml is not valid TOML: {error}"),
                ));
                continue;
            }
        };

        if loader.id != id {
            problems.push(problem(
                "loader",
                format!("loaders/{id}.toml names the id {:?}", loader.id),
            ));
            continue;
        }

        let loader_problems = loader.validate();

        if !loader_problems.is_empty() {
            problems.push(problem(
                "loader",
                format!("loaders/{id}.toml: {}", loader_problems.join("; ")),
            ));
            continue;
        }

        loaders.push((id.to_string(), text));
    }

    (loaders, problems)
}

fn loader_of(
    check: &super::DefinitionCheck,
    game: Option<&GameDef>,
    loaders: &[(String, String)],
) -> Option<GameLoaderView> {
    if let Some(loader) = check
        .summary
        .as_ref()
        .and_then(|summary| summary.game.loader.as_ref())
    {
        return Some(GameLoaderView {
            id: loader.id.clone(),
            name: loader.name.clone(),
            variant: loader.variant.clone(),
            default_variant: loader.default_variant,
        });
    }

    let reference = game?.loader.as_ref()?;
    let id = reference.id().to_string();
    let name = loaders
        .iter()
        .find(|(loader_id, _)| loader_id == &id)
        .and_then(|(_, text)| LoaderDef::parse(text).ok())
        .map_or_else(|| id.clone(), |loader| loader.name);
    Some(GameLoaderView {
        id,
        name,
        variant: reference.variant().map(str::to_string),
        default_variant: false,
    })
}

fn data_url(bytes: &[u8]) -> String {
    let mime = if icon_matches(bytes, "png") {
        "image/png"
    } else {
        "image/webp"
    };
    format!("data:{mime};base64,{}", BASE64_STANDARD.encode(bytes))
}
