//! A loader package: the release archive of a mod loader, with its verified source. Installing it imports
//! the archive into the library as one mod whose files keep their game paths, so the loader reaches the game through
//! the overlay like every other mod and the game folder stays untouched.

use serde::Deserialize;

/// The loader package of a game. A loader preset carries the shared package; a game's `[install]` adds check paths and
/// `needed_by` folders, and may name its own build of the loader with `url`, `sha256` and `size`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoaderPackage {
    /// The name the interface shows.
    #[serde(default)]
    pub name: String,
    /// The release version the source belongs to.
    #[serde(default)]
    pub version: String,
    /// The deployment-root-relative folder the release's root stands for: the folder of the program that loads the
    /// proxy DLL, for example `bin/x64`. Empty means the deployment root itself.
    #[serde(default)]
    pub to: String,
    /// Files that prove the loader is there, relative to the deployment root. While the package is needed, mods are not
    /// applied until an enabled mod or the game folder provides each of them.
    #[serde(default)]
    pub check: Vec<String>,
    /// Deployment-root-relative folders whose mods need the package, such as the loader's own mod folder. The package
    /// is needed once an enabled mod places a file there, so mods that work without the loader do not ask for it. Empty
    /// means every mod needs it.
    #[serde(default)]
    pub needed_by: Vec<String>,
    /// The HTTPS link of the release archive. Validation refuses any other scheme.
    #[serde(default)]
    pub url: String,
    /// The SHA-256 of the archive, as 64 hex digits.
    #[serde(default)]
    pub sha256: String,
    /// The expected download size in bytes. A download that grows past it stops.
    #[serde(default)]
    pub size: Option<u64>,
}
