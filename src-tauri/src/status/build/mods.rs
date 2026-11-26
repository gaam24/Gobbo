use crate::domain::profiles::ProfilesView;
use crate::domain::profiles::entry::ProfileMod;
use crate::error::AppError;
use crate::games::GameDef;
use crate::games::prefix::effective_source;
use crate::games::stores::Store;
use crate::library::relations::UnmetRequirements;
use crate::status::mod_builders::{LibraryContext, mod_view};
use crate::status::views::ModView;
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

use super::PlanOutcome;

/// A revision of one row that a window can keep. Every serialized field is hashed, so a new field cannot be forgotten.
/// The value is hex text because JavaScript cannot hold a `u64` without losing bits.
pub(super) fn row_revision(view: &ModView) -> String {
    let mut hasher = FxHasher::default();
    view.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// The line under the game name: the Steam app id for Steam installs, the store name for the other stores. An empty
/// source falls back to Steam when the game has an app id; an unknown saved name is a manual install.
pub(super) fn subtitle_for(source: &str, steam_app_ids: &[u32]) -> String {
    match effective_source(source, steam_app_ids) {
        Store::Steam => steam_app_ids
            .first()
            .map(|id| format!("Steam {id}"))
            .unwrap_or_default(),
        store => store.display_name().to_string(),
    }
}

/// A worker panic becomes the same `unexpected` error, with the panic text when it was a string.
fn worker_panic(payload: Box<dyn std::any::Any + Send>) -> AppError {
    let text = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic".to_string());
    let detail = format!("a status worker panicked: {text}");
    crate::infra::log::warn(&detail);
    AppError::unexpected(detail)
}

/// Builds every mod view. The work is pure map and string work; after the cache warm it is the largest part of a
/// status, so large lists use a small scoped thread pool and keep the original order by joining the chunks in order.
pub(super) fn build_mod_views(
    listed: &[ProfileMod],
    game: &GameDef,
    context: &LibraryContext<'_>,
    profiles: &ProfilesView<'_>,
    plan: &PlanOutcome,
    requirements: &UnmetRequirements<'_>,
) -> Result<Vec<crate::status::views::ModView>, AppError> {
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(8)
        .min(listed.len().div_ceil(64));
    if workers <= 1 {
        return Ok(listed
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                mod_view(index, entry, game, context, profiles, plan, requirements)
            })
            .collect());
    }

    let chunk_size = listed.len().div_ceil(workers);
    let mut mods = Vec::with_capacity(listed.len());
    std::thread::scope(|scope| -> Result<(), AppError> {
        let handles: Vec<_> = listed
            .chunks(chunk_size)
            .enumerate()
            .map(|(chunk_index, chunk)| {
                let start = chunk_index * chunk_size;
                scope.spawn(move || {
                    chunk
                        .iter()
                        .enumerate()
                        .map(|(offset, entry)| {
                            mod_view(
                                start + offset,
                                entry,
                                game,
                                context,
                                profiles,
                                plan,
                                requirements,
                            )
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            let part = handle.join().map_err(worker_panic)?;
            mods.extend(part);
        }

        Ok(())
    })?;
    Ok(mods)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_worker_panic_keeps_string_text_and_falls_back_for_other_payloads() {
        let error = worker_panic(Box::new("bad entry"));
        assert_eq!(error.code, "unexpected");
        assert_eq!(
            error.params["detail"],
            "a status worker panicked: bad entry"
        );

        let error = worker_panic(Box::new("owned text".to_string()));
        assert_eq!(
            error.params["detail"],
            "a status worker panicked: owned text"
        );

        let error = worker_panic(Box::new(42_u32));
        assert_eq!(
            error.params["detail"],
            "a status worker panicked: non-string panic"
        );
    }
}
