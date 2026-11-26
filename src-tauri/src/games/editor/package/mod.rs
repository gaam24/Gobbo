//! A shareable definition package: one `game.toml`, its optional icon and the user loader presets it names, in one
//! zip (or another archive the reader understands). The same reader serves the preview dialog and the import, so what
//! the window shows is what is written.

mod read;
mod write;

use std::path::{Path, PathBuf};

use serde::Serialize;
use specta::Type;

use super::super::GameOrigin;
use super::super::layers::DefinitionLayers;
use super::super::view::GameLoaderView;
use super::DefinitionCheck;
use super::DefinitionProblemView;
use crate::error::{AppError, ErrorKind};

use read::read_package;
use write::import_with;

/// What the import dialog shows before anything is written.
#[derive(Debug, Serialize, Type)]
pub struct DefinitionPackagePreview {
    pub id: String,
    pub name: String,
    /// The icon as a data URL, when the package has a readable one.
    pub icon: Option<String>,
    /// The loader the game names, when it resolves.
    pub loader: Option<GameLoaderView>,
    /// The files the package would write, relative to the two definition folders.
    pub files: Vec<String>,
    /// Problems that stop the import.
    pub problems: Vec<DefinitionProblemView>,
    /// Things the import accepts but the user should know, for example a skipped icon.
    pub warnings: Vec<String>,
    /// Existing user files the import would replace. Empty for a new definition.
    pub overwrites: Vec<String>,
}

/// A definition package that was written.
#[derive(Debug, Serialize, Type)]
pub struct ImportedDefinitionPackage {
    pub id: String,
    pub name: String,
}

/// The folders a package reads and writes. Explicit so the tests do not depend on the process-wide data root.
struct PackageContext {
    user_games: PathBuf,
    user_loaders: PathBuf,
    /// Absent when the app never set a data root: there is no downloaded catalog to compare against.
    catalog_games: Option<PathBuf>,
    catalog_loaders: Option<PathBuf>,
    bundled_games: PathBuf,
    bundled_loaders: PathBuf,
}

/// One archive member that the package reader found.
struct PackageData {
    preview: DefinitionPackagePreview,
    game_text: String,
    /// The icon file name and bytes, when the package has a valid one.
    icon: Option<(String, Vec<u8>)>,
    /// Loader id and text, in archive order.
    loaders: Vec<(String, String)>,
}

/// Reads a package for the preview. A file that is not a definition package is reported with the stable
/// `games.package_empty` code, so a dropped mod archive can fall back to the normal import.
pub fn inspect_package(path: &Path) -> Result<DefinitionPackagePreview, AppError> {
    let context = package_context()?;
    Ok(read_package(path, &context)?.preview)
}

/// Writes a valid package. A package that names an existing user file needs `overwrite`.
pub fn import_package(path: &Path, overwrite: bool) -> Result<ImportedDefinitionPackage, AppError> {
    let context = package_context()?;
    import_with(path, overwrite, &context)
}

fn package_context() -> Result<PackageContext, AppError> {
    let layers = DefinitionLayers::current();
    let unavailable = || AppError::new("games.user_definitions_unavailable").kind(ErrorKind::Bug);
    Ok(PackageContext {
        user_games: layers
            .game_path(GameOrigin::User)
            .map(Path::to_path_buf)
            .ok_or_else(unavailable)?,
        user_loaders: layers
            .loader_path(GameOrigin::User)
            .map(Path::to_path_buf)
            .ok_or_else(unavailable)?,
        catalog_games: layers
            .game_path(GameOrigin::Official)
            .map(Path::to_path_buf),
        catalog_loaders: layers
            .loader_path(GameOrigin::Official)
            .map(Path::to_path_buf),
        bundled_games: layers
            .game_path(GameOrigin::Bundled)
            .map_or_else(PathBuf::new, Path::to_path_buf),
        bundled_loaders: layers
            .loader_path(GameOrigin::Bundled)
            .map_or_else(PathBuf::new, Path::to_path_buf),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::archive::Archive;
    use crate::test_support::games::SLOT;
    use crate::test_support::{make_zip, scratch, write};

    fn context(base: &Path) -> PackageContext {
        PackageContext {
            user_games: base.join("user/games"),
            user_loaders: base.join("user/loaders"),
            catalog_games: Some(base.join("catalog/games")),
            catalog_loaders: Some(base.join("catalog/loaders")),
            bundled_games: base.join("bundled/games"),
            bundled_loaders: base.join("bundled/loaders"),
        }
    }

    #[test]
    fn a_package_with_a_game_an_icon_and_a_loader_imports() {
        let base = scratch!("definition-package");
        let ctx = context(&base);
        let game = SLOT
            .replace("id = \"slot\"", "id = \"packed\"")
            .replace("name = \"Slot Game\"", "name = \"Packed Game\"")
            .replace(
                "providers = { nexus = { key = \"slot\" } }",
                "loader = \"fixture-loader\"\nproviders = { nexus = { key = \"packed\" } }",
            );
        let loader = r#"
schema = 1
id = "fixture-loader"
name = "Fixture Loader"

[deploy]
[[deploy.slots]]
dir = "Mods"
"#;
        let archive = make_zip(
            &base,
            "packed.zip",
            &[
                ("game.toml", &game),
                ("icon.webp", "RIFF0000WEBP"),
                ("loaders/fixture-loader.toml", loader),
            ],
        );

        let preview = read_package(&archive, &ctx)
            .expect("a readable package")
            .preview;
        assert!(preview.problems.is_empty(), "{:?}", preview.problems);
        assert_eq!(preview.id, "packed");
        assert_eq!(preview.loader.expect("a loader").id, "fixture-loader");
        assert!(
            preview
                .files
                .contains(&"loaders/fixture-loader.toml".to_string())
        );

        let imported = import_with(&archive, false, &ctx).expect("the import");
        assert_eq!(imported.id, "packed");
        assert!(ctx.user_games.join("packed/game.toml").is_file());
        assert!(ctx.user_loaders.join("fixture-loader.toml").is_file());
        assert!(ctx.user_games.join("packed/icon.webp").is_file());
    }

    #[test]
    fn a_wrapped_package_is_found_and_a_missing_game_is_not_a_package() {
        let base = scratch!("definition-package-wrapper");
        let ctx = context(&base);
        let wrapped = make_zip(&base, "wrapped.zip", &[("Packed/game.toml", SLOT)]);
        let preview = read_package(&wrapped, &ctx)
            .expect("a wrapped package")
            .preview;
        assert_eq!(preview.id, "slot");

        let mod_zip = make_zip(&base, "mod.zip", &[("Mods/Mod/content/a.txt", "a")]);
        let error = inspect_package_with(&mod_zip, &ctx).expect_err("a mod is not a package");
        assert_eq!(error.code, "games.package_empty");
    }

    #[test]
    fn an_existing_user_definition_needs_overwrite() {
        let base = scratch!("definition-package-overwrite");
        let ctx = context(&base);
        write(&ctx.user_games.join("slot/game.toml"), SLOT);
        let archive = make_zip(&base, "slot.zip", &[("game.toml", SLOT)]);
        let preview = read_package(&archive, &ctx)
            .expect("a readable package")
            .preview;
        assert!(!preview.overwrites.is_empty());
        let error = import_with(&archive, false, &ctx).expect_err("overwrite is required");
        assert_eq!(error.code, "games.package_exists");
        import_with(&archive, true, &ctx).expect("overwrite");
    }

    fn inspect_package_with(
        path: &Path,
        ctx: &PackageContext,
    ) -> Result<DefinitionPackagePreview, AppError> {
        Ok(read_package(path, ctx)?.preview)
    }

    #[test]
    fn a_package_cannot_replace_an_official_game() {
        let base = scratch!("definition-package-official");
        let ctx = context(&base);
        let catalog = ctx.catalog_games.as_ref().expect("a catalog folder");
        write(&catalog.join("slot/game.toml"), SLOT);
        let archive = make_zip(&base, "slot.zip", &[("game.toml", SLOT)]);

        let preview = read_package(&archive, &ctx)
            .expect("a readable package")
            .preview;
        assert!(
            preview
                .problems
                .iter()
                .any(|problem| problem.message.contains("official")),
            "{:?}",
            preview.problems
        );
        let error =
            import_with(&archive, true, &ctx).expect_err("an official id cannot be replaced");
        assert_eq!(error.code, "games.package_invalid");
        assert!(
            !ctx.user_games.join("slot/game.toml").exists(),
            "no user copy was written"
        );
    }

    #[test]
    fn an_export_carries_the_game_its_icon_and_a_user_loader() {
        let base = scratch!("definition-export");
        let game = base.join("game.toml");
        let icon = base.join("icon.webp");
        let loader = base.join("fixture-loader.toml");
        let out = base.join("export.zip");
        write(&game, SLOT);
        write(&icon, "RIFF0000WEBP");
        write(
            &loader,
            "schema = 1\nid = \"fixture-loader\"\nname = \"Fixture\"\n",
        );
        crate::games::write_export(&game, Some(&icon), Some(("fixture-loader", &loader)), &out)
            .expect("the export");
        let archive = Archive::open(&out).expect("the zip");
        let names: Vec<&str> = archive
            .entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert!(names.contains(&"game.toml"));
        assert!(names.contains(&"icon.webp"));
        assert!(names.contains(&"loaders/fixture-loader.toml"));
    }
}
