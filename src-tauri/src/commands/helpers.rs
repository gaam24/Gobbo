//! The shared helpers behind the Tauri commands: blocking work, per-game commands and task bookkeeping. The command
//! modules call these instead of reaching through the command list.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use tauri::State;
use tauri::ipc::{Channel, IpcResponse};

use crate::context::Data;
use crate::error::AppError;
use crate::infra::tasks as task_commands;
use crate::infra::tasks::TaskKey;
use crate::status::views::KnownRows;
use crate::status::views::Status;

/// Runs `work` on a blocking thread and waits for it without blocking the window.
pub async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(AppError::unexpected)?
}

/// Runs `work` for one game with the data folder.
pub async fn for_game<T: Send + 'static>(
    data: State<'_, Data>,
    game: String,
    work: impl FnOnce(&Data, &str) -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    let root = data.inner().clone();
    blocking(move || work(&root, &game)).await
}

/// Runs one long operation for one game as a task. It starts the task record under `key`, runs `work` on a blocking
/// thread with the task's cancel flag and progress handle, and then settles the record it started with
/// `task_state_for_failure`'s rule. A retry that replaced this run leaves the newer row alone: the handle is bound to
/// the generation `start` created, so a stale completion is a no-op. A cancelled task keeps its state.
pub async fn run_task<T: Send + 'static>(
    data: State<'_, Data>,
    tasks: &task_commands::TaskFlags,
    key: TaskKey,
    game: String,
    kind: &'static str,
    work: impl FnOnce(&Data, &str, &AtomicBool, task_commands::TaskHandle) -> Result<T, AppError>
    + Send
    + 'static,
) -> Result<T, AppError> {
    start_task(
        data,
        tasks,
        key,
        game,
        kind,
        move |root, game, flag, handle| work(root, game, &flag, handle),
    )
    .await
}

/// Starts a task and runs its setup on a blocking thread, answering with the result and a handle bound to the run that
/// was started. The record is left running on success: work that outlives setup finishes its own record later.
async fn start_detached<T: Send + 'static>(
    data: State<'_, Data>,
    tasks: &task_commands::TaskFlags,
    key: TaskKey,
    game: String,
    kind: &'static str,
    start: impl FnOnce(&Data, &str, Arc<AtomicBool>, task_commands::TaskHandle) -> Result<T, AppError>
    + Send
    + 'static,
) -> (Result<T, AppError>, task_commands::TaskHandle) {
    let (flag, handle) = tasks.start_with_handle(key, Some(game.clone()), kind);
    let settle = handle.clone();
    let result = for_game(data, game, move |root, game| {
        start(root, game, flag, handle)
    })
    .await;
    (result, settle)
}

/// Registers a long operation and runs its setup on a blocking thread. The record is left running on success: work
/// that outlives setup — a tool waiting for its program — finishes its own record from the background thread. A setup
/// failure ends the record here, because no background work was started.
pub async fn start_detached_task<T: Send + 'static>(
    data: State<'_, Data>,
    tasks: &task_commands::TaskFlags,
    key: TaskKey,
    game: String,
    kind: &'static str,
    start: impl FnOnce(&Data, &str, Arc<AtomicBool>, task_commands::TaskHandle) -> Result<T, AppError>
    + Send
    + 'static,
) -> Result<T, AppError> {
    let (result, settle) = start_detached(data, tasks, key, game, kind, start).await;
    if let Err(ref error) = result {
        settle.finish_with(task_state_for_failure(error), Some(error));
    }
    result
}

/// The terminal state of work that answered with an error. A stop keeps its cancelled state. An archive already in
/// the library did its work: the window still gets the explanation to show, but Working ends the row successfully
/// instead of leaving a failure the user cannot retry into a different result.
fn task_state_for_failure(error: &AppError) -> &'static str {
    if error.code == crate::import::ALREADY_IN_LIBRARY {
        task_commands::state::DONE
    } else if error.is_cancelled() {
        task_commands::state::CANCELLED
    } else {
        task_commands::state::FAILED
    }
}

/// Registers one long operation, runs it on a blocking thread and ends its record when it returns. Use for work that
/// finishes inside `work`; work that outlives it uses [`start_detached_task`].
pub async fn start_task<T: Send + 'static>(
    data: State<'_, Data>,
    tasks: &task_commands::TaskFlags,
    key: TaskKey,
    game: String,
    kind: &'static str,
    work: impl FnOnce(&Data, &str, Arc<AtomicBool>, task_commands::TaskHandle) -> Result<T, AppError>
    + Send
    + 'static,
) -> Result<T, AppError> {
    let (result, settle) = start_detached(data, tasks, key, game, kind, work).await;
    match &result {
        Ok(_) => settle.finish(task_commands::state::DONE),
        Err(error) => settle.finish_with(task_state_for_failure(error), Some(error)),
    }
    result
}

/// The progress callback of a command: each step goes to the window's channel. A step that cannot be sent (the window
/// closed) is dropped, and the work still finishes.
pub fn channel_sink<T: IpcResponse>(channel: Channel<T>) -> impl FnMut(T) {
    move |step| {
        let _ = channel.send(step);
    }
}

/// A progress callback that also advances the global Working row. The step still goes to the window's channel as
/// before; the percent is reported to the task registry, so a reload of the panel shows real progress. A send failure
/// is ignored and reporting progress never fails the command.
pub fn channel_sink_with_progress<T, F>(
    channel: Channel<T>,
    handle: task_commands::TaskHandle,
    percent: F,
) -> impl FnMut(T)
where
    T: IpcResponse,
    F: Fn(&T) -> u32,
{
    let mut send = channel_sink(channel);
    move |step| {
        handle.progress(percent(&step));
        send(step);
    }
}

/// Runs `work` with the data folder on a blocking thread.
pub async fn with_data<T: Send + 'static>(
    data: State<'_, Data>,
    work: impl FnOnce(&Data) -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    let root = data.inner().clone();
    blocking(move || work(&root)).await
}

/// Changes one game with `work`, then answers with the game's status.
pub async fn change<T>(
    data: State<'_, Data>,
    game: String,
    work: impl FnOnce(&Data, &str) -> Result<T, AppError> + Send + 'static,
) -> Result<Status, AppError> {
    change_patch(data, game, None, work).await
}

/// Changes one game with `work`, then answers with the status. When the window sends the row revisions it holds, the
/// answer carries only the rows that changed, the removals and the new order.
pub async fn change_patch<T>(
    data: State<'_, Data>,
    game: String,
    known: Option<KnownRows>,
    work: impl FnOnce(&Data, &str) -> Result<T, AppError> + Send + 'static,
) -> Result<Status, AppError> {
    for_game(data, game, move |root, game| {
        work(root, game)?;
        let status = crate::status::status(root, game)?;
        Ok(match known {
            Some(known) => crate::status::patch(status, &known),
            None => status,
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_duplicate_archive_ends_the_task_without_a_failure_row() {
        let duplicate = AppError::new(crate::import::ALREADY_IN_LIBRARY);
        assert_eq!(
            task_state_for_failure(&duplicate),
            task_commands::state::DONE
        );
    }

    #[test]
    fn a_stop_keeps_its_cancelled_state_and_other_errors_fail() {
        let stopped = AppError::new("import.cancelled");
        assert_eq!(
            task_state_for_failure(&stopped),
            task_commands::state::CANCELLED
        );

        let broken = AppError::new("import.bad_archive");
        assert_eq!(
            task_state_for_failure(&broken),
            task_commands::state::FAILED
        );
    }
}
