//! The session report: what the hook installed and which switches it used. It is written once, after the hooks are
//! enabled, so a trace read later says what produced it. The report is JSON; the app and a developer can read it.

use gobbo_overlay_core::MountConfig;
use serde_json::json;

use super::settings::settings;
use super::writer::write_file;
use crate::detours::NO_DISABLE_WARNING;

/// The state of one hook after installation.
#[derive(Debug)]
pub(crate) enum HookState {
    /// The detour was created but has not been enabled yet.
    Created,
    /// The detour was created and enabled.
    Enabled,
    /// `GOBBO_OVERLAY_DISABLE` names it.
    Disabled,
    /// The detour could not be created or enabled; the original function is still in place.
    Failed(String),
}

/// One hook and its state, for the session report.
#[derive(Debug)]
pub(crate) struct HookStatus {
    /// The field name in the hook table, which is also the `GOBBO_OVERLAY_DISABLE` name.
    pub(crate) name: &'static str,
    /// What happened to it.
    pub(crate) state: HookState,
}

impl HookStatus {
    /// A hook whose detour was created but not enabled yet.
    pub(crate) fn created(name: &'static str) -> Self {
        Self {
            name,
            state: HookState::Created,
        }
    }

    /// A hook whose detour could not be created or enabled.
    pub(crate) fn failed(name: &'static str, error: String) -> Self {
        Self {
            name,
            state: HookState::Failed(error),
        }
    }

    /// Whether the hook is active.
    pub(crate) fn is_enabled(&self) -> bool {
        matches!(self.state, HookState::Enabled)
    }

    /// Whether the hook is not active because of an error.
    pub(crate) fn is_failed(&self) -> bool {
        matches!(self.state, HookState::Failed(_))
    }

    /// Marks a created hook as enabled after a successful transaction.
    pub(crate) fn mark_enabled(&mut self) {
        self.state = HookState::Enabled;
    }

    /// Marks a created hook as failed, with the reason the transaction gave.
    pub(crate) fn mark_failed(&mut self, error: String) {
        self.state = HookState::Failed(error);
    }

    /// Marks a hook as disabled by `GOBBO_OVERLAY_DISABLE`. A failed hook stays failed: the error is what happened,
    /// while `Disabled` would claim the user chose to leave it off.
    pub(crate) fn mark_disabled(&mut self) {
        if self.is_failed() {
            return;
        }
        self.state = HookState::Disabled;
    }
}

/// The steps of the install and the time each took, in microseconds, in the order they ran. The report carries them, so
/// the cost of starting a game can be read per step.
pub(crate) struct Timeline {
    last: std::time::Instant,
    steps: Vec<(&'static str, u64)>,
}

impl Timeline {
    /// A timeline that starts now.
    pub(crate) fn start() -> Self {
        Self {
            last: std::time::Instant::now(),
            steps: Vec::new(),
        }
    }

    /// Ends the step `name`, which ran since the previous mark.
    pub(crate) fn mark(&mut self, name: &'static str) {
        let now = std::time::Instant::now();
        let micros = now.duration_since(self.last).as_micros();
        self.steps
            .push((name, u64::try_from(micros).unwrap_or(u64::MAX)));
        self.last = now;
    }

    /// The steps, in order.
    pub(crate) fn steps(&self) -> &[(&'static str, u64)] {
        &self.steps
    }
}

/// Writes the session report to the file `GOBBO_OVERLAY_SESSION` names, or to the developer file in a dev build. A
/// failure is logged; it must never stop the game.
pub(crate) fn write(
    config: &MountConfig,
    statuses: &[HookStatus],
    install_ms: u64,
    steps: &[(&'static str, u64)],
) {
    let Some(path) = &settings().session else {
        return;
    };
    let report = report(config, statuses, install_ms, steps);

    if let Err(error) = write_file(path, &report.to_string()) {
        crate::trace::log(&format!("could not write the session report: {error}"));
    }
}

/// The session report value. It is separate from `write` so the format is testable without a file or an environment.
fn report(
    config: &MountConfig,
    statuses: &[HookStatus],
    install_ms: u64,
    steps: &[(&'static str, u64)],
) -> serde_json::Value {
    let warnings: Vec<String> = statuses
        .iter()
        .filter_map(|status| match &status.state {
            HookState::Failed(error) => Some(format!("hook {} failed: {error}", status.name)),
            HookState::Disabled if !NO_DISABLE_WARNING.contains(&status.name) => {
                Some(format!("hook {} disabled", status.name))
            }
            HookState::Disabled => None,
            HookState::Created | HookState::Enabled => None,
        })
        .collect();
    let hooks: Vec<serde_json::Value> = statuses
        .iter()
        .map(|status| match &status.state {
            HookState::Created => json!({ "name": status.name, "state": "created" }),
            HookState::Enabled => json!({ "name": status.name, "state": "enabled" }),
            HookState::Disabled => json!({ "name": status.name, "state": "disabled" }),
            HookState::Failed(error) => {
                json!({ "name": status.name, "state": "failed", "error": error })
            }
        })
        .collect();
    let game = config.game();
    let mounts: Vec<serde_json::Value> = config
        .mounts
        .iter()
        .map(|mount| {
            json!({
                "id": mount.id,
                "guest": mount.guest.display().to_string(),
                "root": mount.root.display().to_string(),
                "staging": mount.staging.as_ref().map(|path| path.display().to_string()),
                "upper": mount.upper.display().to_string(),
            })
        })
        .collect();
    json!({
        "format": 3,
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "exe": exe(),
        "arch": std::env::consts::ARCH,
        "injection": "early",
        "init": "synchronous",
        "install_ms": install_ms,
        "install_steps": steps.iter().map(|(name, micros)| json!({ "step": name, "us": micros })).collect::<Vec<_>>(),
        "warnings": warnings,
        "layers": game.map(|mount| json!({
            "root": mount.root.display().to_string(),
            "staging": mount.staging.as_ref().map(|path| path.display().to_string()),
            "upper": mount.upper.display().to_string(),
        })),
        "mounts": mounts,
        "hooks": hooks,
        "ring": {
            "path": settings().ring.as_ref().map(|path| path.display().to_string()),
        },
        "dumper": {
            "socket": settings().dumper.as_ref().map(|path| path.display().to_string()),
            "connected": super::crash::dumper_connected(),
        },
        "switches": {
            "calls": settings().calls,
            "pass_trace": settings().pass_writes,
            "crash_first": settings().crash_first,
            "passthrough": settings().passthrough,
            "allow_unhooked_children": settings().allow_unhooked_children,
            "log": settings().log.as_ref().map(|path| path.display().to_string()),
            "diag": settings().diag.as_ref().map(|path| path.display().to_string()),
            "crash": settings().crash.as_ref().map(|path| path.display().to_string()),
            "dump": settings().dump.as_ref().map(|path| path.display().to_string()),
            "dumper": settings().dumper.as_ref().map(|path| path.display().to_string()),
            "ring": settings().ring.as_ref().map(|path| path.display().to_string()),
        },
    })
}

/// The program's path, for the report; `<unknown>` when Windows does not give it.
fn exe() -> String {
    std::env::current_exe().map_or_else(
        |_| "<unknown>".to_string(),
        |path| path.display().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layers() -> MountConfig {
        MountConfig {
            version: MountConfig::VERSION,
            mounts: vec![gobbo_overlay_core::MountSpec {
                id: gobbo_overlay_core::MountSpec::GAME_ID.to_string(),
                guest: std::path::PathBuf::from(r"C:\Game"),
                root: std::path::PathBuf::from(r"C:\Game"),
                staging: Some(std::path::PathBuf::from(r"C:\State\mods")),
                upper: std::path::PathBuf::from(r"C:\State\overwrite"),
            }],
        }
    }

    #[test]
    fn the_report_has_the_format_and_hides_tracing_only_disabled_hooks() {
        let mut failed = HookStatus::created("create_file");
        failed.mark_failed("boom".to_string());
        let mut tracing = HookStatus::created("get_file_information");
        tracing.mark_disabled();
        let report = report(&layers(), &[failed, tracing], 12, &[("config", 5)]);
        assert_eq!(report["format"], 3);
        assert_eq!(report["install_ms"], 12);
        assert_eq!(report["hooks"][1]["state"], "disabled");
        let warnings = report["warnings"].as_array().unwrap();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].as_str().unwrap().contains("create_file"));
    }

    #[test]
    fn a_failed_hook_stays_failed_when_marked_disabled() {
        let mut status = HookStatus::failed("create_file", "boom".to_string());
        status.mark_disabled();
        assert!(status.is_failed());
    }
}
