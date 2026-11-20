//! An 8.3-spelled game mount: a loader that reports a module by its short layer path must still make `GetModuleFileNameW`
//! answer the long guest path. The classification side already handles both spellings; this covers the handle mapping.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;
use crate::deploy::undo;
use crate::domain::user_data::{Kind, Mode, Set as UserDataSet};
use crate::test_support::{scratch, write};

/// The 8.3 alias of an existing path. A batch file's `%%~sI` is `GetShortPathNameW`; the app crate forbids `unsafe`, so
/// cmd makes the call. The batch file also keeps Rust's argument quoting from mangling the `cmd /c` command string.
/// `None` when the path does not exist or cmd cannot answer.
fn short_path(path: &Path) -> Option<PathBuf> {
    let script = std::env::temp_dir().join(format!("gobbo-short-name-{}.cmd", std::process::id()));
    fs::write(
        &script,
        "@echo off\r\nfor %%I in (\"%~1\") do @echo %%~sI\r\n",
    )
    .ok()?;
    let output = Command::new("cmd.exe")
        .args([
            "/c",
            &script.display().to_string(),
            &path.display().to_string(),
        ])
        .output()
        .ok()?;
    let _ = fs::remove_file(&script);

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then(|| PathBuf::from(text))
}

/// Builds a small x64 probe that reads through both mounts and asks Windows for the path of a DLL loaded by its short
/// layer path. The source is compiled at test time, so its `unsafe` Win32 calls are not part of the app crate.
fn build_probe(out: &Path, name: &str) -> PathBuf {
    const SOURCE: &str = r#"
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(path: *const u16) -> *mut c_void;
    fn GetModuleFileNameW(module: *mut c_void, buffer: *mut u16, size: u32) -> u32;
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = PathBuf::from(&args[1]);
    let short_guest = &args[2];
    let real = &args[3];
    let dll = PathBuf::from(&args[4]).into_os_string().encode_wide().chain(Some(0)).collect::<Vec<u16>>();

    let read = std::fs::read_to_string(format!("{short_guest}/archive/pc/mod/c.archive")).unwrap();
    let redirect = std::fs::read_to_string(format!("{real}/save.txt")).unwrap();
    std::fs::write(out.join("read.txt"), read).unwrap();
    std::fs::write(out.join("real.txt"), redirect).unwrap();

    let module = unsafe { LoadLibraryW(dll.as_ptr()) };
    if module.is_null() {
        std::process::exit(2);
    }
    let mut buffer = [0u16; 2048];
    let length = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 || length as usize >= buffer.len() {
        std::process::exit(3);
    }
    let name = String::from_utf16_lossy(&buffer[..length as usize]);
    std::fs::write(out.join("module.txt"), name).unwrap();
}
"#;
    let source = out.join(format!("{name}.rs"));
    fs::write(&source, SOURCE).unwrap();
    let probe = out.join(format!("{name}.exe"));
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .args(["--edition=2021", "-O", "-o"])
        .arg(&probe)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "the {name} probe did not build");
    probe
}

/// The short spelling of the game folder and of the staging DLL must both be real 8.3 aliases: when the volume does not
/// generate them the loader reports long paths and there is nothing to regress.
fn skip_without_short_names(
    short_root: Option<&PathBuf>,
    root: &Path,
    short_dll: Option<&PathBuf>,
    dll: &Path,
) -> bool {
    let distinct = |short: Option<&PathBuf>, long: &Path| {
        short.is_some_and(|short| {
            !short
                .display()
                .to_string()
                .eq_ignore_ascii_case(&long.display().to_string())
        })
    };

    if distinct(short_root, root) && distinct(short_dll, dll) {
        return false;
    }

    eprintln!("8.3 names are not generated on this volume; skipping the short-name hook test");
    true
}

#[test]
fn a_module_loaded_by_a_short_layer_path_reports_the_long_game_path() {
    const MODULE: &str = "#[no_mangle]\npub extern \"system\" fn probe() -> u32 { 42 }\n";

    let base = scratch!("short-names");
    let long_root = base.join("Game Folder Long Enough");
    let library = base.join("library");
    write(&long_root.join("archive/pc/mod/a.archive"), "game-a");
    write(&long_root.join("archive/pc/mod/b.archive"), "game-b");
    write(&library.join("modA/archive/pc/mod/c.archive"), "mod-c");
    let source = base.join("short-names-mod.rs");
    fs::write(&source, MODULE).unwrap();
    let dll = library.join("modA/archive/pc/mod/mod.dll");
    fs::create_dir_all(dll.parent().unwrap()).unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .args(["--edition=2021", "--crate-type", "cdylib", "-O", "-o"])
        .arg(&dll)
        .arg(&source)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "the short-name probe module did not build"
    );

    let mut target = crate::test_support::target(&base);
    target.root = long_root.clone();
    target.game_root = long_root.clone();
    apply(&overlay_plan(&library, &["modA"]), &library, &target).unwrap();

    let staged = target.state.join("mods/archive/pc/mod/mod.dll");
    let short_root = short_path(&long_root);
    let short_dll = short_path(&staged);
    if skip_without_short_names(short_root.as_ref(), &long_root, short_dll.as_ref(), &staged) {
        return;
    }
    let (Some(short_root), Some(short_dll)) = (short_root, short_dll) else {
        return;
    };

    // The game mount is spelled short; the user-data redirect keeps its long, real path.
    let real = base.join("documents").join("Game");
    let user_upper = base.join("state/userdata/documents/upper");
    fs::create_dir_all(&real).unwrap();
    fs::create_dir_all(&user_upper).unwrap();
    fs::write(real.join("save.txt"), "real").unwrap();
    let session = Session::with_roots(
        &target.state,
        &short_root,
        &short_root,
        vec![UserDataSet {
            id: "documents".to_string(),
            kind: Kind::Saves,
            real: real.clone(),
            mode: Mode::Redirect,
            isolated: true,
            inherit: true,
            base: real.clone(),
            upper: user_upper,
            store: base.join("state/userdata/documents/copy"),
        }],
    );

    let out = scratch!("short-names-out");
    let probe = build_probe(&out, "short-names");
    let code = run(
        &session,
        &probe.display().to_string(),
        &[
            out.display().to_string(),
            short_root.display().to_string(),
            real.display().to_string(),
            short_dll.display().to_string(),
        ],
    )
    .unwrap();
    assert_eq!(code, 0, "the short-name probe failed");

    // Guards: a layer read through the short-spelled mount and a long-spelled redirect still see their files.
    assert_eq!(fs::read_to_string(out.join("read.txt")).unwrap(), "mod-c");
    assert_eq!(fs::read_to_string(out.join("real.txt")).unwrap(), "real");

    // The assertion: the loader's short layer spelling is mapped to the one canonical long guest path.
    let reported = fs::read_to_string(out.join("module.txt")).unwrap();
    let expected = long_root
        .join(r"archive\pc\mod\mod.dll")
        .display()
        .to_string();
    assert!(
        reported.eq_ignore_ascii_case(&expected),
        "{reported} != {expected}"
    );

    undo(&target.journal_roots()).unwrap();
}
