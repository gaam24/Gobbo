//! The Tauri commands: the whole interface of the backend to the window. Each one is a thin call into the module that
//! owns the use case.
//!
//! Every command runs its work through `blocking`: Tauri runs synchronous commands on the main thread, and these all
//! touch the disk or the network, so the window never waits for them. A command that changes a game answers with the
//! game's fresh status (`change`), so the interface never needs a second call to see the result.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::collections::commands as collections;
use crate::deploy::commands as deploy;
use crate::diagnostics::commands as diagnostics;
use crate::games::commands as games;
use crate::import::commands as import_commands;
use crate::launch::commands as launch;
use crate::mods::commands as mods;
use crate::overwrite::commands as overwrite;
use crate::packs::commands as packs;
use crate::profiles::commands as profiles;
use crate::settings::commands as settings;
use crate::snapshots::commands as snapshots;
use crate::sources::commands as sources;
use crate::tray::commands as tray;
use crate::update::commands as updates;

use tauri::State;
use tauri::ipc::{Channel, IpcResponse};
use tauri_specta::{Builder, collect_commands};

use crate::context::Data;
use crate::error::AppError;
use crate::infra::tasks as task_commands;
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
/// thread with the task's cancel flag and progress handle, and then finishes the record with `task_state_for_failure`'s
/// rule. A task the user cancelled keeps its cancelled state, which `TaskFlags::finish` already guarantees.
pub async fn run_task<T: Send + 'static>(
    data: State<'_, Data>,
    tasks: &task_commands::TaskFlags,
    key: String,
    game: String,
    kind: &'static str,
    work: impl FnOnce(&Data, &str, &AtomicBool, task_commands::TaskHandle) -> Result<T, AppError>
    + Send
    + 'static,
) -> Result<T, AppError> {
    let flag = tasks.start(key.clone(), Some(game.clone()), kind);
    let handle = tasks.handle(&key);
    let result = for_game(data, game, move |root, game| {
        work(root, game, flag.as_ref(), handle)
    })
    .await;
    match &result {
        Ok(_) => tasks.finish(&key, task_commands::state::DONE),
        Err(error) => tasks.finish_with(&key, task_state_for_failure(error), Some(error)),
    }
    result
}

/// Registers a long operation and runs its setup on a blocking thread. The record is left running on success: work
/// that outlives setup — a tool waiting for its program — finishes its own record from the background thread. A setup
/// failure ends the record here, because no background work was started.
pub async fn start_detached_task<T: Send + 'static>(
    data: State<'_, Data>,
    tasks: &task_commands::TaskFlags,
    key: String,
    game: String,
    kind: &'static str,
    start: impl FnOnce(&Data, &str, Arc<AtomicBool>, task_commands::TaskHandle) -> Result<T, AppError>
    + Send
    + 'static,
) -> Result<T, AppError> {
    let flag = tasks.start(key.clone(), Some(game.clone()), kind);
    let handle = tasks.handle(&key);
    let result = for_game(data, game, move |root, game| {
        start(root, game, flag, handle)
    })
    .await;

    if let Err(ref error) = result {
        tasks.finish_with(&key, task_state_for_failure(error), Some(error));
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
    key: String,
    game: String,
    kind: &'static str,
    work: impl FnOnce(&Data, &str, Arc<AtomicBool>, task_commands::TaskHandle) -> Result<T, AppError>
    + Send
    + 'static,
) -> Result<T, AppError> {
    let done_key = key.clone();
    let result = start_detached_task(data, tasks, key, game, kind, work).await;

    if result.is_ok() {
        tasks.finish(&done_key, task_commands::state::DONE);
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

/// Defines the commands that run one use case for one game, the shape most commands have. Each takes the game id and
/// its own arguments, and runs on a blocking thread:
///
/// - `query name(args) -> Answer = work;` answers with what `work` returns;
/// - `change name(args) = work;` answers with the game's fresh status after `work`.
///
/// `work` is a function of `(&Data, &str)`, or `|root, game| body` with the arguments in scope. A command that needs
/// more (the app handle, a channel, another state) is written out as a plain function.
macro_rules! game_commands {
    () => {};
    (
        $(#[$doc:meta])*
        query $name:ident($($arg:ident: $ty:ty),* $(,)?) -> $out:ty = |$root:ident, $game:ident| $body:expr;
        $($rest:tt)*
    ) => {
        $(#[$doc])*
        #[tauri::command]
        #[specta::specta]
        pub async fn $name(
            data: tauri::State<'_, $crate::context::Data>,
            game: String,
            $($arg: $ty),*
        ) -> Result<$out, $crate::error::AppError> {
            $crate::commands::for_game(data, game, move |$root, $game| $body).await
        }

        $crate::commands::game_commands!($($rest)*);
    };
    ($(#[$doc:meta])* query $name:ident() -> $out:ty = $work:path; $($rest:tt)*) => {
        $(#[$doc])*
        #[tauri::command]
        #[specta::specta]
        pub async fn $name(
            data: tauri::State<'_, $crate::context::Data>,
            game: String,
        ) -> Result<$out, $crate::error::AppError> {
            $crate::commands::for_game(data, game, $work).await
        }

        $crate::commands::game_commands!($($rest)*);
    };
    (
        $(#[$doc:meta])*
        change $name:ident($($arg:ident: $ty:ty),* $(,)?) = |$root:ident, $game:ident| $body:expr;
        $($rest:tt)*
    ) => {
        $(#[$doc])*
        #[tauri::command]
        #[specta::specta]
        pub async fn $name(
            data: tauri::State<'_, $crate::context::Data>,
            game: String,
            $($arg: $ty),*
        ) -> Result<$crate::status::views::Status, $crate::error::AppError> {
            $crate::commands::change(data, game, move |$root, $game| $body).await
        }

        $crate::commands::game_commands!($($rest)*);
    };
    ($(#[$doc:meta])* change $name:ident() = $work:path; $($rest:tt)*) => {
        $(#[$doc])*
        #[tauri::command]
        #[specta::specta]
        pub async fn $name(
            data: tauri::State<'_, $crate::context::Data>,
            game: String,
        ) -> Result<$crate::status::views::Status, $crate::error::AppError> {
            $crate::commands::change(data, game, $work).await
        }

        $crate::commands::game_commands!($($rest)*);
    };
}

pub(crate) use game_commands;

/// Commands that the frontend sees. The `collect_commands!` invocation is generated at build time from `commands.txt`,
/// so the paths exist in one canonical list. The stable task and notification codes go into `bindings.ts` from the
/// modules that own them, so the window cannot leave a code out of its maps.
pub fn builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(include!(concat!(env!("OUT_DIR"), "/collected-commands.rs")))
        .constant("TASK_KINDS", task_commands::TASK_KINDS)
        .constant(
            "NOTIFICATION_OUTCOMES",
            crate::settings::config::NOTIFICATION_OUTCOMES,
        )
        .constant(
            "APP_NOTIFICATIONS",
            crate::settings::config::APP_NOTIFICATIONS,
        )
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
