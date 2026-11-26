use tauri::State;

use crate::commands::helpers::{blocking, for_game};
use crate::context::Data;
use crate::error::AppError;
use crate::providers;

/// Opens a mod's page on a provider. The provider builds the URL, so the interface cannot pass an arbitrary query or
/// fragment. `tab` picks a page section; only the files tab is allowed.
fn open_source_page_now(
    provider_id: &str,
    game_key: &str,
    mod_id: &str,
    tab: Option<&str>,
) -> Result<(), AppError> {
    let provider = providers::require(provider_id)?;

    if !provider.capabilities().deep_link {
        return Err(providers::unsupported_by(provider_id));
    }

    let url = provider
        .page_url(game_key, mod_id, tab)
        .ok_or_else(|| providers::bad_link().with("provider", provider_id))?;
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(AppError::unexpected)
}

/// Opens the page of a library mod. The backend reads the provider's mod id and the game's provider key, so a stale game
/// catalog in the window cannot turn the action into a silent no-op.
fn open_update_page_now(
    root: &Data,
    game_id: &str,
    id: &str,
    provider_id: &str,
    tab: Option<&str>,
) -> Result<(), AppError> {
    super::update::with_family(root, game_id, id, |game, _library, family| {
        let provider = crate::domain::source::ProviderId::new(provider_id);
        let source = family
            .sources
            .get(&provider)
            .ok_or_else(|| crate::context::library::not_in_library(id))?;
        let game_key = game
            .provider_key(provider_id)
            .ok_or_else(|| providers::no_game_key(provider_id))?;
        open_source_page_now(provider_id, game_key, &source.mod_id, tab)
    })
}

/// Opens a mod's page on any provider the game has a key for.
#[tauri::command]
#[specta::specta]
pub async fn open_source_page(
    provider: String,
    game_key: String,
    mod_id: String,
    tab: Option<String>,
) -> Result<(), AppError> {
    blocking(move || open_source_page_now(&provider, &game_key, &mod_id, tab.as_deref())).await
}

/// Opens the files page of a mod the library knows. The page URL comes from the provider, the mod id from the library and
/// the game key from the game definition.
#[tauri::command]
#[specta::specta]
pub async fn open_update_page(
    data: State<'_, Data>,
    game: String,
    id: String,
    provider: String,
    tab: Option<String>,
) -> Result<(), AppError> {
    for_game(data, game, move |root, game| {
        open_update_page_now(root, game, &id, &provider, tab.as_deref())
    })
    .await
}
