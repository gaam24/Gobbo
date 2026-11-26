//! The data shapes and stable codes the registry writes to its journal and sends to the window.

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// One task as the window reads it. `id` is the stable key that also cancels the task; `kind` and `state` are stable
/// codes that the frontend translates, never display text. `dedupe` is the typed identity of the operation, so the
/// window can group this row with the local row of the same work without parsing `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TaskView {
    /// The key that names the operation and is passed back to `cancel_task`.
    pub id: String,
    /// The game the task works on, when it is bound to one.
    pub game: Option<String>,
    /// A stable code such as `import` or `apply`.
    pub kind: String,
    /// One of `running`, `done`, `cancelled` or `failed`.
    pub state: String,
    /// The last percent the work reported, or `null` while it has not sent any.
    pub percent: Option<u8>,
    /// The typed identity of the operation. Additive: a journal written before this field existed still loads and
    /// gets [`TaskDedupe::Other`], so this is not an on-disk format migration.
    #[serde(default)]
    #[specta(optional)]
    pub dedupe: TaskDedupe,
    /// Why a failed task failed, as the error code and values the window translates. Additive like `dedupe`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[specta(optional)]
    pub error: Option<TaskError>,
}

/// The part of an [`AppError`] a task record keeps: enough to translate the reason, also after a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TaskError {
    pub code: String,
    #[serde(default)]
    pub params: std::collections::BTreeMap<String, String>,
}

impl From<&AppError> for TaskError {
    fn from(error: &AppError) -> Self {
        Self {
            code: error.code.clone(),
            params: error.params.clone(),
        }
    }
}

/// The typed identity of one long task, carried beside its string id. Each workflow has one variant, with the fields
/// that decide which rows describe the same operation. The window groups a backend lifecycle row with the matching
/// local row by this value instead of splitting the id.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum TaskDedupe {
    /// A deployment of a game's active profile.
    Apply {
        /// The game the deployment belongs to.
        game: String,
    },
    /// A loose archive import, keyed by game and archive path.
    Import {
        /// The game the archive is imported into.
        game: String,
        /// The archive that is being imported.
        path: String,
    },
    /// The update of one mod. The mod is not part of the group: one game has one update row at a time.
    Update {
        /// The game the mod belongs to.
        game: String,
    },
    /// The bulk update of one game.
    UpdateAll {
        /// The game being updated.
        game: String,
    },
    /// A requirement install, single or from a plan. One game has one requirement row at a time.
    Requirement {
        /// The game the requirement is installed for.
        game: String,
    },
    /// An automatic collection install, keyed by its game, profile and collection.
    Collection {
        /// The game the collection is installed into.
        game: String,
        /// The profile the collection belongs to.
        profile: String,
        /// The collection being installed.
        collection: String,
    },
    /// A pack import or a pack mod download. Both share one group, so the richer local row replaces the backend one.
    Pack {
        /// The game the pack is imported into.
        game: String,
        /// The pack file that is being used.
        path: String,
    },
    /// A pack link download, keyed by game and link.
    PackLink {
        /// The game the pack is downloaded for.
        game: String,
        /// The link that is being downloaded.
        link: String,
    },
    /// The install of a game's loader package. One game has one loader row at a time.
    Loader {
        /// The game the loader is installed into.
        game: String,
    },
    /// A tool run or a tool's prerequisite install, keyed by game and tool.
    Tool {
        /// The game the tool belongs to.
        game: String,
        /// The tool's id in the game definition.
        tool: String,
    },
    /// Work without a typed identity yet, such as a row restored from a journal written before this field existed.
    #[default]
    Other,
}

/// Stable kind codes, named for the call sites that emit them. Each is a literal, so adding a code cannot shift the
/// value of another one. [`TASK_KINDS`] is the exported list of these same constants.
pub mod kind {
    /// A loose archive import.
    pub const IMPORT: &str = "import";
    /// The update of one mod.
    pub const UPDATE: &str = "update";
    /// The bulk update of a game.
    pub const UPDATE_ALL: &str = "update-all";
    /// A requirement install, single or from a plan.
    pub const REQUIREMENT: &str = "requirement";
    /// A deployment.
    pub const APPLY: &str = "apply";
    /// A pack import or pack mod download.
    pub const PACK: &str = "pack";
    /// A pack link download.
    pub const PACK_LINK: &str = "pack-link";
    /// An automatic collection install.
    pub const COLLECTION: &str = "collection";
    /// A tool run.
    pub const LAUNCH: &str = "launch";
    /// A tool's prerequisite install.
    pub const INSTALL: &str = "install";
    /// The install of a game's loader package.
    pub const LOADER: &str = "loader";
}

/// Every stable kind code the backend emits for a long task, in the order the window expects. The bootstrap tray
/// labels are built from it, and `commands::builder` exports it to `bindings.ts` as `TASK_KINDS`. Adding a code means
/// adding a literal in [`kind`] and one entry here, so no existing code is relabelled.
pub const TASK_KINDS: &[&str] = &[
    kind::IMPORT,
    kind::UPDATE,
    kind::UPDATE_ALL,
    kind::REQUIREMENT,
    kind::APPLY,
    kind::PACK,
    kind::PACK_LINK,
    kind::COLLECTION,
    kind::LAUNCH,
    kind::INSTALL,
    kind::LOADER,
];

/// Stable state codes of a task record.
pub mod state {
    /// The work is running.
    pub const RUNNING: &str = "running";
    /// The work is waiting for a user action and has no cancellation flag.
    pub const WAITING: &str = "waiting";
    /// The work finished successfully.
    pub const DONE: &str = "done";
    /// The work was stopped by the user.
    pub const CANCELLED: &str = "cancelled";
    /// The work ended with an error.
    pub const FAILED: &str = "failed";
}

/// The task journal in the app data folder, written next to the other state files.
pub const TASKS_FILE: &str = "tasks.json";
