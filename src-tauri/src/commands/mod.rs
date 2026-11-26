//! The Tauri commands: the whole interface of the backend to the window. Each one is a thin call into the module that
//! owns the use case.
//!
//! Every command runs its work through `blocking`: Tauri runs synchronous commands on the main thread, and these all
//! touch the disk or the network, so the window never waits for them. A command that changes a game answers with the
//! game's fresh status (`change`), so the interface never needs a second call to see the result.

/// The shared helpers behind the commands: blocking work, per-game commands and task bookkeeping.
pub mod helpers;
/// The macro that declares the short commands which run one use case for one game.
pub mod macros;

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

use tauri_specta::{Builder, collect_commands};

use crate::infra::tasks as task_commands;

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
