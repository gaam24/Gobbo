//! DLLs from a mod, including a DLL the program imports statically. The game folder must keep no copy of it; the
//! loader reads it from the overlay layer because the hook is installed before the first instruction.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;
use crate::deploy::undo;
use crate::test_support::scratch;

/// A program that imports `mod.dll` statically. The import is resolved by the Windows loader before `main`, which is the
/// case the old remote-thread injection could not reach. With the game folder as its argument it also checks every API
/// that reports a module path: `GetModuleFileNameExW`, `GetMappedFileNameW` and `NtQueryInformationFile`.
const PROBE: &str = r#"
use std::ffi::c_void;

#[link(name = "mod", kind = "dylib")]
unsafe extern "C" {
    fn gobbo_probe_marker() -> u32;
}

#[link(name = "psapi")]
unsafe extern "system" {
    fn GetModuleFileNameExW(process: *mut c_void, module: *mut c_void, buffer: *mut u16, size: u32) -> u32;
    fn GetMappedFileNameW(process: *mut c_void, base: *mut c_void, buffer: *mut u16, size: u32) -> u32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        creation: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationFile(
        handle: *mut c_void,
        status: *mut c_void,
        info: *mut c_void,
        length: u32,
        class: u32,
    ) -> i32;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn text(buffer: &[u16], length: u32) -> String {
    String::from_utf16_lossy(&buffer[..length as usize])
}

fn same_path(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn module_path_checks(root: &str, arch: &str) -> i32 {
    let module = unsafe { GetModuleHandleW(wide("mod.dll").as_ptr()) };
    if module.is_null() {
        return 10;
    }
    let process = unsafe { GetCurrentProcess() };
    let mut buffer = [0u16; 1024];
    let expected = format!(r"{root}\bin\{arch}\mod.dll");
    let length = unsafe { GetModuleFileNameExW(process, module, buffer.as_mut_ptr(), 1024) };
    if length == 0 || !same_path(&text(&buffer, length), &expected) {
        return 11;
    }
    let length = unsafe { GetMappedFileNameW(process, module, buffer.as_mut_ptr(), 1024) };
    if length == 0 {
        return 12;
    }
    let tail = root.get(2..).unwrap_or(root);
    let expected_nt = format!(r"{tail}\bin\{arch}\mod.dll");
    if !text(&buffer, length)
        .to_ascii_lowercase()
        .ends_with(&expected_nt.to_ascii_lowercase())
    {
        return 13;
    }
    let handle = unsafe {
        CreateFileW(
            wide(&expected).as_ptr(),
            0x8000_0000,
            1 | 2 | 4,
            std::ptr::null(),
            3,
            0,
            std::ptr::null_mut(),
        )
    };
    if handle as isize == -1 {
        return 14;
    }
    let mut status = [0usize; 2];
    let mut info = [0usize; (8 + 2048 * 2usize).div_ceil(8)];
    let result = unsafe {
        NtQueryInformationFile(
            handle,
            status.as_mut_ptr().cast(),
            info.as_mut_ptr().cast(),
            (info.len() * std::mem::size_of::<usize>()) as u32,
            9,
        )
    };
    unsafe { CloseHandle(handle) };
    if result < 0 {
        return 15;
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(info.as_ptr().cast::<u8>(), info.len() * std::mem::size_of::<usize>())
    };
    let name_length = u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if name_length == 0 || 4 + name_length as usize > bytes.len() {
        return 16;
    }
    let name: Vec<u16> = bytes[4..4 + name_length as usize]
        .chunks_exact(2)
        .map(|pair| u16::from_ne_bytes([pair[0], pair[1]]))
        .collect();
    let actual = String::from_utf16_lossy(&name);
    if !actual
        .to_ascii_lowercase()
        .ends_with(&expected_nt.to_ascii_lowercase())
    {
        eprintln!(
            "NtQueryInformationFile name: {actual:?} ({} bytes), expected suffix {expected_nt:?}",
            name_length
        );
        return 17;
    }
    0
}

fn main() {
    let marker = unsafe { gobbo_probe_marker() };
    if marker != 42 {
        std::process::exit(4);
    }
    let args: Vec<String> = std::env::args().collect();
    if let (Some(root), Some(arch)) = (args.get(1), args.get(2)) {
        std::process::exit(module_path_checks(root, arch));
    }
}
"#;

const MOD_DLL: &str = r#"
#[unsafe(no_mangle)]
pub extern "C" fn gobbo_probe_marker() -> u32 {
    42
}
"#;

const GAME_DLL: &str = r#"
#[unsafe(no_mangle)]
pub extern "C" fn gobbo_probe_marker() -> u32 {
    7
}
"#;

/// A static import of a mod DLL works and leaves no file next to the program.
#[test]
fn a_program_statically_imports_a_dll_from_a_layer() {
    let Some(lib) = lib_exe() else {
        eprintln!("lib.exe is not available; skipping the static import test");
        return;
    };
    let (library, root, target) = fixture("static-import");
    let Some((dll, probe)) = build_fixture(&library, &root, "x64", "static-import-x64") else {
        eprintln!("the x64 fixture did not build; skipping the static import test");
        return;
    };
    assert!(dll.is_file());
    let session = deploy_mod_a(&library, &target);
    let code = run(&session, &probe.display().to_string(), &[]).unwrap();
    assert_eq!(code, 0, "the statically imported DLL did not run");
    assert!(
        !root.join("bin/x64/mod.dll").exists(),
        "the mod DLL was placed in the game folder"
    );
    let _ = lib;
    undo(&target.journal_roots()).unwrap();
}

/// Every API that reports a module path answers with the game path for a DLL loaded from a layer: the Windows module
/// path, the process-module path, the mapped file name and the NT file name of its handle.
#[test]
fn every_module_path_api_reports_the_game_path_for_a_layer_dll() {
    let Some(_lib) = lib_exe() else {
        eprintln!("lib.exe is not available; skipping the module path test");
        return;
    };
    let (library, root, target) = fixture("module-paths");
    let Some((_dll, probe)) = build_fixture(&library, &root, "x64", "module-paths-x64") else {
        eprintln!("the x64 fixture did not build; skipping the module path test");
        return;
    };
    let session = deploy_mod_a(&library, &target);
    let dlls = dll_report(&session, &probe);
    assert!(
        dlls.injected
            .iter()
            .any(|dll| dll.name.eq_ignore_ascii_case("mod.dll")),
        "{:?}",
        dlls.injected
    );
    assert!(
        dlls.layer.iter().any(|path| path == "bin/x64/mod.dll"),
        "{:?}",
        dlls.layer
    );
    let code = run(
        &session,
        &probe.display().to_string(),
        &[root.display().to_string(), "x64".to_string()],
    )
    .unwrap();
    assert_eq!(
        code, 0,
        "a module path API reported the layer path instead of the game path"
    );
    assert!(
        !root.join("bin/x64/mod.dll").exists(),
        "the mod DLL was placed in the game folder"
    );
    undo(&target.journal_roots()).unwrap();
}

/// A child process started by the game gets the same static-import redirect before it runs.
#[test]
fn a_child_process_statically_imports_a_dll_from_a_layer() {
    let Some(_lib) = lib_exe() else {
        eprintln!("lib.exe is not available; skipping the child static import test");
        return;
    };
    let (library, root, target) = fixture("static-import-child");
    let Some((_dll, probe)) = build_fixture(&library, &root, "x64", "static-import-child-x64")
    else {
        eprintln!("the x64 fixture did not build; skipping the child static import test");
        return;
    };
    let session = deploy_mod_a(&library, &target);
    let code = run(
        &session,
        r"C:\Windows\System32\cmd.exe",
        &["/c".to_string(), probe.display().to_string()],
    )
    .unwrap();
    assert_eq!(code, 0, "the child's statically imported DLL did not run");
    assert!(
        !root.join("bin/x64/mod.dll").exists(),
        "the child's mod DLL was placed in the game folder"
    );
    undo(&target.journal_roots()).unwrap();
}

/// A mod DLL that covers a game DLL of the same name is used by the static import, while the game's file stays in place.
#[test]
fn a_static_import_prefers_the_mod_layer_over_the_game_file() {
    let Some(_lib) = lib_exe() else {
        eprintln!("lib.exe is not available; skipping the static import override test");
        return;
    };
    let (library, root, target) = fixture("static-import-cover");
    let Some((_mod_dll, probe)) = build_fixture(&library, &root, "x64", "static-import-cover-x64")
    else {
        eprintln!("the x64 fixture did not build; skipping the static import override test");
        return;
    };
    let Some(game_dll) = build_game_dll(&root, "x64") else {
        eprintln!("the game DLL did not build; skipping the static import override test");
        return;
    };
    let before = fs::read(&game_dll).unwrap();
    let session = deploy_mod_a(&library, &target);
    let code = run(&session, &probe.display().to_string(), &[]).unwrap();
    assert_eq!(code, 0, "the mod DLL did not override the game DLL");
    assert_eq!(fs::read(&game_dll).unwrap(), before, "the game DLL changed");
    undo(&target.journal_roots()).unwrap();
}

/// The same check for a 32-bit program, when the 32-bit hook and injector are built.
#[test]
fn a_32_bit_program_statically_imports_a_dll_from_a_layer() {
    let Some(_lib) = lib_exe() else {
        eprintln!("lib.exe is not available; skipping the 32-bit static import test");
        return;
    };
    let (library, root, target) = fixture("static-import-32");
    let Some((dll, probe)) = build_fixture(&library, &root, "x86", "static-import-32-x86") else {
        eprintln!("the x86 fixture did not build; skipping the 32-bit static import test");
        return;
    };
    assert!(dll.is_file());

    if adjacent_binary("gobbo_overlay_hook32.dll").is_none()
        || adjacent_binary("gobbo_overlay_inject32.exe").is_none()
    {
        eprintln!(
            "the 32-bit hook or injector is not built; skipping the 32-bit static import test"
        );
        return;
    }

    let session = deploy_mod_a(&library, &target);
    let code = run(
        &session,
        &probe.display().to_string(),
        &[root.display().to_string(), "x86".to_string()],
    )
    .unwrap();
    assert_eq!(code, 0, "the statically imported 32-bit DLL did not run");
    assert!(
        !root.join("bin/x86/mod.dll").exists(),
        "the 32-bit mod DLL was placed in the game folder"
    );
    undo(&target.journal_roots()).unwrap();
}

/// Builds `mod.dll` in the library and `probe.exe` in the game folder. `arch` is `x64` or `x86`.
fn build_fixture(
    library: &Path,
    root: &Path,
    arch: &str,
    scratch_name: &str,
) -> Option<(PathBuf, PathBuf)> {
    let target = if arch == "x86" {
        "i686-pc-windows-msvc"
    } else {
        "x86_64-pc-windows-msvc"
    };
    let out = scratch!(scratch_name);
    let dll_source = out.join(format!("mod-{arch}.rs"));
    fs::write(&dll_source, MOD_DLL).unwrap();
    let dll = library.join(format!("modA/bin/{arch}/mod.dll"));
    fs::create_dir_all(dll.parent()?).unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(&rustc)
        .args([
            "--edition=2021",
            "--crate-type",
            "cdylib",
            "-O",
            "--target",
            target,
            "-o",
        ])
        .arg(&dll)
        .arg(&dll_source)
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }

    let def = out.join("mod.def");
    fs::write(&def, "LIBRARY mod\nEXPORTS\n    gobbo_probe_marker\n").unwrap();
    let import = out.join("mod.lib");
    let machine = if arch == "x86" { "x86" } else { "x64" };
    let lib = lib_exe()?;
    let status = Command::new(lib)
        .arg(format!("/DEF:{}", def.display()))
        .arg(format!("/OUT:{}", import.display()))
        .arg(format!("/MACHINE:{machine}"))
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }

    let probe_source = out.join(format!("probe-{arch}.rs"));
    fs::write(&probe_source, PROBE).unwrap();
    let probe = root.join(format!("bin/{arch}/probe.exe"));
    fs::create_dir_all(probe.parent()?).unwrap();
    let status = Command::new(&rustc)
        .args([
            "--edition=2021",
            "-O",
            "--target",
            target,
            "-L",
            &format!("native={}", out.display()),
            "-o",
        ])
        .arg(&probe)
        .arg(&probe_source)
        .status()
        .ok()?;
    status.success().then_some((dll, probe))
}

/// Builds the game's own `mod.dll`, which exports a different marker. It stays in the game folder; the patch must make
/// the loader read the mod layer instead.
fn build_game_dll(root: &Path, arch: &str) -> Option<PathBuf> {
    let target = if arch == "x86" {
        "i686-pc-windows-msvc"
    } else {
        "x86_64-pc-windows-msvc"
    };
    let out = if arch == "x86" {
        scratch!("static-import-game-dll-x86")
    } else {
        scratch!("static-import-game-dll-x64")
    };
    let source = out.join("game.rs");
    fs::write(&source, GAME_DLL).unwrap();
    let dll = root.join(format!("bin/{arch}/mod.dll"));
    fs::create_dir_all(dll.parent()?).unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(&rustc)
        .args([
            "--edition=2021",
            "--crate-type",
            "cdylib",
            "-O",
            "--target",
            target,
            "-o",
        ])
        .arg(&dll)
        .arg(&source)
        .status()
        .ok()?;
    status.success().then_some(dll)
}

/// A helper binary next to the test executable or in the Cargo dev layout above it.
fn adjacent_binary(name: &str) -> Option<PathBuf> {
    crate::infra::binaries::adjacent(name)
}

/// A DLL of the game folder that imports `mod.dll` and exports one marker, which returns the marker of `mod.dll`.
const DEPENDENCY_DLL: &str = r#"
#[link(name = "mod", kind = "dylib")]
unsafe extern "C" {
    fn gobbo_probe_marker() -> u32;
}

#[unsafe(no_mangle)]
pub extern "C" fn gobbo_dep_marker() -> u32 {
    unsafe { gobbo_probe_marker() }
}
"#;

/// A program that imports only the game's DLL, so `mod.dll` is reached through the program's import tree.
const DEPENDENCY_PROBE: &str = r#"
#[link(name = "dep", kind = "dylib")]
unsafe extern "C" {
    fn gobbo_dep_marker() -> u32;
}

fn main() {
    std::process::exit(if unsafe { gobbo_dep_marker() } == 42 { 0 } else { 4 });
}
"#;

/// Compiles `source` for `target` into `output`: a DLL when `dll` is set, else a program. `native` holds the import
/// libraries the source links to.
fn compile(source: &Path, output: &Path, target: &str, native: &Path, dll: bool) -> Option<()> {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let mut command = Command::new(&rustc);
    command.args(["--edition=2021", "-O", "--target", target]);

    if dll {
        command.args(["--crate-type", "cdylib"]);
    }

    let status = command
        .arg("-L")
        .arg(format!("native={}", native.display()))
        .arg("-o")
        .arg(output)
        .arg(source)
        .status()
        .ok()?;
    status.success().then_some(())
}

/// Writes the import library `<name>.lib` in `out` for one export, with `lib.exe`.
fn import_library(lib: &Path, out: &Path, name: &str, export: &str, machine: &str) -> Option<()> {
    let def = out.join(format!("{name}.def"));
    fs::write(&def, format!("LIBRARY {name}\nEXPORTS\n    {export}\n")).unwrap();
    let status = Command::new(lib)
        .arg(format!("/DEF:{}", def.display()))
        .arg(format!(
            "/OUT:{}",
            out.join(format!("{name}.lib")).display()
        ))
        .arg(format!("/MACHINE:{machine}"))
        .status()
        .ok()?;
    status.success().then_some(())
}

/// Builds `mod.dll` into the library, the game's `dep.dll` next to the program, and the program that imports `dep.dll`.
/// Returns the program.
fn build_dependency_fixture(
    lib: &Path,
    library: &Path,
    root: &Path,
    scratch_name: &str,
) -> Option<PathBuf> {
    let target = "x86_64-pc-windows-msvc";
    let out = scratch!(scratch_name);
    let mod_source = out.join("mod.rs");
    fs::write(&mod_source, MOD_DLL).unwrap();
    let mod_dll = library.join("modA/bin/x64/mod.dll");
    fs::create_dir_all(mod_dll.parent()?).unwrap();
    compile(&mod_source, &mod_dll, target, &out, true)?;
    import_library(lib, &out, "mod", "gobbo_probe_marker", "x64")?;

    let dep_source = out.join("dep.rs");
    fs::write(&dep_source, DEPENDENCY_DLL).unwrap();
    let dep_dll = root.join("bin/x64/dep.dll");
    fs::create_dir_all(dep_dll.parent()?).unwrap();
    compile(&dep_source, &dep_dll, target, &out, true)?;
    import_library(lib, &out, "dep", "gobbo_dep_marker", "x64")?;

    let probe_source = out.join("dep-probe.rs");
    fs::write(&probe_source, DEPENDENCY_PROBE).unwrap();
    let probe = root.join("bin/x64/dep_probe.exe");
    compile(&probe_source, &probe, target, &out, false)?;
    Some(probe)
}

/// A DLL of the game folder imports `mod.dll`, which no folder of the game or the system has. The copy is in the layer,
/// and the program imports only the game DLL, so the loader reaches `mod.dll` through the program's import tree. The
/// layer folder is on the search path of the program, so the loader finds it there.
#[test]
fn a_dll_the_program_imports_gets_its_own_dependency_from_a_layer() {
    let Some(lib) = lib_exe() else {
        eprintln!("lib.exe is not available; skipping the dependency import test");
        return;
    };
    let (library, root, target) = fixture("dependency-import");
    let Some(probe) = build_dependency_fixture(&lib, &library, &root, "dependency-layer-x64")
    else {
        eprintln!("the dependency fixture did not build; skipping the dependency import test");
        return;
    };
    let session = deploy_mod_a(&library, &target);
    let code = run(&session, &probe.display().to_string(), &[]).unwrap();
    assert_eq!(
        code, 0,
        "the game DLL's dependency was not found in the layer"
    );
    assert!(
        !root.join("bin/x64/mod.dll").exists(),
        "the mod DLL was placed in the game folder"
    );
    undo(&target.journal_roots()).unwrap();
}

/// `lib.exe` from the Visual Studio installation, found with `vswhere`. A machine without the C++ tools skips the
/// static import tests instead of failing the whole suite.
fn lib_exe() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("LIB_EXE") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }

    let vswhere =
        Path::new(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe");
    let output = Command::new(vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationPath",
        ])
        .output()
        .ok()?;
    let root = String::from_utf8(output.stdout).ok()?;
    let root = Path::new(root.trim());
    let tools = root.join("VC/Tools/MSVC");

    for entry in fs::read_dir(tools).ok()? {
        let entry = entry.ok()?;
        let candidate = entry.path().join("bin/Hostx64/x64/lib.exe");

        if candidate.is_file() {
            return Some(candidate);
        }
    }

    None
}
