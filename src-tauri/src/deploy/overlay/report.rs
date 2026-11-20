//! What an overlay launch reports: the launcher's timings, the DLLs of the layer, and the decisions and crash dumps the
//! hook leaves in the session's state folder.

use std::fs;
use std::path::{Path, PathBuf};

use super::session::Session;
use crate::domain::resolver::DeployPlan;
use gobbo_overlay_decisions::Decision;

/// The launcher's steps before the program's first instruction, in microseconds. Linux has no hook installer, so its
/// `Running::timings` returns `None`; the Windows values come from `overlay-launch`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LaunchTimings {
    /// Everything before `CreateProcessW` in `start`: the hook lookup, the mount file and the dump monitors.
    pub setup_us: u64,
    /// The crash-dump monitors, part of `setup_us`.
    pub dump_us: u64,
    /// The DLL list and its static-import read, filled by the caller before the launch.
    pub dll_report_us: u64,
    pub create_process_us: u64,
    pub detect_us: u64,
    pub stage0_us: u64,
    pub ready_event_us: u64,
    pub inject_us: u64,
    pub imports_us: u64,
    pub resume_us: u64,
    pub ready_wait_us: u64,
}

/// The base name of the decision ring inside a session's state folder. Every process of the game adds its id and
/// `.ring`; the app reads the files while the game runs.
pub const DECISIONS_BASE: &str = "overlay-decisions";

/// The file that names the last out-of-process crash dump of a session.
pub const LAST_DUMP_FILE: &str = "last_dump";

/// The in-process dump of a session, used when the out-of-process dumper is not available (notably a 32-bit game).
pub const CRASH_DUMP_FILE: &str = "crash.dmp";

/// Where the decision ring of a session lives.
pub fn decisions_base(state: &Path) -> PathBuf {
    state.join(DECISIONS_BASE)
}

/// The newest decisions of a session's processes, and the total they recorded. A session without a ring, or a ring
/// file that is being read while it changes, gives what can be read; this never fails a launch or a report.
pub fn decisions(state: &Path, limit: usize) -> (Vec<Decision>, u64) {
    gobbo_overlay_decisions::read_base(&decisions_base(state), limit)
        .unwrap_or_else(|_| (Vec::new(), 0))
}

/// Removes the decision files of an earlier launch, before the next one starts its ring. The ring is read on every
/// platform, but only the Windows hook writes it.
#[cfg(windows)]
pub fn clear_decisions(state: &Path) {
    let _ = gobbo_overlay_decisions::clear(&decisions_base(state));
}

/// The crash dump of the last launch, when there is one. The out-of-process dumper names its file in
/// `LAST_DUMP_FILE`; the in-process fallback always writes `CRASH_DUMP_FILE`.
pub fn latest_dump(state: &Path) -> Option<String> {
    let marker = state.join(LAST_DUMP_FILE);

    if let Ok(text) = fs::read_to_string(&marker) {
        let path = text.trim();

        if !path.is_empty() && Path::new(path).is_file() {
            return Some(path.to_string());
        }
    }

    let fallback = state.join(CRASH_DUMP_FILE);
    fallback.is_file().then(|| fallback.display().to_string())
}

/// The DLLs an overlay launch found in the mod layer and the static imports it will point at that layer.
#[derive(Debug, Clone, Default)]
pub struct DllReport {
    /// The game-relative paths of every `.dll` in the mod layer, sorted.
    pub layer: Vec<String>,
    /// The static imports of the program that the launcher rewrites to the mod layer.
    pub injected: Vec<InjectedDll>,
}

/// One static import of the program that the launcher points at the mod layer.
#[derive(Debug, Clone)]
pub struct InjectedDll {
    /// The name as the import table spells it, for example `WINMM.dll`.
    pub name: String,
    /// Its game-relative path, for example `bin/x64/winmm.dll`.
    pub path: String,
    /// The full path of the file in the mod layer.
    pub source: String,
}

/// The file in a session's state folder that lists the DLLs of its layer. `prepare` writes it from the plan, so a launch
/// does not walk the whole layer to build its report.
pub const LAYER_DLLS_FILE: &str = "layer-dlls.txt";

/// The DLLs a prepared layer holds, from the file `prepare` wrote. A layer prepared before that file existed, or one
/// edited by hand, falls back to a walk.
pub fn session_layer_dlls(session: &Session) -> Vec<String> {
    fs::read_to_string(session.state.join(LAYER_DLLS_FILE)).map_or_else(
        |_| layer_dlls(&session.mods),
        |text| {
            text.lines()
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        },
    )
}

/// The prefix of the layer below `inner`, with `/` separators. An empty `inner` gives an empty prefix.
pub(super) fn layer_prefix(inner: &Path) -> String {
    let mut prefix = String::new();

    for part in inner.components() {
        prefix.push_str(&part.as_os_str().to_string_lossy());
        prefix.push('/');
    }

    prefix
}

/// Whether a game-relative path names a DLL.
fn is_dll(path: &str) -> bool {
    path.rsplit('.')
        .next()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
}

/// Saves the DLL list of a prepared layer. A diagnostic, so a failed write only means the next launch walks instead.
pub(super) fn write_layer_dlls(session: &Session, plan: &DeployPlan, inner: &Path) {
    let prefix = layer_prefix(inner);
    let mut found: Vec<String> = plan
        .files
        .iter()
        .filter(|file| is_dll(&file.dest))
        .map(|file| format!("{prefix}{}", file.dest))
        .collect();
    found.sort_unstable();
    let _ = fs::write(session.state.join(LAYER_DLLS_FILE), found.join("\n"));
}

/// The game-relative paths of every `.dll` below `root`, sorted. A folder that cannot be read is skipped: this is a
/// diagnostic, not a deployment rule.
pub fn layer_dlls(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let _ = crate::infra::fs::walk(
        root,
        crate::infra::fs::WalkErrors::Skip,
        &mut |_entry, rel, is_dir| {
            if !is_dir && is_dll(rel) {
                found.push(rel.to_string());
            }

            Ok(())
        },
    );
    found.sort_unstable();
    found.dedup();
    found
}
