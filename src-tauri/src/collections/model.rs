//! The IPC shapes of collections. They carry the plan and the statuses; the window does not derive either.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::domain::collection::{
    CollectionCounts, CollectionItemStatus, CollectionOrigin, CollectionPlan,
};
use crate::error::{AppError, Message};
use crate::import::model::FomodStepView;

/// A collection opened for preview, with the status of the active profile.
#[derive(Debug, Serialize, Type)]
pub struct CollectionPreview {
    pub plan: CollectionPlan,
    pub statuses: BTreeMap<String, CollectionItemStatus>,
    pub counts: CollectionCounts,
    pub profiles: Vec<CollectionProfileView>,
    /// The profile the statuses were computed for.
    pub profile: String,
    pub has_archive: bool,
    pub key_saved: bool,
    /// The provider needs a saved key (Nexus). Thunderstore needs none, so the window shows no account notice.
    pub needs_key: bool,
    pub direct_download: bool,
    pub account: Option<String>,
    pub already_installed: bool,
}

/// One profile, as the install dialog offers it.
#[derive(Debug, Serialize, Type)]
pub struct CollectionProfileView {
    pub id: String,
    pub name: String,
    pub active: bool,
    /// A pack profile cannot take a collection; a pack update would replace the list.
    pub pack: bool,
    pub collections: u32,
}

/// Where a new collection block enters an existing list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CollectionPosition {
    Top,
    Bottom,
}

/// What the preview dialog sends when the user starts an install.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct CollectionInstallRequest {
    /// The collection URL or `nxm://` link the preview was opened with.
    pub link: String,
    pub revision: Option<u32>,
    /// `None` creates a profile.
    pub profile: Option<String>,
    /// The name of a new profile. Ignored when `profile` names one.
    pub profile_name: String,
    /// The selected item keys. Required items left out are treated as skipped.
    pub selected: Vec<String>,
    pub position: CollectionPosition,
    /// Switch to the new profile when the install ends.
    pub activate: bool,
}

/// A collection attached to a profile, with its live statuses.
#[derive(Debug, Serialize, Type)]
pub struct CollectionSessionView {
    pub profile: String,
    pub origin: CollectionOrigin,
    pub plan: CollectionPlan,
    pub statuses: BTreeMap<String, CollectionItemStatus>,
    pub counts: CollectionCounts,
    /// The first member that still needs work.
    pub next: Option<String>,
    /// Whether the provider has a newer revision than the one installed in this profile.
    pub update: bool,
}

/// What one automatic install round did. The window repeats the round while `needs_choice` is not empty.
#[derive(Debug, Serialize, Type)]
pub struct CollectionInstallView {
    pub profile: String,
    pub installed: Vec<String>,
    pub failed: Vec<CollectionFailure>,
    pub needs_choice: Vec<CollectionChoiceNeeded>,
    pub needs_user: Vec<String>,
    pub skipped: Vec<String>,
    pub warnings: Vec<Message>,
    pub cancelled: bool,
    pub session: CollectionSessionView,
}

/// One collection item that an install round could not install, with the error the window shows.
#[derive(Debug, Serialize, Type)]
pub struct CollectionFailure {
    pub key: String,
    pub name: String,
    pub error: AppError,
}

/// One collection item whose archive needs FOMOD choices before it can be imported.
#[derive(Debug, Serialize, Type)]
pub struct CollectionChoiceNeeded {
    pub key: String,
    pub name: String,
    pub path: String,
    pub module_name: String,
    pub steps: Vec<FomodStepView>,
}

/// One progress step of an automatic collection install.
#[derive(Debug, Clone, Serialize, Type)]
pub struct CollectionProgress {
    pub key: String,
    pub name: String,
    pub index: u32,
    pub total: u32,
    pub percent: u32,
    pub state: CollectionItemState,
    pub error: Option<AppError>,
}

/// The step an automatic collection install is at for one member, or why it stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CollectionItemState {
    Downloading,
    Importing,
    Patching,
    Installed,
    Failed,
    NeedsChoice,
    NeedsUser,
    Skipped,
}

/// What the guided flow does with one member.
#[derive(Debug, Serialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollectionItemAction {
    /// Already in the profile, or just imported from a bundle or a direct URL.
    Installed,
    /// The exact file is already in the library, so no download is needed. The user enables it from the mod list.
    AlreadyInLibrary {
        name: String,
    },
    /// The file page is open. The download with this owner is the member's file.
    Waiting {
        owner: String,
        url: String,
    },
    /// The user has to download this resource and import it or mark it done.
    NeedsUser {
        url: Option<String>,
        instructions: String,
    },
    /// The archive needs FOMOD choices before it can be imported.
    NeedsChoice {
        path: String,
        module_name: String,
        steps: Vec<FomodStepView>,
    },
    /// The source cannot be installed automatically; the reason is shown.
    Unsupported {
        reason: Message,
    },
    Failed {
        error: AppError,
    },
}
