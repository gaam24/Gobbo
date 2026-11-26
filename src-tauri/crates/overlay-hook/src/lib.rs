//! The Windows overlay hook. Gobbo loads this DLL into a game before the game starts. It hooks the file functions of
//! `kernel32` and redirects them through the layers of one or more mounts: a read sees the top-most layer that has the
//! file, a write goes to the upper layer (copy-up first), a deletion or a rename of a file of a lower layer leaves a
//! whiteout behind, and a folder listing shows every layer merged. The game mount covers the game folder; a user-data
//! mount covers a folder the game keeps outside it (`Documents/The Witcher 3`) and writes only its profile's upper
//! layer. Paths outside every guest path pass through untouched.
//!
//! The mounts come from the JSON file named by `GOBBO_OVERLAY_MOUNTS` (see `session_config` and `gobbo-overlay-core`);
//! without it the three legacy layer variables describe the game mount. Without either the DLL does nothing.
//! To find what the hook does and where a program loops, see `diag`: `GOBBO_OVERLAY_LOG` (a trace), `GOBBO_OVERLAY_DIAG`
//! (call counts, rewritten every 1000 calls), `GOBBO_OVERLAY_DISABLE` (detours to leave off) and
//! `GOBBO_OVERLAY_TEST_PASSTHROUGH` (no redirection at all).
//!
//! The modules: `detours` (the table of hooked exports), `win32` (the Win32 file calls), `nt` (the NT layer under them),
//! `search` (folder listings), `handles` (handles taken from a layer), `file_info` (calls that read a handle), `process`
//! (child processes), `paths` (where a path really is), `whiteouts` (what the game deleted), `state` (the process-wide
//! values), `guard` (the hook's own calls), `trace` and `diag` (the logs and counts), `text` (strings for Win32), and
//! `types` (the signatures).
//!
//! Limits of this version, all of them places where the game folder could still be written to:
//! - a folder of a lower layer cannot be renamed; the call fails instead, so the game folder stays as it is;
//! - a rename through an open handle (`SetFileInformationByHandle`) is not redirected, and is refused when the handle
//!   points into a lower layer;
//! - a 32-bit hook cannot load into a 64-bit child; a 64-bit hook loads both 32-bit and 64-bit children (the 32-bit
//!   child goes through `gobbo_overlay_inject32.exe`);
//! - a direct read of the PEB module list still sees the staging path of a mod DLL; the file and module path APIs are
//!   mapped instead, and `EnumProcessModules` itself returns only handles;
//! - the hook is loaded only into processes started through Gobbo, and into their children; a child started with a
//!   custom environment block that omits `GOBBO_OVERLAY_MOUNTS` does not see the user-data redirects.

#![cfg(windows)]
// The exported names (`DllMain`, `GobboOverlayInit`) are the names the loader and the launcher resolve, not Rust names.
#![allow(non_snake_case)]

#[macro_use]
mod macros;

#[cfg(feature = "bench-alloc")]
mod bench_alloc;
#[cfg(feature = "bench-stats")]
mod bench_stats;
#[cfg(feature = "bench-stats")]
pub(crate) use bench_stats::{
    io as stat_io, lock as stat_lock, probe as stat_probe, spelling as stat_spelling,
};
#[cfg(not(feature = "bench-stats"))]
#[inline(always)]
pub(crate) fn stat_probe() {}
#[cfg(not(feature = "bench-stats"))]
#[inline(always)]
pub(crate) fn stat_lock() {}
#[cfg(not(feature = "bench-stats"))]
#[inline(always)]
pub(crate) fn stat_io() {}
#[cfg(not(feature = "bench-stats"))]
#[inline(always)]
pub(crate) fn stat_spelling() {}
mod detours;
mod diag;
mod early;
mod file_info;
mod guard;
mod handles;
mod nt;
mod paths;
mod process;
mod search;
mod session_config;
mod state;
mod text;
mod trace;
mod types;
mod whiteouts;
mod win32;

use std::ffi::c_void;
use std::sync::OnceLock;

use gobbo_overlay_core::{MountConfig, MountSpec};
use windows_sys::Win32::Foundation::{CloseHandle, FALSE, GetLastError};
use windows_sys::Win32::System::LibraryLoader::DisableThreadLibraryCalls;
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows_sys::Win32::System::Threading::{EVENT_MODIFY_STATE, OpenEventW, SetEvent};

use crate::detours::{ESSENTIAL, PROCESS, PROCESS_INTERNAL};
use crate::state::{Mount, Mounts};
use trace::log;

/// The result of `GobboOverlayInit`. `OnceLock` makes the export idempotent: a second call returns the first result.
static INIT: OnceLock<bool> = OnceLock::new();

/// `DllMain`: only records the module and turns off thread notifications. The loader lock is held here, so no file I/O,
/// allocation, logging or detour installation happens in this function. Both detach paths do nothing.
///
/// # Safety
/// Called by the Windows loader with this DLL's module handle.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(
    module: *mut c_void,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        diag::remember_own(module);
        // SAFETY: `module` is this DLL's module handle, and the call only turns off thread notifications.
        unsafe { DisableThreadLibraryCalls(module) };
    }
    // DLL_PROCESS_DETACH does nothing on either path: dynamic unload is not a supported operation. The installed
    // hooks and the background threads outlive it, the OS reclaims everything at process death, and a trace tail
    // shorter than the flush interval is lost by design. A lock or I/O here could deadlock under the loader lock.

    1
}

/// Initializes the overlay in this process. The launcher's stub calls this after `LoadLibraryExW` and before the
/// program's `LdrInitializeThunk`, so the hooks are in place before the loader resolves the program's static imports.
/// It runs outside the loader lock, because `DllMain` has returned by the time the stub calls it.
///
/// Returns 1 when the essential hooks are active and 0 when they are not. The stub stops the process on 0; a game must
/// never continue into its own code without the overlay. The call is idempotent.
#[unsafe(no_mangle)]
pub extern "C" fn GobboOverlayInit() -> i32 {
    let ready = *INIT.get_or_init(|| {
        if let Some(module) = diag::own_module()
            && let Some(path) = detours::exports::module_path(module)
        {
            process::remember_hook(path);
        }

        let ready = install();

        if ready {
            signal_ready();
        }

        ready
    });

    if ready { 1 } else { 0 }
}

/// Signals `Local\GobboOverlayReady_<pid>`, which the launcher waits on before it resumes the game. A failure is
/// logged so a launcher timeout has a cause; readiness itself never depends on the signal reaching the launcher.
fn signal_ready() {
    let name = gobbo_overlay_launch::ready_event_name(std::process::id());
    // SAFETY: `name` is a NUL-terminated name, and the call only opens the event the launcher created.
    let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) };

    if event.is_null() {
        // SAFETY: `GetLastError` only reads the error of the calling thread, before logging can change it.
        let error = unsafe { GetLastError() };
        log(&format!(
            "the ready event could not be opened (error {error}); the launcher may time out"
        ));
        return;
    }
    // SAFETY: `event` is an event handle opened above; it is closed once.
    let set = unsafe { SetEvent(event) };
    if set == FALSE {
        // SAFETY: `GetLastError` only reads the error of the calling thread, before logging can change it.
        let error = unsafe { GetLastError() };
        log(&format!(
            "the ready event could not be set (error {error}); the launcher may time out"
        ));
    }
    // SAFETY: `event` is an event handle opened above; it is closed once here.
    unsafe { CloseHandle(event) };
}

fn install() -> bool {
    let started = std::time::Instant::now();
    let mut timeline = diag::Timeline::start();
    let Some(config) = session_config::load() else {
        log("no overlay mounts");
        return false;
    };
    let Some(game_spec) = config.game() else {
        log("the mount set has no game mount");
        return false;
    };
    timeline.mark("config");
    diag::prune_old_files();
    timeline.mark("prune");
    // Only the trace's caller, the first-chance crash report and the dump paths need module ranges. Walking every
    // module is startup work with no benefit when none of them is on; the crash report prints the bare address when
    // the table is empty.
    let switches = diag::settings();
    if switches.calls
        || switches.crash.is_some()
        || switches.dump.is_some()
        || switches.dumper.is_some()
    {
        diag::remember_modules();
    }
    timeline.mark("modules");
    diag::install_crash_reporting();
    timeline.mark("crash_reporting");
    log_session(&config);
    let game_layers = game_spec.layers();
    let game_whiteouts = whiteouts::load(MountSpec::GAME_ID, &game_layers);
    timeline.mark("whiteouts_game");
    let game = Mount::new(
        MountSpec::GAME_ID.to_string(),
        game_spec.guest.clone(),
        game_layers,
        game_whiteouts,
    );
    let mut redirects = Vec::new();

    for spec in config.redirects() {
        let layers = spec.layers();
        let whiteouts = whiteouts::load(&spec.id, &layers);
        redirects.push(Mount::new(
            spec.id.clone(),
            spec.guest.clone(),
            layers,
            whiteouts,
        ));
    }

    timeline.mark("whiteouts_redirects");
    state::store_session(Mounts { game, redirects });
    // Build the path cache now, not on the first hooked call: the short-name lookup and the mounts are read once,
    // outside the game's pass-through budget.
    let _ = crate::paths::paths();
    timeline.mark("paths");
    // The loader's stubs may still jump into the stage 0 redirect. They go back to their own bytes before the detours
    // go in; a partial handover must stop here, or the loader's calls would bypass every decision the hook makes.
    if let Err(error) = early::restore() {
        log(&format!(
            "early redirect could not be handed back ({error}); detours are not installed"
        ));
        return false;
    }
    timeline.mark("early_restore");
    let (created, statuses) = unsafe { detours::create_hooks() };
    state::store_hooks(created);
    timeline.mark("create_hooks");
    // Warm the NT layer prefixes before the game runs: the first `GetMappedFileNameW` or `NtQueryInformationFile`
    // answer must not open folders or allocate inside the pass-through budget.
    let _ = crate::handles::map_layer_path_nt(&[]);
    timeline.mark("nt_prefixes");
    let Some(hooks) = state::hooks() else {
        diag::write_session(
            &config,
            &statuses,
            started.elapsed().as_millis() as u64,
            timeline.steps(),
        );
        log("overlay hooks were not stored");
        return false;
    };
    // SAFETY: the detours were created for the functions they replace, and nothing has run through them yet.
    let statuses = unsafe { hooks.enable(statuses) };
    // The threads that wait for the program start now, after the detours are in (see `diag::start_background_threads`).
    diag::start_background_threads();
    timeline.mark("enable");
    let enabled = statuses.iter().filter(|status| status.is_enabled()).count();
    let failed = statuses.iter().filter(|status| status.is_failed()).count();
    // The launcher may resume the game only when every hook the overlay needs is active. Optional hooks such as replace
    // and tracing may be missing without letting the game run against the real folder; the link hooks are now part of
    // the essential set. The two lists live next to the detour table so a renamed field has one place to be fixed.
    let is_enabled = |name: &str| {
        statuses
            .iter()
            .any(|status| status.name == name && status.is_enabled())
    };
    let ready = ESSENTIAL.iter().all(|name| is_enabled(name))
        && PROCESS.iter().any(|name| is_enabled(name));
    crate::process::set_internal_hook(is_enabled(PROCESS_INTERNAL));
    // The detours are live now. The report and the ring are the hook's own files, and their paths come from the
    // environment; keep their I/O out of the hook so a path under a mount is not redirected or recorded as a decision.
    let _diagnostic_io = crate::guard::Guard::enter();
    diag::write_session(
        &config,
        &statuses,
        started.elapsed().as_millis() as u64,
        timeline.steps(),
    );
    log(&format!(
        "overlay hooks: {enabled} enabled, {failed} failed, ready={ready}"
    ));
    // The session report is written before the readiness gate is evaluated. When the gate says no, append the
    // essential hooks that caused it, so "disabled" on one hook is not the whole explanation.
    if !ready {
        warn_unready(&statuses);
    }
    // The ring opens after the session report: its own writes, and the report's, are not game decisions.
    diag::open_ring(diag::settings().ring.as_deref());
    ready
}

/// The first lines of the trace: the program, the process, the mounts and the switches, so a trace read on its own says
/// what it is a trace of.
fn log_session(config: &MountConfig) {
    let program = std::env::current_exe().map_or_else(
        |_| "<unknown>".to_string(),
        |path| path.display().to_string(),
    );
    log(&format!(
        "session: hook {} in process {} for {program}",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    ));

    for mount in config.mounts() {
        log(&format!(
            "mount {}: guest {}, root {}, staging {}, upper {}",
            mount.id,
            mount.guest.display(),
            mount.root.display(),
            mount
                .staging
                .as_ref()
                .map_or_else(String::new, |path| path.display().to_string()),
            mount.upper.display()
        ));
    }

    if diag::settings().passthrough {
        log("passthrough: calls are not redirected");
    }
}

/// The essential hooks whose state explains why the overlay is not ready. The session report already warns about failed
/// hooks and most disabled ones; this names the ones the readiness gate needs, so the cause of `ready=false` is explicit.
fn unready_causes(statuses: &[diag::HookStatus]) -> Vec<String> {
    let mut causes = Vec::new();

    for name in ESSENTIAL.iter().chain(PROCESS.iter()) {
        for status in statuses.iter().filter(|status| status.name == *name) {
            if let Some(cause) = status.inactive_cause() {
                causes.push(format!("essential hook {name} {cause}"));
            }
        }
    }

    causes
}

/// Appends the readiness causes to the session report's warnings, under the install's diagnostic guard. The report is
/// written before the gate is evaluated, so its warnings cannot include this otherwise. Any failure is best-effort:
/// diagnostics must never stop the game.
fn warn_unready(statuses: &[diag::HookStatus]) {
    let causes = unready_causes(statuses);
    if causes.is_empty() {
        return;
    }
    log(&format!("overlay not ready: {}", causes.join("; ")));

    let Some(path) = &diag::settings().session else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(mut report) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(warnings) = report
        .get_mut("warnings")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return;
    };
    warnings.extend(causes.into_iter().map(serde_json::Value::String));
    if let Ok(text) = serde_json::to_string(&report) {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unready_overlay_names_essential_hooks_that_were_disabled_or_failed() {
        let mut disabled = diag::HookStatus::created("create_file");
        disabled.mark_disabled();
        let failed = diag::HookStatus::failed("nt_open_file", "no export".into());
        let mut optional = diag::HookStatus::created("get_file_information");
        optional.mark_disabled();
        let causes = unready_causes(&[disabled, failed, optional]);

        assert_eq!(causes.len(), 2, "{causes:?}");
        assert!(causes[0].contains("create_file"), "{causes:?}");
        assert!(causes[1].contains("nt_open_file"), "{causes:?}");
    }
}
