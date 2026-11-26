//! The cancellation flags, task records and cloneable progress handles of one running app.

use std::collections::HashMap;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppError;

use super::events::{
    TASK_CANCELLED_EVENT, TASK_FINISHED_EVENT, TASK_PROGRESS_EVENT, TASK_STARTED_EVENT,
};
use super::journal::{TaskJournal, persisted};
use super::key::TaskKey;
use super::model::{TaskError, TaskView, state};

/// Publishes one lifecycle event. A missing listener (the window is reloading) must not fail the work.
fn emit(app: Option<&AppHandle>, event: &str, view: &TaskView) {
    if let Some(app) = app {
        let _ = app.emit(event, view);
    }
}

#[derive(Default)]
struct Registry {
    /// The flags named by the keys the existing cancel commands use.
    flags: HashMap<String, Arc<AtomicBool>>,
    /// The last record for each key, including terminal ones, so a cancel that arrives after the work cannot revive
    /// a finished task and a test can inspect the outcome.
    tasks: HashMap<String, TaskView>,
    /// The generation of the newest run of each key. Work keeps the generation its handle was made for, so a
    /// replacement start makes the old run's completion a no-op.
    generations: HashMap<String, u64>,
    /// The on-disk journal, held here so both the flags and a handle can persist a terminal row. Absent in tests that
    /// need no disk.
    journal: Option<TaskJournal>,
}

impl Registry {
    /// Starts a new generation of `id` and asks the previous run, if any, to stop. Returns the new generation. The
    /// caller holds the guard.
    fn next_generation(&mut self, id: &str) -> u64 {
        let generation = self.generations.entry(id.to_string()).or_insert(0);
        *generation += 1;
        if let Some(previous) = self.flags.get(id) {
            previous.store(true, Ordering::Relaxed);
        }
        *generation
    }

    /// Writes the rows that must survive a restart. The caller holds the guard, so two changes cannot land their
    /// snapshots in the wrong order. The file is small and is replaced atomically.
    fn persist(&self) {
        if let Some(journal) = &self.journal {
            let rows: Vec<TaskView> = self
                .tasks
                .values()
                .filter(|task| persisted(task))
                .cloned()
                .collect();
            journal.save(&rows);
        }
    }
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
}

impl Default for TaskFlags {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Registry::default())),
            app: None,
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
                journal,
                ..Registry::default()
            })),
            app: Some(app),
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
                journal: Some(journal),
                ..Registry::default()
            })),
            app: None,
        }
    }

    /// A cloneable handle bound to one task key and the generation that is current now. A progress callback or a
    /// completion reports through it without owning this state; an unknown key is a no-op.
    #[cfg(test)]
    pub fn handle(&self, id: &str) -> TaskHandle {
        let generation = self.lock().generations.get(id).copied().unwrap_or(0);
        TaskHandle {
            id: id.to_string(),
            generation,
            inner: Arc::clone(&self.inner),
            app: self.app.clone(),
        }
    }

    /// Starts one operation and answers its flag. A second start under the same key replaces the first, asks it to
    /// stop and makes its handle stale, which is what the window does when it retries before the previous run ended.
    /// The record starts as `running` and is announced with `task-started`.
    #[cfg(test)]
    pub fn start(
        &self,
        key: impl Into<TaskKey>,
        game: Option<String>,
        kind: &str,
    ) -> Arc<AtomicBool> {
        self.start_with_handle(key, game, kind).0
    }

    /// Starts one operation and answers its flag together with a handle bound to the generation created here. The
    /// flag, generation, handle and row are made under one guard, so a start that wins between this call and a later
    /// [`Self::handle`] cannot make the handle settle the newer run. As with [`Self::start`], a second start under the
    /// same key replaces the first, asks it to stop and makes its handle stale. The record starts as `running` and is
    /// announced with `task-started`.
    pub fn start_with_handle(
        &self,
        key: impl Into<TaskKey>,
        game: Option<String>,
        kind: &str,
    ) -> (Arc<AtomicBool>, TaskHandle) {
        let key = key.into();
        let id = key.as_str().to_string();
        let flag = Arc::new(AtomicBool::new(false));
        let view = TaskView {
            id: id.clone(),
            game,
            kind: kind.to_string(),
            state: state::RUNNING.to_string(),
            percent: None,
            dedupe: key.dedupe().clone(),
            error: None,
        };
        let handle = {
            let mut inner = self.lock();
            let generation = inner.next_generation(&id);
            inner.flags.insert(id, Arc::clone(&flag));
            inner.tasks.insert(view.id.clone(), view.clone());
            inner.persist();
            drop(inner);
            TaskHandle {
                id: view.id.clone(),
                generation,
                inner: Arc::clone(&self.inner),
                app: self.app.clone(),
            }
        };

        self.emit(TASK_STARTED_EVENT, &view);
        (flag, handle)
    }

    /// Starts work that waits for a user action instead of running. It gets a `waiting` row but no cancellation flag,
    /// so `cancel` cannot stop it; finishing it moves the row to its terminal state like any other task. Replacing a
    /// run still advances the generation and stops the previous flag, so its late completion is ignored. The window
    /// announces it with the same `task-started` event and hydrates it from the snapshot after a restart.
    #[cfg(test)]
    pub fn start_waiting(&self, key: impl Into<TaskKey>, game: Option<String>, kind: &str) {
        let key = key.into();
        let id = key.as_str().to_string();
        let view = TaskView {
            id: id.clone(),
            game,
            kind: kind.to_string(),
            state: state::WAITING.to_string(),
            percent: None,
            dedupe: key.dedupe().clone(),
            error: None,
        };
        {
            let mut inner = self.lock();
            inner.next_generation(&id);
            inner.tasks.insert(id, view.clone());
            inner.persist();
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
            inner.persist();
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
    #[cfg(test)]
    pub fn finish(&self, key: &str, state: &str) {
        self.finish_with(key, state, None);
    }

    /// [`Self::finish`] that keeps the reason of a failure on the record, so the window can say why. It settles the
    /// current run; work that may have been replaced uses [`TaskHandle::finish_with`] instead.
    #[cfg(test)]
    pub fn finish_with(&self, key: &str, state: &str, error: Option<&AppError>) {
        self.handle(key).finish_with(state, error);
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
            inner.persist();
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
    pub(super) fn record(&self, key: &str) -> Option<TaskView> {
        self.lock().tasks.get(key).cloned()
    }

    /// Whether no cancellation flags were restored from the journal. A restored row can be retried, not cancelled.
    #[cfg(test)]
    pub(super) fn flags_are_empty(&self) -> bool {
        self.lock().flags.is_empty()
    }

    /// Locks the registry. A poisoned mutex holds no partial state: the operations below only insert or remove one
    /// entry at a time, so continuing with the inner value is safe.
    fn lock(&self) -> MutexGuard<'_, Registry> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Publishes one lifecycle event. A missing listener (the window is reloading) must not fail the work.
    fn emit(&self, event: &str, view: &TaskView) {
        emit(self.app.as_ref(), event, view);
    }
}

/// A cloneable handle bound to one run of a task. Progress and completion go through it without owning the Tauri
/// `State`; a completion from a run that a newer start replaced is a no-op.
#[derive(Clone)]
pub struct TaskHandle {
    id: String,
    generation: u64,
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
        if let Some(view) = view {
            self.emit(TASK_PROGRESS_EVENT, &view);
        }
    }

    /// Settles this run of the task. A run replaced by a newer start is a no-op: no event and no record change.
    pub fn finish(&self, state: &str) {
        self.finish_with(state, None);
    }

    /// [`Self::finish`] that keeps the reason of a failure on the record, so the window can say why.
    pub fn finish_with(&self, state: &str, error: Option<&AppError>) {
        let finished = {
            let mut inner = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
            if inner.generations.get(&self.id).copied().unwrap_or(0) != self.generation {
                return;
            }
            inner.flags.remove(&self.id);
            let finished = inner.tasks.get_mut(&self.id).and_then(|task| {
                if task.state == state::CANCELLED {
                    return None;
                }
                task.state = state.to_string();
                task.error = (state == state::FAILED)
                    .then_some(error.map(TaskError::from))
                    .flatten();
                Some(task.clone())
            });
            inner.persist();
            finished
        };

        if let Some(view) = finished {
            self.emit(TASK_FINISHED_EVENT, &view);
        }
    }

    /// Publishes one lifecycle event. A missing listener (the window is reloading) must not fail the work.
    fn emit(&self, event: &str, view: &TaskView) {
        emit(self.app.as_ref(), event, view);
    }
}
