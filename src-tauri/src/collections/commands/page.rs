//! Opening the provider page of a collection recorded in a profile.

use tauri::AppHandle;
use tauri::Manager;

use crate::commands::helpers::blocking;
use crate::context::{Data, load_game};
use crate::domain::profiles::Profiles;
use crate::error::AppError;

/// Opens the collection page of one profile origin.
#[tauri::command]
#[specta::specta]
pub async fn open_collection_page(
    app: AppHandle,
    game: String,
    profile: String,
    collection: String,
) -> Result<(), AppError> {
    blocking(move || {
        let data = app.state::<Data>();
        let loaded = load_game(data.inner(), &game)?;
        let profile = loaded
            .profiles
            .profiles
            .iter()
            .find(|known| known.id == profile)
            .ok_or_else(|| Profiles::not_found(&profile))?;
        let origin = profile
            .collections
            .iter()
            .find(|origin| origin.key() == collection)
            .ok_or_else(|| AppError::user("collection.not_in_profile"))?;
        // The page is rebuilt by the provider, so a stored URL never decides what the opener visits.
        let url = crate::providers::collection_provider(&origin.provider)
            .and_then(|collection| collection.page_url(&origin.game_key, &origin.slug))
            .ok_or_else(|| AppError::user("collection.bad_link").with("url", &origin.url))?;
        tauri_plugin_opener::open_url(&url, None::<&str>).map_err(AppError::unexpected)
    })
    .await
}
