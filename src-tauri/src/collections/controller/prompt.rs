use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::collections::model::{CollectionInstallView, CollectionProgress, CollectionSessionView};
use crate::collections::{download, session};
use crate::context::Data;
use crate::error::AppError;
use crate::sources::downloads::Downloads;

use super::model::{CollectionDecision, CollectionRoundView};
use super::run::{round_view, run_round};

/// The code `download::import_file` uses when a hand-picked archive turns out to be a FOMOD installer. The wizard has
/// to run before the member can be imported; it is not a failure of the collection.
const HAND_PICKED_NEEDS_CHOICE: &str = "collection.needs_choice";

/// Wraps a session with an empty round outcome. Used when a decision itself settles a member (mark done or skip) and
/// no download happens, so the window still receives a complete round view.
pub(super) fn settled(session: CollectionSessionView) -> CollectionInstallView {
    CollectionInstallView {
        profile: session.profile.clone(),
        installed: Vec::new(),
        failed: Vec::new(),
        needs_choice: Vec::new(),
        needs_user: Vec::new(),
        skipped: Vec::new(),
        warnings: Vec::new(),
        cancelled: false,
        session,
    }
}

fn view(
    root: &Data,
    game: &str,
    profile: &str,
    collection: &str,
) -> Result<CollectionInstallView, AppError> {
    Ok(settled(session::status(root, game, profile, collection)?))
}

/// Applies one decision to the member it names, then drives the next round. The FOMOD wizard and the
/// "already in library" handoffs stay intact: choices go through the shared import, a picked file that turns out to be
/// a FOMOD pauses again with a fresh choice prompt instead of losing the wizard.
#[expect(
    clippy::too_many_arguments,
    reason = "a decision has the collection key, the member, the answer, progress and cancellation"
)]
pub fn resolve_prompt(
    root: &Data,
    game_id: &str,
    profile: &str,
    collection: &str,
    item: &str,
    decision: CollectionDecision,
    downloads: &Downloads,
    progress: &mut dyn FnMut(CollectionProgress),
    cancel: Option<&AtomicBool>,
) -> Result<CollectionRoundView, AppError> {
    match decision {
        CollectionDecision::Choice { path, choices } => {
            download::import_file(
                root,
                game_id,
                profile,
                collection,
                item,
                Path::new(&path),
                Some(choices),
            )?;
        }
        CollectionDecision::File { path } => {
            match download::import_file(
                root,
                game_id,
                profile,
                collection,
                item,
                Path::new(&path),
                None,
            ) {
                Ok(_) => {}
                Err(error) if error.code == HAND_PICKED_NEEDS_CHOICE => {
                    return choice_prompt_for_file(
                        root, game_id, profile, collection, item, &path, error,
                    );
                }
                Err(error) => return Err(error),
            }
        }
        CollectionDecision::MarkDone => {
            session::mark_done(root, game_id, profile, collection, item)?;
        }
        CollectionDecision::Skip { permanent } => {
            session::skip(root, game_id, profile, collection, item, permanent)?;
        }
    }

    run_round(
        root, game_id, profile, collection, downloads, progress, cancel,
    )
}

/// Rebuilds the FOMOD prompt for a picked file whose archive needs choices, so the wizard can carry on after the
/// manual-file handoff just as it does after an automatic download.
fn choice_prompt_for_file(
    root: &Data,
    game_id: &str,
    profile: &str,
    collection: &str,
    item: &str,
    path: &str,
    original: AppError,
) -> Result<CollectionRoundView, AppError> {
    let (module_name, steps) = match crate::import::inspect_at(game_id, Path::new(path))? {
        crate::import::model::ZipInspect::Fomod { module_name, steps } => (module_name, steps),
        crate::import::model::ZipInspect::Plain => return Err(original),
    };
    let name = session::status(root, game_id, profile, collection)?
        .plan
        .item(item)
        .map_or_else(|| item.to_string(), |found| found.name.clone());
    Ok(round_view(CollectionInstallView {
        needs_choice: vec![crate::collections::model::CollectionChoiceNeeded {
            key: item.to_string(),
            name,
            choice: crate::import::model::ChoiceNeeded {
                path: PathBuf::from(path),
                module_name,
                steps,
            },
        }],
        ..view(root, game_id, profile, collection)?
    }))
}
