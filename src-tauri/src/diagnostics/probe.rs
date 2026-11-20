//! The overlay doctor: run the probe inside the active overlay session and return its checks to the window.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use gobbo_overlay_probe::{REPORT_FLAG, Report};
use serde::{Deserialize, Serialize};

use crate::context::{Data, load_game};
use crate::deploy::overlay::{self, Session};
use crate::error::AppError;
use crate::launch::start::game_running;
use crate::launch::start::overlay_session_state;

/// The result of one probe binary (x64 or x86).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct OverlayProbeReport {
    /// `x64` or `x86`: the architecture of the probe and the hook it tested.
    pub arch: String,
    /// Whether every check passed, including the physical game folder check.
    pub ok: bool,
    /// The unique folder the probe used under the game root.
    pub probe: String,
    /// The layers the probe ran in.
    pub layers: BTreeMap<String, String>,
    /// The checks, in the order the probe ran them.
    pub checks: Vec<OverlayProbeCheck>,
}

/// One line of the probe report.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct OverlayProbeCheck {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

impl From<gobbo_overlay_probe::Check> for OverlayProbeCheck {
    fn from(check: gobbo_overlay_probe::Check) -> Self {
        Self {
            name: check.name,
            ok: check.ok,
            detail: check.detail,
        }
    }
}

/// What one probe run contributed: the probe's own answer plus the parts the app's view needs.
struct ProbeResult {
    ok: bool,
    checks: Vec<OverlayProbeCheck>,
    probe: Option<String>,
    layers: BTreeMap<String, String>,
}

/// The state folder of the active overlay session, for the diagnostics panel. `None` when nothing is deployed.
pub fn state_path(root: &Data, game_id: &str) -> Option<String> {
    let data = root.game(game_id);
    data.deployed_state()
        .map(|state| state.display().to_string())
}

/// Runs every probe binary that is installed, in the overlay of the game's active profile.
pub fn run_probe(root: &Data, game_id: &str) -> Result<Vec<OverlayProbeReport>, AppError> {
    if game_running(root, game_id)? {
        return Err(crate::error::game_running());
    }

    let loaded = load_game(root, game_id)?;
    let data = loaded.data;
    let state =
        overlay_session_state(&data)?.ok_or_else(|| AppError::user("deploy.no_overlay_session"))?;
    let game_root = PathBuf::from(loaded.settings.game_root.trim());
    let session = Session::at(&state, &game_root);
    let probes = probe_programs();

    if probes.is_empty() {
        return Err(AppError::user("deploy.overlay_probe_missing")
            .with("path", "gobbo_overlay_probe.exe")
            .recovery(crate::error::Recovery::new(
                crate::error::RecoveryAction::OpenLog,
            )));
    }

    let mut reports = Vec::new();

    for (arch, probe) in probes {
        let report_path = state.join(format!("overlay-probe-{arch}.json"));
        let _ = fs::remove_file(&report_path);
        let args = vec![REPORT_FLAG.to_string(), report_path.display().to_string()];
        let started = overlay::run_with_output(&session, &probe.display().to_string(), &args, None);
        let mut result = match started {
            Ok(_code) => read_report(&report_path),
            Err(error) => ProbeResult {
                ok: false,
                checks: vec![failed("start", error.to_string())],
                probe: None,
                layers: BTreeMap::new(),
            },
        };
        let clean = result
            .probe
            .as_deref()
            .is_none_or(|folder| !game_root.join(folder).exists());
        result.checks.push(OverlayProbeCheck {
            name: "game folder clean".to_string(),
            ok: clean,
            detail: if clean {
                String::new()
            } else {
                "the game folder has the probe folder".to_string()
            },
        });
        // The probe's own answer is authoritative; only the app's physical-folder check can add a failure.
        let ok = result.ok && clean;
        reports.push(OverlayProbeReport {
            arch: arch.to_string(),
            ok,
            probe: result.probe.unwrap_or_default(),
            layers: result.layers,
            checks: result.checks,
        });
    }

    Ok(reports)
}

/// The probe binaries next to the Gobbo executable, native first. The dev layout has the Cargo binary name.
fn probe_programs() -> Vec<(&'static str, PathBuf)> {
    let candidates = [
        ("x64", ["gobbo_overlay_probe.exe", "overlay-probe.exe"]),
        ("x86", ["gobbo_overlay_probe32.exe", "overlay-probe32.exe"]),
    ];
    let mut found = Vec::new();

    for (arch, names) in candidates {
        for name in names {
            if let Some(path) = crate::infra::binaries::adjacent(name) {
                found.push((arch, path));
                break;
            }
        }
    }

    found
}

/// The probe's answer, checks, probe folder and layers of one report. A missing or broken report gives one failed check
/// instead, so the window still sees why the doctor could not answer.
fn read_report(path: &Path) -> ProbeResult {
    match read_file(path) {
        Ok(report) => ProbeResult {
            ok: report.ok,
            checks: report.checks.into_iter().map(Into::into).collect(),
            probe: report.probe,
            layers: report.layers.unwrap_or_default(),
        },
        Err(error) => ProbeResult {
            ok: false,
            checks: vec![failed("report", error)],
            probe: None,
            layers: BTreeMap::new(),
        },
    }
}

/// Reads and parses one probe report.
fn read_file(path: &Path) -> Result<Report, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

/// A failed check with a detail.
fn failed(name: &str, detail: String) -> OverlayProbeCheck {
    OverlayProbeCheck {
        name: name.to_string(),
        ok: false,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_broken_report_becomes_a_failed_check() {
        let path =
            std::env::temp_dir().join(format!("gobbo-probe-report-{}.json", std::process::id()));
        fs::write(&path, "not json").unwrap();
        let result = read_report(&path);
        assert!(!result.ok);
        assert_eq!(result.checks.len(), 1);
        assert!(!result.checks[0].ok);
        assert_eq!(result.checks[0].name, "report");
        assert_eq!(result.probe, None);
        assert!(result.layers.is_empty());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn a_probe_folder_is_read_from_the_report() {
        assert_eq!(read_report(Path::new("missing-file")).probe, None);
    }

    #[test]
    fn the_probes_answer_is_used_even_when_its_checks_are_empty() {
        let path =
            std::env::temp_dir().join(format!("gobbo-probe-answer-{}.json", std::process::id()));
        fs::write(
            &path,
            r#"{"ok":false,"probe":null,"layers":null,"checks":[]}"#,
        )
        .unwrap();
        let result = read_report(&path);
        assert!(!result.ok);
        assert!(result.checks.is_empty());
        let _ = fs::remove_file(path);
    }
}
