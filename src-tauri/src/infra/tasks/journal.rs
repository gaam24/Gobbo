//! The task journal in the app data folder and the rule for what survives a restart.

use std::path::PathBuf;

use crate::infra::json;

use super::model::{TASKS_FILE, TaskError, TaskView, state};

/// The reason of a task that was still running when Gobbo stopped. The work cannot continue after a restart.
pub(super) const INTERRUPTED: &str = "tasks.interrupted";

/// The on-disk rows of work that outlives one session: running work, waiting work and failures. Successful and
/// cancelled work already has its terminal event, so it is dropped from the journal on the next change.
pub(super) struct TaskJournal {
    path: PathBuf,
}

impl TaskJournal {
    /// The journal of the app data folder `dir`.
    pub(super) fn new(dir: PathBuf) -> Self {
        Self {
            path: dir.join(TASKS_FILE),
        }
    }

    /// Loads the rows and turns work that was still running when the app stopped into failures, so a restart offers a
    /// Retry instead of showing work that can never report progress again. A missing or damaged journal is empty; a
    /// failed read is a diagnostic, never a reason to refuse to start the app.
    pub(super) fn load(&self) -> Vec<TaskView> {
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
    pub(super) fn save(&self, rows: &[TaskView]) {
        if let Err(error) = json::write_json(&self.path, &rows) {
            crate::infra::log::warn(format!(
                "could not save the task journal {}: {error}",
                self.path.display()
            ));
        }
    }
}

/// Whether a record still belongs in the journal after a restart.
pub(super) fn persisted(task: &TaskView) -> bool {
    task.state == state::RUNNING || task.state == state::FAILED || task.state == state::WAITING
}
