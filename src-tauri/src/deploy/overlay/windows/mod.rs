//! Windows: a hook DLL in the game process. Gobbo starts the program suspended, points its main thread at a stub that
//! loads `gobbo_overlay_hook.dll` (`crates/overlay-hook`) and initializes it before the Windows loader resolves the
//! program's static imports, and then resumes it. The hook redirects the program's file calls to the layers, so the game
//! sees the mods and its writes go to `overwrite/`. Windows has no mount and Gobbo writes no mod file into the game
//! folder: a DLL next to the program is read from the mod layer like any other file.

use std::collections::BTreeSet;
use std::io::PipeWriter;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::time::Instant;

use gobbo_overlay_core::early::{ENV, FILE_NAME};
use gobbo_overlay_core::{ENV_MOUNTS, Layer, Layers, join_rel, relative_to};
use gobbo_overlay_decisions::ENV_RING;
use gobbo_overlay_launch::{Hooked, stage0};

use super::{
    CRASH_DUMP_FILE, DllReport, InjectedDll, LaunchTimings, Session, clear_decisions,
    decisions_base,
};
use crate::deploy::error::DeployError;
use crate::domain::resolver::DeployPlan;
use crate::error::{FileError, Message};
use crate::infra::store::write_atomic;
pub use mounts::{mount_config, mounts_path};

/// Checks that the hook DLL is there before anything is written: a deployment without it would not redirect the game.
pub fn check(_session: &Session) -> Result<(), DeployError> {
    hook_dll().map(|_| ())
}

/// Windows has no mount that outlives the game: the hook lives in the game process itself. A game started outside Gobbo
/// still reads the layer through its hook only when it was started by Gobbo, so a process in the game folder means a
/// rebuild must wait.
pub fn in_use(root: &Path) -> bool {
    crate::platform::game_is_running(root)
}

/// A started program with the hook loaded. `wait` returns its exit code. The process runs in this Gobbo process, so
/// Gobbo keeps its handle.
pub struct Running {
    program: PathBuf,
    hooked: Hooked,
    timings: LaunchTimings,
    dump: Option<dump::Monitor>,
    dump32: Option<dump32::Monitor>,
}

/// Starts `program` suspended with the hook loaded, in the layers of the session, and resumes it. `env` is added to
/// the environment the program inherits (it replaces variables of the same name). No file is written into the game
/// folder: the hook is loaded from Gobbo's folder, and the program's DLLs are read from the mod layer.
pub fn start(
    session: &Session,
    program: &str,
    args: &[String],
    env: &[(String, String)],
    output: Option<PipeWriter>,
) -> Result<Running, DeployError> {
    let request_started = Instant::now();
    let hook = hook_dll()?;
    let hook32 = hook_dll32();
    let layers = session.layers();
    // The hook reads every mount from one file. The three environment variables below stay for `gobbo run` and for
    // tests that start the hook without a session; the file wins when both are present.
    let mounts_path = mounts_path(&session.state);
    let mounts = serde_json::to_vec(&mount_config(session)?).map_err(DeployError::Json)?;
    std::fs::create_dir_all(&session.state).map_err(|e| FileError::new(&session.state, e))?;
    write_atomic(&mounts_path, &mounts)?;
    // Start the 32-bit helper now, so its process start and socket bind overlap the program read and the sweep.
    let pending32 = dump32::spawn(&session.state);
    // The early redirect of this launch: its handover file is written by the launcher before the game runs. A file left
    // by an earlier launch must not be read, because its addresses belong to another process. The file name and the
    // variable that names it are shared with the hook through `gobbo_overlay_core::early`.
    let handover = session.state.join(FILE_NAME);
    let _ = std::fs::remove_file(&handover);
    let early = early_config(&layers)?;
    let mut mount_env = vec![(ENV_MOUNTS.to_string(), mounts_path.display().to_string())];
    mount_env.push((ENV.to_string(), handover.display().to_string()));
    // A DLL the executable imports statically is loaded before any program code. The launcher points those imports at
    // the mod layer, because the hook is not installed early enough for the loader's own import lookup.
    let imports = static_imports_for(&layers, Path::new(program));
    // Each launch starts a fresh decision ring and a fresh dump slot; the app reads the files while this launch runs.
    clear_decisions(&session.state);
    sweep_stale_copies(session);
    dump::clear_previous(&session.state);
    let decisions = decisions_base(&session.state);
    let dump_started = Instant::now();
    let monitor = dump::start(&session.state);
    let monitor32 = pending32.and_then(dump32::Pending::wait);
    let dump_us = micros(dump_started);
    let mut vars: Vec<(String, String)> = layers
        .to_env()
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect();
    vars.append(&mut mount_env);
    vars.push((ENV_RING.to_string(), decisions.display().to_string()));

    if let Some(monitor) = &monitor {
        vars.push((
            dump::ENV_DUMPER.to_string(),
            monitor.path().display().to_string(),
        ));
    }

    if let Some(monitor) = &monitor32 {
        vars.push((
            dump32::ENV.to_string(),
            monitor.path().display().to_string(),
        ));
    }
    // A 32-bit hook keeps this in-process dump; a 64-bit hook ignores it in favour of the out-of-process one.
    vars.push((
        dump::ENV_IN_PROCESS_DUMP.to_string(),
        session.state.join(CRASH_DUMP_FILE).display().to_string(),
    ));
    // `env` replaces a variable of the same name: a duplicate in the environment block would leave Windows free to
    // pick either value. This is how a test turns a switch off for one launch.
    for (name, value) in env {
        if let Some(index) = vars
            .iter()
            .position(|(existing, _)| existing.eq_ignore_ascii_case(name))
        {
            vars.remove(index);
        }

        vars.push((name.clone(), value.clone()));
    }

    let setup_us = micros(request_started);
    let started = gobbo_overlay_launch::spawn_hooked(&gobbo_overlay_launch::Launch {
        program: Path::new(program),
        args,
        hook: &hook,
        hook32: hook32.as_deref(),
        imports: &imports,
        env: &vars,
        cwd: Some(session.root.as_path()),
        output: output.as_ref().map(AsRawHandle::as_raw_handle),
        early: early.map(|config| stage0::Early {
            config,
            handover: &handover,
        }),
    });
    // The pipe's write end is ours only until the start: the program has its own copy now.
    drop(output);

    match started {
        Ok(hooked) => {
            let t = hooked.timings();
            Ok(Running {
                program: PathBuf::from(program),
                timings: LaunchTimings {
                    setup_us,
                    dump_us,
                    dll_report_us: 0,
                    create_process_us: t.create_process_us,
                    detect_us: t.detect_us,
                    stage0_us: t.stage0_us,
                    ready_event_us: t.ready_event_us,
                    inject_us: t.inject_us,
                    imports_us: t.imports_us,
                    resume_us: t.resume_us,
                    ready_wait_us: t.ready_wait_us,
                },
                hooked,
                dump: monitor,
                dump32: monitor32,
            })
        }
        Err(error) => {
            drop(monitor);
            drop(monitor32);
            Err(FileError::new(Path::new(program), error).into())
        }
    }
}

/// Microseconds since `started`, saturating instead of wrapping on a very long step.
fn micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

impl Running {
    /// The launcher's steps, from the start request and the program read to the hook's ready event.
    // The Linux overlay has no hook installer and returns `None`; the Windows caller keeps the same shape.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the Linux Running::timings returns None for the same signature"
    )]
    pub const fn timings(&self) -> Option<LaunchTimings> {
        Some(self.timings)
    }

    /// Waits until the program exits and returns its exit code.
    pub fn wait(&mut self) -> Result<i32, DeployError> {
        let outcome = self
            .hooked
            .wait()
            .map_err(|e| DeployError::from(FileError::new(&self.program, e)));
        if let Some(monitor) = self.dump.take() {
            monitor.stop();
        }

        if let Some(monitor) = self.dump32.take() {
            monitor.stop();
        }
        // A Windows exit code is 32 bits. A crash code such as 0xC0000005 is above i32::MAX, so it keeps its bits as a negative value.
        Ok(i32::from_ne_bytes(outcome?.to_ne_bytes()))
    }
}

/// The DLLs this launch found: every `.dll` in the mod layer, and the static imports the launcher will point there.
pub fn dll_report(session: &Session, program: &Path) -> DllReport {
    let layers = session.layers();
    let (imports, _) = resolved_imports(&layers, program);
    DllReport {
        layer: super::session_layer_dlls(session),
        injected: imports
            .into_iter()
            .map(|import| InjectedDll {
                name: import.name,
                path: import.game_rel.replace('\\', "/"),
                source: import.source.display().to_string(),
            })
            .collect(),
    }
}

/// A DLL the program imports statically and the layer file that will satisfy it.
struct ResolvedImport {
    name: String,
    game_rel: String,
    source: PathBuf,
}

/// The static imports of `program` whose DLL is in the mod layer. An import whose layer file is missing is left to the
/// game's own folder, exactly as the loader would resolve it without the overlay. The second value names the reserved
/// imports that a mod does provide and that cannot be virtualized.
fn resolved_imports(layers: &Layers, program: &Path) -> (Vec<ResolvedImport>, Vec<String>) {
    let Some(folder) = program.parent() else {
        return (Vec::new(), Vec::new());
    };
    let Some(relative) = relative_to(
        &layers.root.display().to_string(),
        &folder.display().to_string(),
    ) else {
        return (Vec::new(), Vec::new());
    };
    let mut imports = Vec::new();
    let mut reserved = Vec::new();

    for import in gobbo_overlay_launch::imports(program).unwrap_or_default() {
        if import.name.is_empty() {
            continue;
        }

        let game_rel = join_rel(&relative, &import.name.replace('\\', "/"));

        if placement::reserved(&import.name) {
            // The hook loads this DLL before the game. Only report it when a mod actually has a copy that would be
            // skipped; every game imports ws2_32, and a warning without a mod copy would be noise on every launch.
            if layers.resolve(&game_rel, Path::is_file).is_some() {
                reserved.push(import.name);
            }

            continue;
        }

        let Some((layer, path)) = layers.resolve(&game_rel, Path::is_file) else {
            continue;
        };

        if layer != Layer::Root {
            imports.push(ResolvedImport {
                name: import.name,
                game_rel,
                source: path,
            });
        }
    }

    (imports, reserved)
}

/// The early redirect of a session: the game folder served from the mod layer. `None` when the session has no layer.
fn early_config(layers: &Layers) -> Result<Option<stage0::logic::Config>, DeployError> {
    let Some(staging) = layers.staging.as_ref() else {
        return Ok(None);
    };
    stage0::redirect_config(&layers.root, staging)
        .map(Some)
        .map_err(|error| FileError::new(&layers.root, error).into())
}

/// The static imports that will be patched, with the warning for a reserved name when a mod provides one.
fn static_imports_for(layers: &Layers, program: &Path) -> Vec<(String, PathBuf)> {
    let (imports, reserved) = resolved_imports(layers, program);

    for name in reserved {
        crate::infra::log::warn(format!(
            "{name} is loaded by the overlay hook before the game; a mod's copy is not virtualized"
        ));
    }

    imports
        .into_iter()
        .map(|import| (import.name, import.source))
        .collect()
}

/// Warnings for a plan that deploys a DLL the hook loads before the game. The loader keeps the system module under that
/// base name, so the mod's copy at the game path is not used. A file in a mod's own folder is not a game path and needs
/// no warning.
pub fn reserved_dll_warnings(plan: &DeployPlan) -> Vec<Message> {
    let mut seen = BTreeSet::new();
    let mut warnings = Vec::new();

    for file in &plan.files {
        if !file.game_path {
            continue;
        }

        let name = file
            .dest
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(file.dest.as_str());
        if !placement::reserved(name) || !seen.insert(file.dest.as_str()) {
            continue;
        }

        warnings.push(
            Message::new("preview.reserved_dll")
                .with("name", name)
                .with("path", &file.dest),
        );
    }

    warnings
}

/// Removes the copy-up temporary files that an interrupted copy left in the layers of the session. A copy-up writes its
/// temporary file beside the target and renames it into place, so a crash leaves a file of that shape behind. Nothing
/// runs from the game folder while this happens: a game that still runs may be copying, so the sweep waits for it to
/// stop. The hook hides these files in the meantime. A failure to remove one is not an error: the file stays hidden.
fn sweep_stale_copies(session: &Session) {
    if crate::platform::game_is_running(&session.root) {
        return;
    }

    let Ok(config) = mount_config(session) else {
        crate::infra::log::warn("overlay: invalid mount config; skipping the stale copy sweep");
        return;
    };

    for mount in config.mounts() {
        remove_copy_temps(&mount.upper);
    }
}

/// Removes the copy-up temporary files below `folder`. A symbolic link is not followed.
fn remove_copy_temps(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();

        if kind.is_dir() {
            remove_copy_temps(&path);
        } else if kind.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(gobbo_overlay_core::is_copy_temp_name)
        {
            let _ = std::fs::remove_file(&path);
        }
    }
}

mod dump;
mod dump32;
mod mounts;
mod placement;

use placement::{hook_dll, hook_dll32};

#[cfg(test)]
mod tests;
