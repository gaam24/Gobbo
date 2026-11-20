//! The switches of the hook, read from the environment once. A dev build turns the trace and the report on by default,
//! writing to the developer files of the Gobbo folder.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::OnceLock;

use gobbo_overlay_decisions::{
    BASE_STEM, CRASH_DUMP_FILE, ENV_DUMPER, ENV_DUMPER32, ENV_IN_PROCESS_DUMP, ENV_RING,
};

/// The switches, read once.
pub(crate) struct Settings {
    /// The trace file (`GOBBO_OVERLAY_LOG`).
    pub(crate) log: Option<PathBuf>,
    /// The count report file (`GOBBO_OVERLAY_DIAG`).
    pub(crate) diag: Option<PathBuf>,
    /// The crash report file (`GOBBO_OVERLAY_CRASH`).
    pub(crate) crash: Option<PathBuf>,
    /// The minidump file (`GOBBO_OVERLAY_DUMP`), written for a crash when set.
    pub(crate) dump: Option<PathBuf>,
    /// The out-of-process minidumper socket (`GOBBO_OVERLAY_DUMPER`, or `GOBBO_OVERLAY_DUMPER32` on x86).
    pub(crate) dumper: Option<PathBuf>,
    /// The base of the decision ring (`GOBBO_OVERLAY_RING`). Every process adds its id and `.ring`.
    pub(crate) ring: Option<PathBuf>,
    /// The session report file (`GOBBO_OVERLAY_SESSION`).
    pub(crate) session: Option<PathBuf>,
    /// The detours left off (`GOBBO_OVERLAY_DISABLE`).
    disabled: Vec<String>,
    /// Whether first-chance exceptions are reported (`GOBBO_OVERLAY_CRASH_FIRST`). Off by default: an exception a
    /// program handles itself is normal, and the vectored handler sees every one of them.
    pub(crate) crash_first: bool,
    /// Whether every call and result is traced and counted (`GOBBO_OVERLAY_CALLS`). Off by default: a line per call
    /// with a stack walk made games unplayable and distorted measurements.
    pub(crate) calls: bool,
    /// Whether a write, delete or rename outside the game folder is traced (`GOBBO_OVERLAY_PASS`). Off by default:
    /// a game writes its saves and settings outside the game folder all the time.
    pub(crate) pass_writes: bool,
    /// Whether every call passes through (`GOBBO_OVERLAY_TEST_PASSTHROUGH`).
    pub(crate) passthrough: bool,
    /// Whether a child that could not receive the hook is allowed to run (`GOBBO_OVERLAY_ALLOW_UNHOOKED_CHILDREN`).
    /// Off by default: a child that may touch the game folder must not run unhooked.
    pub(crate) allow_unhooked_children: bool,
    /// Whether the enable transaction must count the process's threads (`GOBBO_OVERLAY_SAFE_THREADS`). Off by default:
    /// `GobboOverlayInit` runs from the launcher's stub before the program's first instruction, so the main thread is
    /// the only one. The switch is the fallback for a manual load or an unusual launcher.
    pub(crate) safe_threads: bool,
}

impl Settings {
    /// Whether the detour with the field name `name` is left off.
    pub(crate) fn disabled(&self, name: &str) -> bool {
        self.disabled.iter().any(|item| item == name)
    }
}

/// The switches of this process.
pub(crate) fn settings() -> &'static Settings {
    static SETTINGS: OnceLock<Settings> = OnceLock::new();
    SETTINGS.get_or_init(|| Settings {
        log: switch("GOBBO_OVERLAY_LOG", "trace.log"),
        diag: switch("GOBBO_OVERLAY_DIAG", "counts.txt"),
        crash: switch("GOBBO_OVERLAY_CRASH", "crash.txt"),
        dump: dump_switch(),
        dumper: dumper_switch(),
        ring: ring_switch(),
        session: switch("GOBBO_OVERLAY_SESSION", "session.json"),
        disabled: std::env::var("GOBBO_OVERLAY_DISABLE")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        crash_first: flag("GOBBO_OVERLAY_CRASH_FIRST"),
        calls: flag("GOBBO_OVERLAY_CALLS"),
        pass_writes: flag("GOBBO_OVERLAY_PASS"),
        passthrough: flag("GOBBO_OVERLAY_TEST_PASSTHROUGH"),
        allow_unhooked_children: flag("GOBBO_OVERLAY_ALLOW_UNHOOKED_CHILDREN"),
        safe_threads: flag("GOBBO_OVERLAY_SAFE_THREADS"),
    })
}

/// An on/off switch: any non-empty value except `off` turns it on; unset, empty or `off` leaves it off.
fn flag(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty() && value != "off")
}

/// The minidump file: `1` or `on` selects the developer file, another non-empty value is a path, and unset/`off` means
/// no dump. Unlike the trace, a dump is never on by default: it is large and a first-chance exception may be harmless.
fn dump_switch() -> Option<PathBuf> {
    match std::env::var_os(ENV_IN_PROCESS_DUMP) {
        Some(value) if value.is_empty() || value == "off" => None,
        Some(value) if value == "1" || value == "on" => developer_file(CRASH_DUMP_FILE),
        Some(value) => Some(PathBuf::from(value)),
        None => None,
    }
}

/// The base of the decision ring: a path, or `1`/`on` for the developer base. Unset, a dev build keeps a ring next to
/// the other developer files; a release build writes none until the app names a base.
fn ring_switch() -> Option<PathBuf> {
    let developer = || developer_dir().map(|dir| dir.join(BASE_STEM));

    match std::env::var_os(ENV_RING) {
        Some(value) if value.is_empty() || value == "off" => None,
        Some(value) if value == "1" || value == "on" => developer(),
        Some(value) => Some(PathBuf::from(value)),
        None => {
            if cfg!(debug_assertions) {
                developer()
            } else {
                None
            }
        }
    }
}

/// The out-of-process minidumper socket. The x64 Gobbo process serves the x64 hook; the 32-bit hook uses the helper and
/// its separate variable, because the minidumper protocol carries pointer-sized fields.
fn dumper_switch() -> Option<PathBuf> {
    let name = if cfg!(target_pointer_width = "64") {
        ENV_DUMPER
    } else {
        ENV_DUMPER32
    };
    choose(std::env::var_os(name), || None)
}

/// The file a switch names: the value of the variable, or in a dev build the developer file `default_file` of this
/// program.
fn switch(name: &str, default_file: &str) -> Option<PathBuf> {
    choose(std::env::var_os(name), || {
        if cfg!(debug_assertions) {
            developer_file(default_file)
        } else {
            None
        }
    })
}

/// The file a switch names, given its variable's value. An empty value or `off` names nothing; any other value is a path.
/// Without a value, `default` decides.
fn choose(value: Option<OsString>, default: impl FnOnce() -> Option<PathBuf>) -> Option<PathBuf> {
    match value {
        Some(value) if value.is_empty() || value == "off" => None,
        Some(value) => Some(PathBuf::from(value)),
        None => default(),
    }
}

/// `%LOCALAPPDATA%\Gobbo\overlay`, the folder of the developer files.
pub(super) fn developer_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("Gobbo").join("overlay"))
}

/// `<game>.<pid>.<file>` in the developer folder, where `<game>` is the name of the running program and `<pid>` its process
/// id. Each run has files of its own, so a run never writes into the trace of another.
fn developer_file(file: &str) -> Option<PathBuf> {
    let game = std::env::current_exe()
        .ok()?
        .file_stem()?
        .to_string_lossy()
        .into_owned();
    Some(developer_dir()?.join(format!("{game}.{}.{file}", std::process::id())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_or_off_switch_names_nothing() {
        let developer = || Some(PathBuf::from("dev"));
        assert_eq!(choose(Some(OsString::new()), developer), None);
        assert_eq!(choose(Some(OsString::from("off")), developer), None);
    }

    #[test]
    fn a_path_switch_names_that_path_and_an_unset_one_takes_the_default() {
        let developer = || Some(PathBuf::from("dev"));
        assert_eq!(
            choose(Some(OsString::from("C:\\x\\trace.log")), developer),
            Some(PathBuf::from("C:\\x\\trace.log"))
        );
        assert_eq!(choose(None, developer), Some(PathBuf::from("dev")));
        assert_eq!(choose(None, || None), None);
    }
}
