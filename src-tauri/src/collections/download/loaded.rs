use std::path::PathBuf;

use crate::acquire;
use crate::collections::cache;
use crate::collections::model::CollectionChoiceNeeded;
use crate::collections::status;
use crate::context::{Data, load_game};
use crate::domain::collection::{CollectionOrigin, CollectionPlan};
use crate::domain::library::{Library, LibraryView};
use crate::domain::profiles::{Profile, Profiles};
use crate::domain::source::SourceRef;
use crate::error::AppError;
use crate::games::GameDef;
use crate::providers::SourceDownloaded;

pub(super) struct LoadedCollection {
    pub(super) data: Data,
    pub(super) game: GameDef,
    pub(super) library: std::sync::Arc<Library>,
    pub(super) profile: Profile,
    pub(super) origin: CollectionOrigin,
    pub(super) plan: CollectionPlan,
    pub(super) cache_dir: PathBuf,
    pub(super) cache_archive: Option<PathBuf>,
}

impl LoadedCollection {
    /// The member statuses, using the view the operation built for this library. A view borrows the library, so it
    /// cannot be a field of this owned struct; the command that loads a collection owns it for the whole operation.
    pub(super) fn statuses(&self, view: &LibraryView<'_>) -> status::StatusSet {
        status::statuses_with_view(
            &self.plan,
            &self.library,
            view,
            &self.profile,
            Some(&self.origin),
        )
    }

    /// Whether the library already holds the exact provider file a member would download. A page can have several
    /// parts, so the file id, not the version or the family, decides. This is checked before a transfer starts, because
    /// the import would refuse the same archive by its digest afterwards and leave a duplicate in the downloads folder.
    pub(super) fn has_library_file(view: &LibraryView<'_>, source: &SourceRef) -> bool {
        acquire::existing_file(view, source).is_some()
    }
}

pub(super) fn load(
    root: &Data,
    game_id: &str,
    profile_id: &str,
    collection_key: &str,
) -> Result<LoadedCollection, AppError> {
    let loaded = load_game(root, game_id)?;
    let profile = loaded
        .profiles
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| Profiles::not_found(profile_id))?
        .clone();
    let origin = profile
        .collections
        .iter()
        .find(|origin| origin.key() == collection_key)
        .ok_or_else(|| AppError::user("collection.not_in_profile"))?
        .clone();
    let plan = cache::read_plan_or_missing(&loaded.data, &origin)
        .map_err(|error| error.with("slug", &origin.slug))?;
    let cache_dir = cache::dir(&loaded.data, &plan.info);
    let cache_archive = cache::archive(&loaded.data, &plan.info);
    Ok(LoadedCollection {
        data: loaded.data,
        game: loaded.game,
        library: loaded.library,
        profile,
        origin,
        plan,
        cache_dir,
        cache_archive,
    })
}

pub(super) enum ItemOutcome {
    Installed {
        manual: bool,
    },
    /// The library already had this exact file; nothing was fetched or imported.
    Existing,
    /// The archive is a FOMOD without choices.
    NeedsChoice(Box<CollectionChoiceNeeded>),
    NeedsUser,
    Unsupported,
}

/// An archive fetched ahead of its import. A paid account can download several members at once; the import still
/// happens in plan order because it writes the library and the profile. A failure is carried here so the normal
/// per-member report can show it in the right place.
pub(super) enum PreparedDownload {
    Ready(SourceDownloaded),
    Failed(AppError),
}
