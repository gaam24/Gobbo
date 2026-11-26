use serde::{Deserialize, Serialize};
use specta::Type;

use crate::collections::model::CollectionInstallView;
use crate::domain::fomod::Choice;
use crate::import::model::FomodStepView;

/// The event that asks the window to open a provider page for a member of an automatic round whose file the user must
/// pick by hand. The backend owns the page URL and the download expectation; the window only opens the page.
pub const COLLECTION_PAGE_EVENT: &str = "collection-open-page";

/// The page request carried by [`COLLECTION_PAGE_EVENT`] and by a round that is waiting on a member. The window opens
/// the page through its normal source action; it never has to build a URL or guess the download's owner.
#[derive(Debug, Clone, Serialize, Type)]
pub struct CollectionPageRequest {
    pub game: String,
    pub profile: String,
    pub collection: String,
    pub item: String,
    pub name: String,
    pub provider: String,
    pub game_key: String,
    pub mod_id: String,
    pub url: String,
}

/// The one decision the window must make before a round can continue. It is derived from the round and echoed back
/// with the chosen answer; the controller never asks the window to pick the next member or to drive the loop.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollectionPrompt {
    /// A FOMOD archive has no stored choices; the shared wizard must supply them.
    Choice {
        item: String,
        name: String,
        path: String,
        module_name: String,
        steps: Vec<FomodStepView>,
    },
    /// A manual or browse member needs a picked file, a "mark done" or a skip.
    Manual {
        item: String,
        name: String,
        url: Option<String>,
        instructions: String,
    },
}

/// The answer to a [`CollectionPrompt`]. `item` names the member the prompt referred to; it is an identifier the
/// window echoes, not loop state. The archive `path` is the one the prompt named or the file the user picked.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollectionDecision {
    /// Import the prompted archive with the wizard's FOMOD choices.
    Choice { path: String, choices: Vec<Choice> },
    /// Import a file the user picked for a manual or waiting member.
    File { path: String },
    /// Mark a manual or browse member installed by hand.
    MarkDone,
    /// Skip the member, permanently when `permanent` (then the profile origin excludes it).
    Skip { permanent: bool },
}

/// What one backend-driven round did. `install` carries the fresh session and the round's notices; `prompt` is the
/// single decision that pauses the loop, `waiting` is a member whose provider page is open and whose owned download is
/// expected, and `finished` says the round has no pending members left. A waiting round needs no user decision: the
/// backend imports the matching download and continues on its own.
#[derive(Debug, Serialize, Type)]
pub struct CollectionRoundView {
    pub install: CollectionInstallView,
    pub prompt: Option<CollectionPrompt>,
    pub waiting: Option<CollectionPageRequest>,
    pub finished: bool,
}

/// The pure part of a round: which decision the outcome needs and whether the round is finished. Plain data in, plain
/// data out, so the pause and resume rules are testable without a game, a library or a provider.
#[derive(Debug, Clone)]
pub struct RoundState {
    pub prompt: Option<CollectionPrompt>,
    pub finished: bool,
}
