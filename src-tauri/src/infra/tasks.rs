//! Cancellation flags and the task registry for the long operations the window can stop or watch.
//!
//! The app owns one registry, so a task started while the window shows another game is still listed and cancellable
//! from the global Working panel. The registry publishes its lifecycle as Tauri events; it never translates the codes it
//! emits.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use super::json;
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

/// The reason of a task that was still running when Gobbo stopped. The work cannot continue after a restart.
const INTERRUPTED: &str = "tasks.interrupted";

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

/// The single owner of the string ids and the typed identities of long tasks. Each named constructor returns the id
/// the workflow already used, together with the [`TaskDedupe`] a command passes to the task registry. No command
/// builds a task id with `format!` any more, and no feature has to re-parse an id to know which rows to merge.
pub struct TaskKey {
    id: String,
    dedupe: TaskDedupe,
}

impl TaskKey {
    /// The key of one tool operation: `kind` is [`kind::LAUNCH`] or [`kind::INSTALL`].
    pub fn tool(kind: &str, game: &str, tool: &str) -> Self {
        Self {
            id: format!("tool:{kind}:{game}:{tool}"),
            dedupe: TaskDedupe::Tool {
                game: game.to_string(),
                tool: tool.to_string(),
            },
        }
    }

    /// The key of the install of one game's loader package.
    pub fn loader(game: &str) -> Self {
        Self {
            id: format!("loader:{game}"),
            dedupe: TaskDedupe::Loader {
                game: game.to_string(),
            },
        }
    }

    /// The key of a deployment of one game's active profile.
    pub fn apply(game: &str) -> Self {
        Self {
            id: format!("apply:{game}"),
            dedupe: TaskDedupe::Apply {
                game: game.to_string(),
            },
        }
    }

    /// The key of one loose archive import, named by game and archive path.
    pub fn import(game: &str, path: &str) -> Self {
        Self {
            id: format!("import:{game}\n{path}"),
            dedupe: TaskDedupe::Import {
                game: game.to_string(),
                path: path.to_string(),
            },
        }
    }

    /// The key of one mod update. The mod id stays in the string for cancellation, but the dedupe group is per game.
    pub fn update_mod(game: &str, id: &str) -> Self {
        Self {
            id: format!("update-mod:{game}\n{id}"),
            dedupe: TaskDedupe::Update {
                game: game.to_string(),
            },
        }
    }

    /// The key of the bulk update of one game.
    pub fn update_all(game: &str) -> Self {
        Self {
            id: format!("update:{game}"),
            dedupe: TaskDedupe::UpdateAll {
                game: game.to_string(),
            },
        }
    }

    /// The key shared by the single and the bulk requirement install of one game. A new install replaces the flag of
    /// the previous one, which is what a retry in the window does.
    pub fn requirement(game: &str) -> Self {
        Self {
            id: format!("requirement:{game}"),
            dedupe: TaskDedupe::Requirement {
                game: game.to_string(),
            },
        }
    }

    /// The key of an automatic collection install, named by game, profile and collection.
    pub fn collection_install(game: &str, profile: &str, collection: &str) -> Self {
        Self {
            id: format!("collection:{game}\n{profile}\n{collection}"),
            dedupe: TaskDedupe::Collection {
                game: game.to_string(),
                profile: profile.to_string(),
                collection: collection.to_string(),
            },
        }
    }

    /// The key of one pack import. A pack download has its own key so the two can be stopped separately.
    pub fn pack_import(game: &str, path: &str) -> Self {
        Self {
            id: format!("pack:{game}\n{path}"),
            dedupe: TaskDedupe::Pack {
                game: game.to_string(),
                path: path.to_string(),
            },
        }
    }

    /// The key of one pack mod download. It shares the dedupe group of the import but has its own cancellation flag.
    pub fn pack_download(game: &str, path: &str) -> Self {
        Self {
            id: format!("pack-download:{game}\n{path}"),
            dedupe: TaskDedupe::Pack {
                game: game.to_string(),
                path: path.to_string(),
            },
        }
    }

    /// The key of one pack link download, named by game and link.
    pub fn pack_link(game: &str, link: &str) -> Self {
        Self {
            id: format!("pack-link:{game}\n{link}"),
            dedupe: TaskDedupe::PackLink {
                game: game.to_string(),
                link: link.to_string(),
            },
        }
    }

    /// The string id that also names the cancellation flag and the journal row.
    pub fn as_str(&self) -> &str {
        &self.id
    }

    /// The typed identity of the operation, used by the window to group lifecycle and progress rows.
    pub const fn dedupe(&self) -> &TaskDedupe {
        &self.dedupe
    }

    /// Rebuilds the typed identity from an id. The shared `run_task` helper takes a plain `String`, so converting one
    /// back here is the single place that reads the encoding; the named constructors remain the only writers.
    fn dedupe_from_id(id: &str) -> TaskDedupe {
        let mut parts = id.split('\n');
        let head = parts.next().unwrap_or("");
        let tail: Vec<&str> = parts.collect();
        let (prefix, game) = head.split_once(':').unwrap_or((head, ""));
        let rest = tail.join("\n");

        match prefix {
            "apply" => TaskDedupe::Apply {
                game: game.to_string(),
            },
            "import" => TaskDedupe::Import {
                game: game.to_string(),
                path: rest,
            },
            "update-mod" => TaskDedupe::Update {
                game: game.to_string(),
            },
            "update" => TaskDedupe::UpdateAll {
                game: game.to_string(),
            },
            "requirement" => TaskDedupe::Requirement {
                game: game.to_string(),
            },
            "loader" => TaskDedupe::Loader {
                game: game.to_string(),
            },
            "collection" => TaskDedupe::Collection {
                game: game.to_string(),
                profile: tail.first().copied().unwrap_or("").to_string(),
                collection: tail.get(1).copied().unwrap_or("").to_string(),
            },
            "pack" | "pack-download" => TaskDedupe::Pack {
                game: game.to_string(),
                path: rest,
            },
            "pack-link" => TaskDedupe::PackLink {
                game: game.to_string(),
                link: rest,
            },
            // `tool:<kind>:<game>:<tool>`: the game and the tool are the last two fields.
            "tool" => game
                .rsplit_once(':')
                .and_then(|(head, tool)| {
                    let (_, game) = head.split_once(':')?;
                    Some(TaskDedupe::Tool {
                        game: game.to_string(),
                        tool: tool.to_string(),
                    })
                })
                .unwrap_or_default(),
            _ => TaskDedupe::Other,
        }
    }
}

impl From<String> for TaskKey {
    fn from(id: String) -> Self {
        let dedupe = Self::dedupe_from_id(&id);
        Self { id, dedupe }
    }
}

impl From<TaskKey> for String {
    fn from(key: TaskKey) -> Self {
        key.id
    }
}

/// Every stable kind code the backend emits for a long task, in one place. The named constants in [`kind`] index this
/// list, the bootstrap tray labels are built from it, and `commands::builder` exports it to `bindings.ts` as
/// `TASK_KINDS`. Adding a code here is the only step needed to make the frontend see it.
pub const TASK_KINDS: &[&str] = &[
    "import",
    "update",
    "update-all",
    "requirement",
    "apply",
    "pack",
    "pack-link",
    "collection",
    "launch",
    "install",
    "loader",
];

/// Stable kind codes, named for the call sites that emit them. Each one indexes [`TASK_KINDS`], so there is no second
/// list of codes to keep in step.
pub mod kind {
    /// A loose archive import.
    pub const IMPORT: &str = super::TASK_KINDS[0];
    /// The update of one mod.
    pub const UPDATE: &str = super::TASK_KINDS[1];
    /// The bulk update of a game.
    pub const UPDATE_ALL: &str = super::TASK_KINDS[2];
    /// A requirement install, single or from a plan.
    pub const REQUIREMENT: &str = super::TASK_KINDS[3];
    /// A deployment.
    pub const APPLY: &str = super::TASK_KINDS[4];
    /// A pack import or pack mod download.
    pub const PACK: &str = super::TASK_KINDS[5];
    /// A pack link download.
    pub const PACK_LINK: &str = super::TASK_KINDS[6];
    /// An automatic collection install.
    pub const COLLECTION: &str = super::TASK_KINDS[7];
    /// A tool run.
    pub const LAUNCH: &str = super::TASK_KINDS[8];
    /// A tool's prerequisite install.
    pub const INSTALL: &str = super::TASK_KINDS[9];
    /// The install of a game's loader package.
    pub const LOADER: &str = super::TASK_KINDS[10];
}

/// Stable state codes of a task record.
pub mod state {
    /// The work is running.
    pub(super) const RUNNING: &str = "running";
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

/// The on-disk rows of work that outlives one session: running work, waiting work and failures. Successful and
/// cancelled work already has its terminal event, so it is dropped from the journal on the next change.
struct TaskJournal {
    path: PathBuf,
}

impl TaskJournal {
    /// The journal of the app data folder `dir`.
    fn new(dir: PathBuf) -> Self {
        Self {
            path: dir.join(TASKS_FILE),
        }
    }

    /// Loads the rows and turns work that was still running when the app stopped into failures, so a restart offers a
    /// Retry instead of showing work that can never report progress again. A missing or damaged journal is empty; a
    /// failed read is a diagnostic, never a reason to refuse to start the app.
    fn load(&self) -> Vec<TaskView> {
        let Some(mut rows) = json::read_json_or_none::<Vec<TaskView>>(&self.path) else {
            return Vec::new();
        };

        for row in &mut rows {
            if row.state == state::RUNNING {
                row.state = state::FAILED.to_string();
                row.error = Some(TaskError {
                    code: INTERRUPTED.to_string(),
                    params: std::collections::BTreeMap::new(),
                });
            }
        }

        rows
    }

    /// Writes the rows through the app's atomic JSON writer. A failed write is logged and ignored: the window still
    /// sees the in-memory row and the next change tries again.
    fn save(&self, rows: &[TaskView]) {
        if let Err(error) = json::write_json(&self.path, &rows) {
            crate::infra::log::warn(format!(
                "could not save the task journal {}: {error}",
                self.path.display()
            ));
        }
    }
}

/// Whether a record still belongs in the journal after a restart.
fn persisted(task: &TaskView) -> bool {
    task.state == state::RUNNING || task.state == state::FAILED || task.state == state::WAITING
}

/// The event that carries a newly started [`TaskView`].
const TASK_STARTED_EVENT: &str = "task-started";
/// The event that carries a finished [`TaskView`].
const TASK_FINISHED_EVENT: &str = "task-finished";
/// The event that carries a cancelled [`TaskView`].
const TASK_CANCELLED_EVENT: &str = "task-cancelled";
/// The event that carries a [`TaskView`] whose `percent` changed.
const TASK_PROGRESS_EVENT: &str = "task-progress";

#[derive(Default)]
struct Registry {
    /// The flags named by the keys the existing cancel commands use.
    flags: HashMap<String, Arc<AtomicBool>>,
    /// The last record for each key, including terminal ones, so a cancel that arrives after the work cannot revive
    /// a finished task and a test can inspect the outcome.
    tasks: HashMap<String, TaskView>,
}

/// The cancellation flags and task records of the running operations, by a key that names the operation. A command
/// starts a record, hands its flag to the work and ends it when the work returns; the matching cancel command sets the
/// flag. The flag is shared with the work through an `Arc`, so a cancel command never waits for the operation itself.
pub struct TaskFlags {
    /// Shared with the [`TaskHandle`]s handed to progress callbacks, so they can update a row without owning Tauri
    /// state. The mutex guards only the flags and the records; it is never held across another lock.
    inner: Arc<Mutex<Registry>>,
    /// Set by the app; absent only in the unit tests, which inspect the registry without a window.
    app: Option<AppHandle>,
    /// The on-disk journal, held by this state instead of a global. Absent only in unit tests that need no disk.
    journal: Option<TaskJournal>,
}

impl Default for TaskFlags {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Registry::default())),
            app: None,
            journal: None,
        }
    }
}

impl TaskFlags {
    /// Creates the registry with the handle used to publish task events and with the journal of the app data folder.
    /// Rows that were running when the app last stopped are restored as failures, ready for a Retry.
    pub fn new(app: AppHandle) -> Self {
        let journal = app.path().app_data_dir().ok().map(TaskJournal::new);
        let tasks = journal
            .as_ref()
            .map(TaskJournal::load)
            .unwrap_or_default()
            .into_iter()
            .map(|view| (view.id.clone(), view))
            .collect();
        Self {
            inner: Arc::new(Mutex::new(Registry {
                tasks,
                ..Registry::default()
            })),
            app: Some(app),
            journal,
        }
    }

    /// Builds a registry with a journal at `dir`, for a test that simulates a restart. Without an app handle the
    /// restored rows publish no events; the window hydrates them from `listTasks` instead.
    #[cfg(test)]
    pub(crate) fn with_journal(dir: PathBuf) -> Self {
        let journal = TaskJournal::new(dir);
        let tasks = journal
            .load()
            .into_iter()
            .map(|view| (view.id.clone(), view))
            .collect();
        Self {
            inner: Arc::new(Mutex::new(Registry {
                tasks,
                ..Registry::default()
            })),
            app: None,
            journal: Some(journal),
        }
    }

    /// Writes the rows that must survive a restart. The caller holds the registry guard, so two changes cannot land
    /// their snapshots in the wrong order. The file is small and is replaced atomically.
    fn persist(&self, inner: &Registry) {
        if let Some(journal) = &self.journal {
            let rows: Vec<TaskView> = inner
                .tasks
                .values()
                .filter(|task| persisted(task))
                .cloned()
                .collect();
            journal.save(&rows);
        }
    }

    /// A cloneable handle bound to one task key. A progress callback reports percent through it without owning this
    /// state; an unknown key is a no-op.
    pub fn handle(&self, id: &str) -> TaskHandle {
        TaskHandle {
            id: id.to_string(),
            inner: Arc::clone(&self.inner),
            app: self.app.clone(),
        }
    }

    /// Starts one operation and answers its flag. A second start under the same key replaces the first, which is what
    /// the window does when it retries an operation before the previous one ended. The record starts as `running` and
    /// is announced with `task-started`; an end that arrives after a replacement belongs to the newer run.
    pub fn start(
        &self,
        key: impl Into<TaskKey>,
        game: Option<String>,
        kind: &str,
    ) -> Arc<AtomicBool> {
        let key = key.into();
        let flag = Arc::new(AtomicBool::new(false));
        let view = TaskView {
            id: key.as_str().to_string(),
            game,
            kind: kind.to_string(),
            state: state::RUNNING.to_string(),
            percent: None,
            dedupe: key.dedupe().clone(),
            error: None,
        };
        {
            let mut inner = self.lock();
            inner
                .flags
                .insert(key.as_str().to_string(), Arc::clone(&flag));
            inner.tasks.insert(view.id.clone(), view.clone());
            self.persist(&inner);
            drop(inner);
        }

        self.emit(TASK_STARTED_EVENT, &view);
        flag
    }

    /// Starts work that waits for a user action instead of running. It gets a `waiting` row but no cancellation flag,
    /// so `cancel` cannot stop it; finishing it moves the row to its terminal state like any other task. The window
    /// announces it with the same `task-started` event and hydrates it from the snapshot after a restart.
    #[allow(
        dead_code,
        reason = "the backend waiting-row API for work paused on a user action; collection prompts use the session view today"
    )]
    pub fn start_waiting(&self, key: impl Into<TaskKey>, game: Option<String>, kind: &str) {
        let key = key.into();
        let view = TaskView {
            id: key.as_str().to_string(),
            game,
            kind: kind.to_string(),
            state: state::WAITING.to_string(),
            percent: None,
            dedupe: key.dedupe().clone(),
            error: None,
        };
        {
            let mut inner = self.lock();
            inner.tasks.insert(key.as_str().to_string(), view.clone());
            self.persist(&inner);
            drop(inner);
        }

        self.emit(TASK_STARTED_EVENT, &view);
    }

    /// Marks one running operation for cancellation and announces it. An unknown or already finished key is ignored,
    /// so a late click cannot rewrite the outcome the window has already seen. A waiting row has no flag and is left
    /// alone: the user acts on it instead of stopping it.
    pub fn cancel(&self, key: &str) {
        let view = {
            let mut inner = self.lock();

            if let Some(flag) = inner.flags.get(key) {
                flag.store(true, Ordering::Relaxed);
            }

            let view = inner.tasks.get_mut(key).and_then(|task| {
                if task.state != state::RUNNING {
                    return None;
                }

                task.state = state::CANCELLED.to_string();
                Some(task.clone())
            });
            self.persist(&inner);
            drop(inner);
            view
        };

        if let Some(view) = view {
            self.emit(TASK_CANCELLED_EVENT, &view);
        }
    }

    /// Removes the flag of a finished operation and records its terminal state. A task already cancelled keeps that
    /// state and does not get a second event; the window has heard the cancellation already. Terminal rows are removed
    /// from the journal, so a restart does not resurrect them.
    pub fn finish(&self, key: &str, state: &str) {
        self.finish_with(key, state, None);
    }

    /// [`Self::finish`] that keeps the reason of a failure on the record, so the window can say why.
    pub fn finish_with(&self, key: &str, state: &str, error: Option<&AppError>) {
        let finished = {
            let mut inner = self.lock();
            inner.flags.remove(key);
            let finished = inner.tasks.get_mut(key).and_then(|task| {
                if task.state == state::CANCELLED {
                    return None;
                }

                task.state = state.to_string();
                task.error = (state == state::FAILED)
                    .then_some(error.map(TaskError::from))
                    .flatten();
                Some(task.clone())
            });
            self.persist(&inner);
            drop(inner);
            finished
        };

        if let Some(view) = finished {
            self.emit(TASK_FINISHED_EVENT, &view);
        }
    }

    /// Forgets a failed or waiting task the user has read, also in the journal, so a restart does not show it again. A
    /// running task is left alone: it is stopped with [`Self::cancel`].
    pub fn dismiss(&self, key: &str) {
        let mut inner = self.lock();
        let finished = inner
            .tasks
            .get(key)
            .is_some_and(|task| task.state != state::RUNNING);

        if finished {
            inner.tasks.remove(key);
            self.persist(&inner);
        }

        drop(inner);
    }

    /// The tasks the window still needs to show, in a stable order: running work, waiting work and failures. Successful
    /// and cancelled work already has its terminal event, so a window reload must not resurrect it as a running row.
    /// The full record stays in memory until the app exits, but it is not listed again.
    pub fn list(&self) -> Vec<TaskView> {
        let mut tasks: Vec<TaskView> = self
            .lock()
            .tasks
            .values()
            .filter(|task| persisted(task))
            .cloned()
            .collect();
        tasks.sort_by(|left, right| left.id.cmp(&right.id));
        tasks
    }

    /// The stored record of one key, including terminal ones. Tests use it to check the outcome of a task that is no
    /// longer listed.
    #[cfg(test)]
    fn record(&self, key: &str) -> Option<TaskView> {
        self.lock().tasks.get(key).cloned()
    }

    /// Whether no cancellation flags were restored from the journal. A restored row can be retried, not cancelled.
    #[cfg(test)]
    fn flags_are_empty(&self) -> bool {
        self.lock().flags.is_empty()
    }

    /// Locks the registry. A poisoned mutex holds no partial state: the operations below only insert or remove one
    /// entry at a time, so continuing with the inner value is safe.
    fn lock(&self) -> MutexGuard<'_, Registry> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Publishes one lifecycle event. A missing listener (the window is reloading) must not fail the work.
    fn emit(&self, event: &str, view: &TaskView) {
        if let Some(app) = &self.app {
            let _ = app.emit(event, view);
        }
    }
}

/// A cloneable handle bound to one running task. A command's progress callback calls [`TaskHandle::progress`] to move
/// the global Working row forward without owning the Tauri `State` that registered the task.
#[derive(Clone)]
pub struct TaskHandle {
    id: String,
    inner: Arc<Mutex<Registry>>,
    /// Cloned from the registry owner; absent only in the unit tests.
    app: Option<AppHandle>,
}

impl TaskHandle {
    /// Records a percent for the bound task and announces it with `task-progress`. The work reports 0 through 100; a
    /// larger value is clamped. A task that is no longer running, or an unknown key, is ignored, so a late report cannot
    /// change a row the window has already settled. This never fails the command that calls it.
    pub fn progress(&self, percent: u32) {
        let view = self
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .tasks
            .get_mut(&self.id)
            .and_then(|task| {
                if task.state != state::RUNNING {
                    return None;
                }

                task.percent = Some(percent.min(100) as u8);
                Some(task.clone())
            });
        let Some(view) = view else {
            return;
        };

        if let Some(app) = &self.app {
            let _ = app.emit(TASK_PROGRESS_EVENT, &view);
        }
    }
}

/// Whether an optional flag asks the work to stop. The helper keeps the call sites to one line.
pub fn cancelled(flag: Option<&AtomicBool>) -> bool {
    flag.is_some_and(|flag| flag.load(Ordering::Relaxed))
}

/// Stops the work with the stable `code` when the flag asks for it: `stop_if(cancel, IMPORT_CANCELLED)?`.
pub fn stop_if(
    flag: Option<&AtomicBool>,
    code: &'static str,
) -> Result<(), crate::error::AppError> {
    if cancelled(flag) {
        Err(crate::error::AppError::user(code))
    } else {
        Ok(())
    }
}

/// Lists the tasks the window can see in the global Working panel. It answers with the same `CommandResult` shape as
/// the other read commands, so a reload can hydrate the panel through the shared IPC helper.
#[tauri::command]
#[specta::specta]
#[allow(
    clippy::unnecessary_wraps,
    reason = "the IPC shape must be a CommandResult so the frontend can hydrate through its shared unwrap helper"
)]
pub fn list_tasks(tasks: State<'_, TaskFlags>) -> Result<Vec<TaskView>, AppError> {
    Ok(tasks.list())
}

/// Stops a task by the id it was registered with. The id is the key of the stored cancellation flag, so the specific
/// cancel commands and this generic one share the same mechanism.
#[tauri::command]
#[specta::specta]
pub fn cancel_task(tasks: State<'_, TaskFlags>, id: String) {
    tasks.cancel(&id);
}

/// Forgets a failed or waiting task the user dismissed in the Working panel.
#[tauri::command]
#[specta::specta]
pub fn dismiss_task(tasks: State<'_, TaskFlags>, id: String) {
    tasks.dismiss(&id);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exported list pins the codes and their order, so a reindex cannot silently rename a named constant.
    #[test]
    fn task_kinds_are_the_codes_the_frontend_expects() {
        assert_eq!(
            TASK_KINDS,
            [
                "import",
                "update",
                "update-all",
                "requirement",
                "apply",
                "pack",
                "pack-link",
                "collection",
                "launch",
                "install",
                "loader",
            ]
        );
        let distinct: std::collections::HashSet<&str> = TASK_KINDS.iter().copied().collect();
        assert_eq!(distinct.len(), TASK_KINDS.len());
    }

    /// The builders keep the ids the running workflows already used, so no cancel command or journal changes.
    #[test]
    fn task_key_builders_keep_the_existing_ids() {
        assert_eq!(TaskKey::apply("game-a").as_str(), "apply:game-a");
        assert_eq!(
            TaskKey::import("game-a", "/mods/a.zip").as_str(),
            "import:game-a\n/mods/a.zip"
        );
        assert_eq!(
            TaskKey::update_mod("game-a", "CET").as_str(),
            "update-mod:game-a\nCET"
        );
        assert_eq!(TaskKey::update_all("game-a").as_str(), "update:game-a");
        assert_eq!(
            TaskKey::requirement("game-a").as_str(),
            "requirement:game-a"
        );
        assert_eq!(
            TaskKey::collection_install("game-a", "profile", "x").as_str(),
            "collection:game-a\nprofile\nx"
        );
        assert_eq!(
            TaskKey::pack_import("game-a", "recipe.zip").as_str(),
            "pack:game-a\nrecipe.zip"
        );
        assert_eq!(
            TaskKey::pack_download("game-a", "recipe.zip").as_str(),
            "pack-download:game-a\nrecipe.zip"
        );
        assert_eq!(
            TaskKey::pack_link("game-a", "nxm://link").as_str(),
            "pack-link:game-a\nnxm://link"
        );
    }

    /// Each builder sets the discriminator the window groups rows by, not just the string id.
    #[test]
    fn task_key_builders_set_the_typed_dedupe() {
        assert_eq!(
            TaskKey::apply("game-a").dedupe(),
            &TaskDedupe::Apply {
                game: "game-a".to_string()
            }
        );
        assert_eq!(
            TaskKey::import("game-a", "/mods/a.zip").dedupe(),
            &TaskDedupe::Import {
                game: "game-a".to_string(),
                path: "/mods/a.zip".to_string()
            }
        );
        assert_eq!(
            TaskKey::update_mod("game-a", "CET").dedupe(),
            &TaskDedupe::Update {
                game: "game-a".to_string()
            }
        );
        assert_eq!(
            TaskKey::update_all("game-a").dedupe(),
            &TaskDedupe::UpdateAll {
                game: "game-a".to_string()
            }
        );
        assert_eq!(
            TaskKey::requirement("game-a").dedupe(),
            &TaskDedupe::Requirement {
                game: "game-a".to_string()
            }
        );
        assert_eq!(
            TaskKey::collection_install("game-a", "profile", "x").dedupe(),
            &TaskDedupe::Collection {
                game: "game-a".to_string(),
                profile: "profile".to_string(),
                collection: "x".to_string()
            }
        );
        assert_eq!(
            TaskKey::pack_import("game-a", "recipe.zip").dedupe(),
            &TaskDedupe::Pack {
                game: "game-a".to_string(),
                path: "recipe.zip".to_string()
            }
        );
        assert_eq!(
            TaskKey::pack_download("game-a", "recipe.zip").dedupe(),
            &TaskDedupe::Pack {
                game: "game-a".to_string(),
                path: "recipe.zip".to_string()
            }
        );
        assert_eq!(
            TaskKey::pack_link("game-a", "nxm://link").dedupe(),
            &TaskDedupe::PackLink {
                game: "game-a".to_string(),
                link: "nxm://link".to_string()
            }
        );
    }

    /// `run_task` passes a plain string to the registry, so the typed identity must survive that round trip unchanged.
    #[test]
    fn a_task_key_survives_the_string_round_trip() {
        let keys = [
            TaskKey::apply("game-a"),
            TaskKey::import("game-a", "/mods/a.zip"),
            TaskKey::update_mod("game-a", "CET"),
            TaskKey::update_all("game-a"),
            TaskKey::requirement("game-a"),
            TaskKey::collection_install("game-a", "profile", "x"),
            TaskKey::pack_import("game-a", "recipe.zip"),
            TaskKey::pack_download("game-a", "recipe.zip"),
            TaskKey::pack_link("game-a", "nxm://link"),
            TaskKey::tool(kind::LAUNCH, "game-a", "xedit"),
            TaskKey::tool(kind::INSTALL, "game-a", "xedit"),
        ];

        for key in keys {
            let rebuilt = TaskKey::from(key.as_str().to_string());
            assert_eq!(rebuilt.as_str(), key.as_str());
            assert_eq!(rebuilt.dedupe(), key.dedupe());
        }
    }

    /// A pack import and a pack download keep separate flags but share the dedupe group, so only one Working row shows.
    #[test]
    fn a_pack_import_and_its_download_share_a_dedupe_group_but_not_a_flag() {
        let import = TaskKey::pack_import("game-a", "recipe.zip");
        let download = TaskKey::pack_download("game-a", "recipe.zip");
        assert_ne!(import.as_str(), download.as_str());
        assert_eq!(import.dedupe(), download.dedupe());
    }

    /// Work with no builder, such as an older journal row, still gets a typed value the window can handle.
    #[test]
    fn an_unknown_id_gets_the_other_dedupe() {
        assert_eq!(
            TaskKey::from("never-started".to_string()).dedupe(),
            &TaskDedupe::Other
        );
    }

    #[test]
    fn a_flag_starts_clear_and_cancels_once() {
        let tasks = TaskFlags::default();
        let flag = tasks.start(
            "import:game:archive.zip".to_string(),
            Some("game".to_string()),
            kind::IMPORT,
        );
        assert!(!cancelled(Some(flag.as_ref())));

        tasks.cancel("import:game:archive.zip");
        assert!(cancelled(Some(flag.as_ref())));

        tasks.finish("import:game:archive.zip", state::FAILED);
        // The old Arc keeps its state; a cancel of a finished operation cannot reach it.
        assert!(cancelled(Some(flag.as_ref())));
    }

    #[test]
    fn a_new_start_under_the_same_key_is_clear() {
        let tasks = TaskFlags::default();
        let first = tasks.start(
            "update:game".to_string(),
            Some("game".to_string()),
            kind::UPDATE,
        );
        tasks.cancel("update:game");
        tasks.finish("update:game", state::FAILED);

        let second = tasks.start(
            "update:game".to_string(),
            Some("game".to_string()),
            kind::UPDATE,
        );
        assert!(cancelled(Some(first.as_ref())));
        assert!(!cancelled(Some(second.as_ref())));
    }

    #[test]
    fn start_lists_a_running_task_and_finish_removes_a_successful_one() {
        let tasks = TaskFlags::default();
        tasks.start(
            "apply:game".to_string(),
            Some("game".to_string()),
            kind::APPLY,
        );
        assert_eq!(
            tasks.list(),
            [TaskView {
                id: "apply:game".to_string(),
                game: Some("game".to_string()),
                kind: kind::APPLY.to_string(),
                state: state::RUNNING.to_string(),
                percent: None,
                dedupe: TaskDedupe::Apply {
                    game: "game".to_string(),
                },
                error: None,
            }]
        );

        tasks.finish("apply:game", state::DONE);
        assert!(tasks.list().is_empty());
        assert_eq!(
            tasks.record("apply:game").map(|task| task.state),
            Some(state::DONE.to_string())
        );
    }

    #[test]
    fn a_failure_stays_listed_for_the_panel() {
        let tasks = TaskFlags::default();
        tasks.start(
            "apply:game".to_string(),
            Some("game".to_string()),
            kind::APPLY,
        );
        tasks.finish("apply:game", state::FAILED);

        let tasks_view = tasks.list();
        assert_eq!(tasks_view.len(), 1);
        assert_eq!(tasks_view[0].state, state::FAILED);
    }

    #[test]
    fn cancel_changes_the_record_and_finish_does_not_overwrite_it() {
        let tasks = TaskFlags::default();
        tasks.start(
            "collection:game:profile:mods".to_string(),
            Some("game".to_string()),
            kind::COLLECTION,
        );
        tasks.cancel("collection:game:profile:mods");
        tasks.finish("collection:game:profile:mods", state::FAILED);

        assert!(tasks.list().is_empty());
        assert_eq!(
            tasks
                .record("collection:game:profile:mods")
                .map(|task| task.state),
            Some(state::CANCELLED.to_string())
        );
    }

    #[test]
    fn a_cancel_after_finish_leaves_the_outcome_alone() {
        let tasks = TaskFlags::default();
        tasks.start(
            "import:game:archive.zip".to_string(),
            Some("game".to_string()),
            kind::IMPORT,
        );
        tasks.finish("import:game:archive.zip", state::DONE);
        tasks.cancel("import:game:archive.zip");

        assert_eq!(
            tasks
                .record("import:game:archive.zip")
                .map(|task| task.state),
            Some(state::DONE.to_string())
        );
    }

    #[test]
    fn cancelling_an_unknown_key_is_a_no_op() {
        let tasks = TaskFlags::default();
        tasks.cancel("never-started");
        assert!(tasks.list().is_empty());
    }

    #[test]
    fn a_second_start_replaces_the_record_of_the_first() {
        let tasks = TaskFlags::default();
        tasks.start("update:game".to_string(), None, kind::UPDATE);
        tasks.start("update:game".to_string(), None, kind::UPDATE_ALL);

        let tasks_view = tasks.list();
        assert_eq!(tasks_view.len(), 1);
        assert_eq!(tasks_view[0].kind, kind::UPDATE_ALL);
        assert_eq!(tasks_view[0].game, None);
        tasks.finish("update:game", state::DONE);
        assert_eq!(
            tasks.record("update:game").map(|task| task.kind),
            Some(kind::UPDATE_ALL.to_string())
        );
    }

    #[test]
    fn a_handle_reports_progress_only_for_its_own_running_task() {
        let tasks = TaskFlags::default();
        tasks.start("job-one".to_string(), None, kind::IMPORT);
        tasks.start("job-two".to_string(), None, kind::UPDATE);
        let handle = tasks.handle("job-one");

        handle.progress(40);
        assert_eq!(
            tasks.record("job-one").and_then(|task| task.percent),
            Some(40)
        );
        assert_eq!(tasks.record("job-two").and_then(|task| task.percent), None);
        assert_eq!(tasks.list()[0].percent, Some(40));
        assert_eq!(tasks.list()[1].percent, None);
    }

    #[test]
    fn reported_progress_is_clamped_to_a_byte() {
        let tasks = TaskFlags::default();
        tasks.start("job".to_string(), None, kind::APPLY);
        let handle = tasks.handle("job");

        handle.progress(u32::MAX);
        assert_eq!(tasks.record("job").and_then(|task| task.percent), Some(100));
    }

    #[test]
    fn progress_after_a_task_ends_cannot_revive_its_row() {
        let tasks = TaskFlags::default();
        tasks.start("job".to_string(), None, kind::APPLY);
        let handle = tasks.handle("job");
        tasks.cancel("job");

        handle.progress(60);
        assert_eq!(tasks.record("job").and_then(|task| task.percent), None);
        assert_eq!(
            tasks.record("job").map(|task| task.state),
            Some(state::CANCELLED.to_string())
        );
    }

    #[test]
    fn a_handle_for_an_unknown_key_is_a_no_op() {
        let tasks = TaskFlags::default();
        tasks.handle("never-started").progress(50);
        assert!(tasks.list().is_empty());
    }

    /// A registry with a journal in a scratch folder, standing in for one app start.
    fn with_journal(name: &str) -> (PathBuf, TaskFlags) {
        let dir = crate::test_support::scratch!(name);
        let tasks = TaskFlags::with_journal(dir.clone());
        (dir, tasks)
    }

    #[test]
    fn a_journal_round_trip_marks_interrupted_work_as_a_failure() {
        let (dir, tasks) = with_journal("tasks-journal-interrupted");
        tasks.start(
            "apply:game".to_string(),
            Some("game".to_string()),
            kind::APPLY,
        );
        tasks.start_waiting(
            "collection:game:p1:c".to_string(),
            Some("game".to_string()),
            kind::COLLECTION,
        );
        tasks.start(
            "import:game:a.zip".to_string(),
            Some("game".to_string()),
            kind::IMPORT,
        );
        tasks.finish("import:game:a.zip", state::FAILED);
        drop(tasks);

        // A new registry over the same folder stands in for the next start.
        let restarted = TaskFlags::with_journal(dir);
        let rows = restarted.list();
        assert_eq!(rows.len(), 3);
        let by_id: HashMap<String, TaskView> =
            rows.into_iter().map(|row| (row.id.clone(), row)).collect();
        assert_eq!(
            by_id.get("apply:game").map(|row| row.state.as_str()),
            Some(state::FAILED),
            "interrupted running work is a failure with a stable state code"
        );
        assert_eq!(
            by_id
                .get("apply:game")
                .and_then(|row| row.error.as_ref())
                .map(|error| error.code.as_str()),
            Some(INTERRUPTED),
            "interrupted work says why it failed"
        );
        assert_eq!(
            by_id
                .get("collection:game:p1:c")
                .map(|row| row.state.as_str()),
            Some(state::WAITING),
            "waiting work survives a restart unchanged"
        );
        assert_eq!(
            by_id.get("import:game:a.zip").map(|row| row.state.as_str()),
            Some(state::FAILED)
        );
        assert!(
            restarted.flags_are_empty(),
            "no cancellation flag is restored for work of an earlier process"
        );
    }

    #[test]
    fn a_failure_keeps_its_reason_until_the_user_dismisses_it() {
        let (dir, tasks) = with_journal("tasks-journal-dismiss");
        tasks.start(
            "import:game:a.zip".to_string(),
            Some("game".to_string()),
            kind::IMPORT,
        );
        tasks.finish_with(
            "import:game:a.zip",
            state::FAILED,
            Some(&AppError::user("import.archive_damaged").with("file", "a.zip")),
        );
        let error = tasks.list()[0].error.clone().unwrap();
        assert_eq!(error.code, "import.archive_damaged");
        assert_eq!(error.params.get("file").map(String::as_str), Some("a.zip"));

        tasks.dismiss("import:game:a.zip");
        assert!(tasks.list().is_empty());
        drop(tasks);
        assert!(
            TaskFlags::with_journal(dir).list().is_empty(),
            "a dismissed failure does not come back after a restart"
        );
    }

    #[test]
    fn a_running_task_cannot_be_dismissed() {
        let tasks = TaskFlags::default();
        tasks.start("apply:game".to_string(), None, kind::APPLY);
        tasks.dismiss("apply:game");
        assert_eq!(tasks.list().len(), 1);
    }

    #[test]
    fn a_finished_task_is_not_restored_after_a_restart() {
        let (dir, tasks) = with_journal("tasks-journal-finished");
        tasks.start(
            "apply:game".to_string(),
            Some("game".to_string()),
            kind::APPLY,
        );
        tasks.finish("apply:game", state::DONE);
        drop(tasks);

        assert!(TaskFlags::with_journal(dir).list().is_empty());
    }

    #[test]
    fn a_waiting_start_is_listed_but_has_no_cancel_flag() {
        let tasks = TaskFlags::default();
        tasks.start_waiting(
            "waiting:game".to_string(),
            Some("game".to_string()),
            kind::COLLECTION,
        );

        let rows = tasks.list();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, state::WAITING);
        assert!(tasks.record("waiting:game").is_some());
        // Cancel cannot reach waiting work, and finishing it removes the row from the panel.
        tasks.cancel("waiting:game");
        assert_eq!(tasks.list()[0].state, state::WAITING);
        tasks.finish("waiting:game", state::DONE);
        assert!(tasks.list().is_empty());
    }
}
