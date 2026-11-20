use crate::domain::profiles::ProfilesView;
use crate::domain::profiles::entry::ProfileMod;
use crate::error::AppError;
use crate::games::GameDef;
use crate::library::relations::UnmetRequirements;
use crate::library::{Library, LibraryView};
use crate::status::mod_views::mod_view;

use super::PlanOutcome;

/// Builds every mod view. The work is pure map and string work; after the cache warm it is the largest part of a
/// status, so large lists use a small scoped thread pool and keep the original order by joining the chunks in order.
pub(super) fn mod_views(
    listed: &[ProfileMod],
    game: &GameDef,
    library: &Library,
    view: &LibraryView<'_>,
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
                mod_view(
                    index,
                    entry,
                    game,
                    library,
                    view,
                    profiles,
                    plan,
                    requirements,
                )
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
                                library,
                                view,
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
            let part = handle
                .join()
                .map_err(|_| AppError::unexpected("a status worker panicked"))?;
            mods.extend(part);
        }

        Ok(())
    })?;
    Ok(mods)
}
