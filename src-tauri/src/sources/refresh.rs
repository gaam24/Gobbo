//! Refreshing the page data of a mod through the provider port, so a new provider needs no feature change.

use crate::context::Data;
use crate::context::edit_library;
use crate::domain::source::SourceMeta;
use crate::domain::source::SourceRef;
use crate::error::{AppError, ErrorKind, MOD_MISSING};
use crate::games;
use crate::infra::clock::now_seconds;
use crate::providers;
use crate::sources::update::apply_meta;

/// The page a refresh stores: the fresh read merged with the meta saved for the same mod, so a field the read does not
/// bring (or a picture this read did not save) keeps its saved value, exactly as the update check keeps it. The check
/// time is this refresh's.
fn refreshed_meta(fresh: SourceMeta, saved: Option<&SourceMeta>, checked_at: u32) -> SourceMeta {
    SourceMeta::checked(fresh, saved, checked_at)
}

/// Reads a mod's page from its provider and saves the common metadata with the family that holds the mod.
pub fn refresh_source_meta(
    root: &Data,
    game_id: &str,
    provider_id: &str,
    mod_id: &str,
) -> Result<SourceMeta, AppError> {
    let provider = providers::require(provider_id)?;
    let game = games::load(game_id)?;
    let game_key = game
        .provider_key(provider_id)
        .ok_or_else(|| providers::no_game_key(provider_id))?
        .to_string();
    let key = providers::required_key(provider)?;
    let fresh = provider.fetch_meta(&key, &game_key, mod_id)?;
    // Reading the page is a check of its newest version, so the list can show when it was read.
    let checked_at = now_seconds();

    edit_library(root, game_id, |_, _, library| {
        let (family_id, saved) = library
            .families
            .iter()
            .find_map(|(id, family)| {
                family
                    .sources
                    .get(provider_id)
                    .filter(|source| source.mod_id == mod_id)
                    .map(|source| (id.clone(), source.meta.clone()))
            })
            .ok_or_else(|| {
                AppError::new(MOD_MISSING)
                    .with("folder", mod_id)
                    .with("provider", provider_id)
                    .kind(ErrorKind::Permanent)
            })?;
        let meta = refreshed_meta(fresh, saved.as_ref(), checked_at);
        let family = library.family_mut(&family_id)?;
        apply_meta(family, &SourceRef::new(provider_id, mod_id), &meta);
        Ok(meta)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refresh_keeps_the_saved_picture_of_the_same_address() {
        let saved = SourceMeta {
            picture_url: Some("https://example.invalid/icon.png".to_string()),
            picture: Some("generic-a-b.png".to_string()),
            ..SourceMeta::default()
        };
        let same = SourceMeta {
            picture_url: Some("https://example.invalid/icon.png".to_string()),
            ..SourceMeta::default()
        };
        let merged = refreshed_meta(same, Some(&saved), 9);
        assert_eq!(merged.picture.as_deref(), Some("generic-a-b.png"));

        let moved = SourceMeta {
            picture_url: Some("https://example.invalid/new.png".to_string()),
            ..SourceMeta::default()
        };
        assert_eq!(refreshed_meta(moved, Some(&saved), 9).picture, None);
    }

    #[test]
    fn a_refresh_keeps_the_saved_fields_the_read_does_not_bring() {
        let saved = SourceMeta {
            summary: Some("Saved summary".to_string()),
            created_at: Some(100),
            extra: std::collections::BTreeMap::from([("kept".to_string(), "old".to_string())]),
            ..SourceMeta::default()
        };
        let fresh = SourceMeta {
            updated_at: Some(200),
            extra: std::collections::BTreeMap::from([("kept".to_string(), "new".to_string())]),
            ..SourceMeta::default()
        };

        let merged = refreshed_meta(fresh, Some(&saved), 300);

        assert_eq!(merged.summary.as_deref(), Some("Saved summary"));
        assert_eq!(merged.created_at, Some(100));
        assert_eq!(merged.updated_at, Some(200));
        assert_eq!(merged.extra["kept"], "new");
        assert_eq!(merged.checked_at, Some(300));
    }

    #[test]
    fn an_unknown_provider_is_refused_before_a_request() {
        let base = crate::test_support::scratch!("refresh-unknown");
        let data = Data::new(base.join("data"));
        let error = refresh_source_meta(&data, "mirror", "modio", "1").unwrap_err();
        assert_eq!(error.code, "source.unknown");
    }
}
