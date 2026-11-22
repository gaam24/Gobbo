//! The probe checks themselves. Every check writes only under `__gobbo_probe_<pid>` and reads the physical layer copies
//! to prove what the hook did; the game folder must stay without that folder.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Error, Write};
use std::path::{Path, PathBuf};

use gobbo_overlay_core::Layers;

use crate::{Check, REPORT_FLAG, Report};

/// The Win32 refusal the hook returns when a hard link would change the game folder. The value is `ERROR_ACCESS_DENIED`
/// from `winerror.h`; it is written out rather than imported so this crate stays plain std on Linux.
const ERROR_ACCESS_DENIED: i32 = 5;

/// Runs the probe and prints/writes its JSON report.
pub fn main() {
    let mut report_path = None;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            REPORT_FLAG => report_path = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                println!("overlay-probe [{REPORT_FLAG} <path>]");
                return;
            }
            other => {
                eprintln!("overlay-probe: unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }

    let report = run();
    let text = match serde_json::to_string_pretty(&report) {
        Ok(text) => text,
        Err(error) => {
            let failed = Report::failed(
                "serialize",
                format!("could not serialize the report: {error}"),
            );
            serde_json::to_string_pretty(&failed).unwrap_or_else(|_| {
                "{\"ok\":false,\"probe\":null,\"layers\":null,\"checks\":[]}".to_string()
            })
        }
    };

    if let Some(path) = report_path
        && let Err(error) = fs::write(&path, &text)
    {
        eprintln!("overlay-probe: could not write {}: {error}", path.display());
        std::process::exit(2);
    }

    println!("{text}");

    if !report.ok {
        std::process::exit(1);
    }
}

/// Runs every check and returns the report.
fn run() -> Report {
    let Some(layers) = Layers::from_env(|name| std::env::var(name).ok()) else {
        return Report::failed(
            "layers",
            "GOBBO_OVERLAY_ROOT/STAGING/UPPER are missing".to_string(),
        );
    };
    let probe = format!("__gobbo_probe_{}", std::process::id());
    let game = layers.root.join(&probe);
    let upper = layers.upper.join(&probe);
    let Some(staging) = layers.staging.as_ref().map(|path| path.join(&probe)) else {
        return Report::failed("staging", "the session has no staging layer".to_string());
    };
    let mut checks = Vec::new();

    check(&mut checks, "layers", Ok(()));
    check(&mut checks, "staging fixture", make_staging(&staging));
    check(&mut checks, "create", create(&game, &upper));
    check(&mut checks, "read", read(&game));
    check(&mut checks, "append", append(&game, &upper));
    check(&mut checks, "metadata", metadata(&game));
    check(&mut checks, "rename", rename(&game, &upper));
    check(
        &mut checks,
        "final path",
        final_path(&game, &layers.root, &layers.upper),
    );
    check(&mut checks, "copy-up", copy_up(&game, &upper, &staging));
    check(&mut checks, "whiteout", whiteout(&game, &staging, &upper));
    check(&mut checks, "list", list(&game));
    check(&mut checks, "link refusal", link_refusal(&game));

    // Cleanup is best effort: a failed check has already been reported, and a unique folder name means a leftover
    // cannot affect the next run.
    let _ = fs::remove_dir_all(&game);
    let _ = fs::remove_dir_all(&staging);

    Report {
        ok: checks.iter().all(|item| item.ok),
        probe: Some(probe),
        layers: Some(BTreeMap::from([
            ("root".to_string(), layers.root.display().to_string()),
            (
                "staging".to_string(),
                layers
                    .staging
                    .as_ref()
                    .map_or_else(String::new, |path| path.display().to_string()),
            ),
            ("upper".to_string(), layers.upper.display().to_string()),
        ])),
        checks,
    }
}

/// Records one check.
fn check(checks: &mut Vec<Check>, name: &str, result: Result<(), String>) {
    checks.push(Check {
        name: name.to_string(),
        ok: result.is_ok(),
        detail: result.err().unwrap_or_default(),
    });
}

/// The two lower files the copy-up and whiteout checks need. They live in the mod layer, which is a Gobbo folder, not
/// the game folder, so writing them directly is allowed.
fn make_staging(staging: &Path) -> Result<(), String> {
    io(fs::create_dir_all(staging))?;
    io(fs::write(staging.join("lower.txt"), b"lower"))?;
    io(fs::write(staging.join("delete.txt"), b"delete"))?;
    Ok(())
}

/// A write through the game path lands in the upper layer.
fn create(game: &Path, upper: &Path) -> Result<(), String> {
    io(fs::create_dir_all(game))?;
    io(fs::write(game.join("file.txt"), b"one"))?;

    if !upper.join("file.txt").is_file() {
        return Err("the upper copy is missing".to_string());
    }

    Ok(())
}

/// A read sees the file just written.
fn read(game: &Path) -> Result<(), String> {
    let text = io(fs::read_to_string(game.join("file.txt")))?;
    (text == "one")
        .then_some(())
        .ok_or_else(|| format!("read {text:?}"))
}

/// An append keeps the old contents, so it also proves the upper copy is used.
fn append(game: &Path, upper: &Path) -> Result<(), String> {
    let mut file = io(OpenOptions::new().append(true).open(game.join("file.txt")))?;
    io(file.write_all(b"two"))?;
    let text = io(fs::read_to_string(game.join("file.txt")))?;
    (text == "onetwo")
        .then_some(())
        .ok_or_else(|| format!("append read {text:?}"))?;
    if !upper.join("file.txt").is_file() {
        return Err("the upper copy disappeared".to_string());
    }

    Ok(())
}

/// The attributes come from the visible copy.
fn metadata(game: &Path) -> Result<(), String> {
    let meta = io(fs::metadata(game.join("file.txt")))?;
    (meta.len() == 6)
        .then_some(())
        .ok_or_else(|| format!("size {}", meta.len()))
}

/// A rename through the game path changes the upper copy only.
fn rename(game: &Path, upper: &Path) -> Result<(), String> {
    io(fs::rename(game.join("file.txt"), game.join("renamed.txt")))?;

    if !upper.join("renamed.txt").is_file() {
        return Err("the renamed upper file is missing".to_string());
    }

    if upper.join("file.txt").exists() {
        return Err("the old upper file remains".to_string());
    }

    Ok(())
}

/// The final path of a layer handle answers with the game path, not the upper path.
fn final_path(game: &Path, root: &Path, upper: &Path) -> Result<(), String> {
    let shown = io(fs::canonicalize(game.join("renamed.txt")))?
        .to_string_lossy()
        .to_lowercase();
    let shown = shown.strip_prefix(r"\\?\").unwrap_or(&shown);
    let root = root.to_string_lossy().to_lowercase();
    let root = root.strip_prefix(r"\\?\").unwrap_or(&root);
    let upper = upper.to_string_lossy().to_lowercase();
    let upper = upper.strip_prefix(r"\\?\").unwrap_or(&upper);

    if !shown.starts_with(root) || shown.starts_with(upper) {
        return Err(format!("final path {shown}"));
    }

    Ok(())
}

/// An append to a lower-layer file copies it up first; the mod layer keeps its bytes.
fn copy_up(game: &Path, upper: &Path, staging: &Path) -> Result<(), String> {
    let mut file = io(OpenOptions::new().append(true).open(game.join("lower.txt")))?;
    io(file.write_all(b"copy"))?;
    let visible = io(fs::read_to_string(game.join("lower.txt")))?;
    let lower = io(fs::read_to_string(staging.join("lower.txt")))?;
    let copy = io(fs::read_to_string(upper.join("lower.txt")))?;

    if visible != "lowercopy" || lower != "lower" || copy != "lowercopy" {
        return Err(format!(
            "visible {visible:?}, lower {lower:?}, upper {copy:?}"
        ));
    }

    Ok(())
}

/// Deleting a lower-layer file hides it and leaves the mod layer alone.
fn whiteout(game: &Path, staging: &Path, upper: &Path) -> Result<(), String> {
    io(fs::remove_file(game.join("delete.txt")))?;

    if !staging.join("delete.txt").is_file() {
        return Err("the lower file was removed".to_string());
    }

    if fs::metadata(game.join("delete.txt")).is_ok() {
        return Err("the deleted file is still visible".to_string());
    }

    if upper.join("delete.txt").exists() {
        return Err("the delete left an upper file".to_string());
    }

    Ok(())
}

/// The listing merges the upper files and hides the whiteout.
fn list(game: &Path) -> Result<(), String> {
    for name in ["a.txt", "b.txt"] {
        io(fs::write(game.join(name), b"x"))?;
    }

    let mut names = Vec::new();

    for entry in io(fs::read_dir(game))? {
        let entry = io(entry)?;
        names.push(entry.file_name().to_string_lossy().to_ascii_lowercase());
    }

    for name in ["a.txt", "b.txt", "lower.txt", "renamed.txt"] {
        if !names.iter().any(|item| item == name) {
            return Err(format!("{name} missing from {names:?}"));
        }
    }

    if names.iter().any(|item| item == "delete.txt") {
        return Err("the whiteout is listed".to_string());
    }

    Ok(())
}

/// A hard link from outside the game to a name inside it is refused with `ERROR_ACCESS_DENIED`, the hook's documented
/// refusal. Any other failure is carried as the check's detail instead of being reported as a pass.
fn link_refusal(game: &Path) -> Result<(), String> {
    let source = std::env::temp_dir().join(format!("gobbo-probe-{}.txt", std::process::id()));
    io(fs::write(&source, b"source"))?;
    let target = game.join("hard.txt");
    let linked = fs::hard_link(&source, &target);
    let _ = fs::remove_file(&source);

    match linked {
        Ok(()) => {
            let _ = fs::remove_file(&target);
            Err("the hard link was created".to_string())
        }
        Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) => Ok(()),
        Err(error) => Err(format!("the hard link failed: {error}")),
    }
}

/// Turns an `io::Result` into the probe's error text.
fn io<T>(result: Result<T, Error>) -> Result<T, String> {
    result.map_err(|error| error.to_string())
}
