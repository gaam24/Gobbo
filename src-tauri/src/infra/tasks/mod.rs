//! Cancellation flags and the task registry for the long operations the window can stop or watch.
//!
//! The app owns one registry, so a task started while the window shows another game is still listed and cancellable
//! from the global Working panel. The registry publishes its lifecycle as Tauri events; it never translates the codes it
//! emits.

mod commands;
mod events;
mod journal;
mod key;
mod model;
mod registry;
mod signal;
#[cfg(test)]
mod tests;

pub use commands::{cancel_task, dismiss_task, list_tasks};
// The Tauri and Specta command macros put their handler macros beside the functions they annotate. The generated
// command list names them through this module, so they travel with the functions.
#[doc(hidden)]
pub use commands::{
    __cmd__cancel_task, __cmd__dismiss_task, __cmd__list_tasks, __specta__fn__cancel_task,
    __specta__fn__dismiss_task, __specta__fn__list_tasks, __tauri_command_name_cancel_task,
    __tauri_command_name_dismiss_task, __tauri_command_name_list_tasks,
};
pub use key::TaskKey;
pub use model::{TASK_KINDS, TaskView, kind, state};
// The typed dedupe is read only by the task tests and the tray tests that build task rows.
#[cfg(test)]
pub use model::TaskDedupe;
pub use registry::{TaskFlags, TaskHandle};
pub use signal::{cancelled, stop_if};
