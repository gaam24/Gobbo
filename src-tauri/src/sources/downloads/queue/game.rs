//! The game a download link belongs to: by the provider key in the link, or by the first listed game the user has.

use crate::context::Data;
use crate::context::managed::managed_ids;
use crate::domain::source::SourceLink;
use crate::error::AppError;
use crate::games;
use crate::providers::SourceProvider;

/// The added game whose provider key is `key`. A link from a game that is not added cannot be downloaded.
fn game_of(root: &Data, provider: &str, key: &str) -> Result<String, AppError> {
    managed_ids(root)?
        .into_iter()
        .find(|id| games::load(id).is_ok_and(|game| game.provider_key(provider) == Some(key)))
        .ok_or_else(|| {
            AppError::user("source.game_not_added")
                .with("provider", provider)
                .with("key", key)
        })
}

/// The game a link's mod goes to, with the provider's key for that game. A link that names its game is matched by that
/// key. A link that names none (a Thunderstore install link) takes the first game its mod is listed for that the user
/// has added.
pub(super) fn game_for_link(
    root: &Data,
    provider: &dyn SourceProvider,
    link: &SourceLink,
) -> Result<(String, String), AppError> {
    let id = provider.id();
    let provider_id = id.as_str();
    if let Some(key) = &link.game_key {
        return Ok((game_of(root, provider_id, key)?, key.clone()));
    }
    let keys = provider.listed_games(&link.source)?;
    first_game(&keys, |key| game_of(root, provider_id, key).ok()).ok_or_else(|| {
        AppError::user("source.game_not_added")
            .with("provider", provider_id)
            .with("key", keys.join(", "))
    })
}

/// The first key that names a game the user has, with that game's id.
pub(super) fn first_game(
    keys: &[String],
    mut game_for: impl FnMut(&str) -> Option<String>,
) -> Option<(String, String)> {
    keys.iter()
        .find_map(|key| game_for(key).map(|game| (game, key.clone())))
}
