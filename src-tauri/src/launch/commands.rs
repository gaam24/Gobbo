//! The commands that start a game, watch it, stop it, and run the tools that go with it.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::context::Data;
use crate::error::AppError;
use crate::games::paths::home_dir;
use crate::infra::tasks::{TaskFlags, state};
use crate::launch::report::RunReport;

crate::commands::game_commands! {
    /// Whether the game runs now, from Gobbo or from Steam. Deploying and launching wait while it does; editing does
    /// not.
    query game_running() -> bool = crate::launch::start::game_running;

    /// Stops every process of the game, also one started outside Gobbo. Returns `true` when a process was stopped.
    /// The last launch of a game: its steps and how the game ended. `None` before the first launch from Gobbo.
    query last_run() -> Option<RunReport> = |root, game| Ok(crate::launch::report::last_run(root, game));
}

/// The task key of one tool operation. The same key is passed to `cancel_task` from the Working panel.
fn tool_task_key(kind: &str, game: &str, tool: &str) -> String {
    format!("tool:{kind}:{game}:{tool}")
}

/// The state a tool run leaves in the task registry. A cancelled run is never an exit code: the user stopped it, so
/// it keeps its `cancelled` state even when the program had already started.
pub fn tool_task_state(code: Option<i32>, cancelled: bool) -> &'static str {
    if cancelled {
        state::CANCELLED
    } else if code == Some(0) {
        state::DONE
    } else {
        state::FAILED
    }
}

/// The payload of `tool-exited`, sent when a tool run ends. `code` is `None` when the run was cancelled; `cancelled`
/// says so without the window having to read the code.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ToolExited {
    /// The game the tool belongs to.
    pub game: String,
    /// The tool that ended.
    pub tool: String,
    /// The tool's exit code, or `None` when the run was cancelled.
    pub code: Option<i32>,
    /// Whether the user stopped the run.
    pub cancelled: bool,
}

/// The event that carries [`ToolExited`].
pub const TOOL_EXITED_EVENT: &str = "tool-exited";

/// The payload of a tool run that ended with this exit code and cancellation state. A cancelled run never carries a
/// code, so a killed tool is not reported with the fake non-zero code its termination produced.
pub const fn tool_exit_payload(
    game: String,
    tool: String,
    exit_code: Option<i32>,
    cancelled: bool,
) -> ToolExited {
    ToolExited {
        game,
        tool,
        code: if cancelled { None } else { exit_code },
        cancelled,
    }
}

/// Finishes the task of a tool run and tells the window how it ended. The record and the event use the same
/// distinction: a cancelled run keeps its `cancelled` state and a null code; exit code `0` is done, any other code is
/// failed.
fn tool_exit_notice(
    app: AppHandle,
    key: String,
    game: String,
    tool: String,
    cancel: Arc<AtomicBool>,
) -> crate::launch::pipeline::ExitNotice {
    Box::new(move |exit_code| {
        let cancelled = crate::infra::tasks::cancelled(Some(cancel.as_ref()));
        {
            let tasks = app.state::<TaskFlags>();
            tasks.finish(&key, tool_task_state(exit_code, cancelled));
        }
        let _ = app.emit(
            TOOL_EXITED_EVENT,
            tool_exit_payload(game.clone(), tool.clone(), exit_code, cancelled),
        );
    })
}

/// Runs a tool of the game in the overlay. Registers a task and returns as soon as the program has started; the
/// `tool-exited` event carries its exit code when it ends. The run can be stopped from the Working panel with
/// `cancel_task` before the program starts.
#[tauri::command]
#[specta::specta]
pub async fn run_tool(
    app: AppHandle,
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    tool: String,
) -> Result<(), AppError> {
    let key = tool_task_key("launch", &game, &tool);
    let notice_key = key.clone();
    let notice_game = game.clone();
    let notice_tool = tool.clone();
    // The setup is short: the pipeline starts the program and detaches the wait to its own thread.
    crate::commands::start_detached_task(
        data,
        &tasks,
        key,
        game,
        "launch",
        move |root, game, cancel, handle| {
            let notice = tool_exit_notice(
                app.clone(),
                notice_key,
                notice_game,
                notice_tool,
                Arc::clone(&cancel),
            );
            crate::launch::tools::running::run_tool(
                root,
                game,
                &tool,
                &home_dir()?,
                cancel,
                handle,
                Some(notice),
            )
        },
    )
    .await
}

/// Installs the prerequisites of a tool into the game's Proton prefix. Runs as a task, so the Working panel shows it
/// for its whole duration and `cancel_task` can stop it before it begins.
#[tauri::command]
#[specta::specta]
pub async fn install_tool_prerequisites(
    data: State<'_, Data>,
    tasks: State<'_, TaskFlags>,
    game: String,
    tool: String,
) -> Result<(), AppError> {
    let key = tool_task_key("install", &game, &tool);
    crate::commands::start_task(
        data,
        &tasks,
        key,
        game,
        "install",
        move |root, game, cancel, _handle| {
            crate::infra::tasks::stop_if(Some(cancel.as_ref()), "tools.cancelled")?;
            crate::launch::tools::install_prerequisites(root, game, &tool, &home_dir()?)
        },
    )
    .await
}

/// Launches the game's executable (through Proton on Linux). `executable` is the name from the game definition,
/// `None` = the default. `apply_draft` applies the current list first; otherwise a clean deployment starts as it
/// is. Returns `true` when the game starts with mods, that is, when the deployment is prepared.
///
/// When the game ends, the command sends `game-exited` with its id, so the running check stops at once instead of
/// waiting for the next poll.
#[tauri::command]
#[specta::specta]
pub async fn launch_game(
    app: tauri::AppHandle,
    data: tauri::State<'_, crate::context::Data>,
    game: String,
    executable: Option<String>,
    apply_draft: bool,
) -> Result<bool, crate::error::AppError> {
    let event_game = game.clone();
    crate::commands::for_game(data, game, move |root, game| {
        let app = app.clone();
        crate::launch::start::launch_game(
            root,
            game,
            executable.as_deref(),
            apply_draft,
            &home_dir()?,
            Some(Box::new(move |_code| {
                let _ = app.emit("game-exited", event_game);
            })),
        )
    })
    .await
}

/// Stops the game and records that the user asked for it, so the last run is not treated as a crash.
#[tauri::command]
#[specta::specta]
pub async fn stop_game(
    data: tauri::State<'_, crate::context::Data>,
    game: String,
) -> Result<bool, crate::error::AppError> {
    crate::commands::for_game(data, game, |root, game| {
        let stopped = crate::launch::start::stop_game(root, game)?;
        if stopped {
            super::report::mark_stopped(root, game);
        }
        Ok(stopped)
    })
    .await
}
