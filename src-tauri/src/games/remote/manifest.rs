//! The catalog manifest: fetching `catalog.json`, the types it parses into, and the gate that decides whether an entry
//! can be offered: its schema and the `min_gobbo` floor of the game and its loader. Any field the catalog grows later is ignored, so an older app can still read a newer
//! file as long as its API version is unchanged.

use std::cmp::Ordering;
use std::io::Read;
use std::time::Instant;

use serde::Deserialize;
use sha2::Sha256;
use specta::Type;

use crate::domain::version::compare;
use crate::error::{AppError, FileError};
use crate::infra::http::{USER_AGENT, agent, trace_failure, trace_response};
use crate::infra::io::digest_file;

use super::super::definition::SUPPORTED_SCHEMA;
use super::super::definition_path_from;
use super::{CatalogEntryView, CatalogStatus};

/// The API version of `catalog.json` this app understands. A newer shape is refused as a whole instead of being read
/// field by field, so a changed meaning cannot be mistaken for a valid entry.
const SUPPORTED_API_VERSION: u32 = 1;

/// The manifest at the catalog address: a version and the game and loader entries.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoteCatalog {
    /// The format version of the whole manifest.
    pub api_version: u32,
    /// The game definitions the catalog offers.
    pub games: Vec<RemoteGame>,
    /// The shared loader presets the games may name.
    pub loaders: Vec<RemoteLoader>,
}

/// One game definition in the manifest, with the checksums that make a download verifiable.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoteGame {
    /// The folder name of the game, also the id the window sends back to install.
    pub id: String,
    /// The display name of the game.
    pub name: String,
    /// The `game.toml` schema the file uses.
    pub schema: u32,
    /// The loader preset id this game names, when it has one.
    pub loader: Option<String>,
    /// Whether a real installation was tested.
    pub verified: bool,
    /// The path of `game.toml`, relative to the catalog base address.
    pub file: String,
    /// The SHA-256 of `game.toml`.
    pub sha256: String,
    /// The path of the icon, relative to the catalog base address.
    pub icon: Option<String>,
    /// The SHA-256 of the icon.
    pub icon_sha256: Option<String>,
    /// The oldest Gobbo release that serves this definition correctly, copied from `game.toml`.
    pub min_gobbo: Option<String>,
}

/// One shared loader preset in the manifest. The catalog also carries its name and schema; install needs only these.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoteLoader {
    /// The id a game definition names in its `loader` field.
    pub id: String,
    /// The path of the loader TOML, relative to the catalog base address.
    pub file: String,
    /// The SHA-256 of the loader TOML.
    pub sha256: String,
    /// The oldest Gobbo release that serves this preset correctly, copied from the preset.
    pub min_gobbo: Option<String>,
}

impl RemoteCatalog {
    /// The game with `id`, when the manifest lists it.
    pub(super) fn game(&self, id: &str) -> Option<&RemoteGame> {
        self.games.iter().find(|game| game.id == id)
    }

    /// The loader preset with `id`, when the manifest lists it.
    pub(super) fn loader(&self, id: &str) -> Option<&RemoteLoader> {
        self.loaders.iter().find(|loader| loader.id == id)
    }
}

/// A failure to read the manifest: a down network, a broken answer or a newer API version. All three are retryable
/// except the version gate, which needs a newer app and is permanent.
pub(super) fn fetch_error(detail: impl ToString) -> AppError {
    AppError::retryable("catalog.fetch").with("detail", detail)
}

/// Fetches and parses the manifest at `url`, refusing a format this app does not understand. There is no cache: the
/// window and an install each ask for the current file, and a re-check sees a catalog that changed minutes ago.
pub(super) fn fetch_catalog(url: &str) -> Result<RemoteCatalog, AppError> {
    let bytes = fetch_bytes(url)?;
    let catalog: RemoteCatalog = serde_json::from_slice(&bytes)
        .map_err(|error| fetch_error(format!("catalog is not valid JSON: {error}")))?;

    if catalog.api_version > SUPPORTED_API_VERSION {
        return Err(AppError::new("catalog.unsupported")
            .with("api_version", catalog.api_version)
            .with("supported", SUPPORTED_API_VERSION));
    }

    Ok(catalog)
}

/// Reads the manifest body with the shared HTTP agent. A non-success status and a broken connection are both ordinary
/// "the network may work later" failures, not bugs in the app.
fn fetch_bytes(url: &str) -> Result<Vec<u8>, AppError> {
    let started = Instant::now();
    let response = agent()
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| {
            trace_failure("catalog", "GET", url, started, &error);
            fetch_error(error)
        })?;
    let status = response.status().as_u16();

    if !(200..300).contains(&status) {
        trace_response("catalog", "GET", url, status, None, started);
        return Err(fetch_error(format!(
            "catalog answered with status {status}"
        )));
    }

    let mut bytes = Vec::new();
    let read = response
        .into_body()
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(fetch_error)?;
    trace_response("catalog", "GET", url, status, Some(read as u64), started);
    Ok(bytes)
}

/// Classifies the manifest against the definitions on this computer. The folder precedence is the same as the one
/// `definition_path` uses: user, official, bundled. A user definition always wins and is never reported as changed.
pub(super) fn classify(
    catalog: &RemoteCatalog,
    catalog_games: Option<&std::path::Path>,
    user_games: Option<&std::path::Path>,
    bundled_games: &std::path::Path,
) -> Result<CatalogStatus, AppError> {
    let mut status = CatalogStatus {
        changed: Vec::new(),
        available: Vec::new(),
        blocked: Vec::new(),
    };

    for game in &catalog.games {
        let min = floor(catalog, game);
        let too_new = min
            .as_deref()
            .is_some_and(|min| compare(min, env!("CARGO_PKG_VERSION")) == Ordering::Greater);

        if too_new || game.schema > SUPPORTED_SCHEMA {
            status.blocked.push(view(game, min));
            continue;
        }

        let user_path = user_games.map(|dir| dir.join(&game.id).join("game.toml"));
        if user_path.as_deref().is_some_and(std::path::Path::is_file) {
            // An id the user owns is theirs to edit; the catalog has no say over that file.
            continue;
        }

        let path = definition_path_from(user_games, catalog_games, bundled_games, &game.id);
        if !path.is_file() {
            status.available.push(view(game, min));
            continue;
        }

        let sha = digest_file::<Sha256>(&path, &mut |_| {})
            .map_err(|error| FileError::new(&path, error))?;
        if !sha.eq_ignore_ascii_case(&game.sha256) {
            status.changed.push(view(game, min));
        }
    }

    Ok(status)
}

/// The higher `min_gobbo` of a game and the loader it names, when either sets one.
fn floor(catalog: &RemoteCatalog, game: &RemoteGame) -> Option<String> {
    let loader = game
        .loader
        .as_deref()
        .and_then(|id| catalog.loader(id))
        .and_then(|loader| loader.min_gobbo.as_deref());

    match (game.min_gobbo.as_deref(), loader) {
        (Some(a), Some(b)) => Some(
            if compare(a, b) == Ordering::Less {
                b
            } else {
                a
            }
            .to_string(),
        ),
        (a, b) => a.or(b).map(str::to_string),
    }
}

/// The small part of a manifest entry the Games page draws.
fn view(game: &RemoteGame, min_gobbo: Option<String>) -> CatalogEntryView {
    CatalogEntryView {
        id: game.id.clone(),
        name: game.name.clone(),
        loader: game.loader.clone(),
        verified: game.verified,
        min_gobbo,
    }
}
