//! The calls a program makes through the hook: environment, links, renames, delete-on-close and final paths.

use std::fs;

use super::*;
use crate::deploy::overlay::run;
use crate::deploy::undo;
use crate::domain::user_data::{Kind, Mode, Set as UserDataSet};
use crate::test_support::scratch;

#[test]
#[allow(
    clippy::literal_string_with_formatting_args,
    reason = "the probe source contains format placeholders"
)]
fn a_user_data_redirect_keeps_the_real_folder_untouched() {
    let base = scratch!("redirect-user-data");
    let root = base.join("game");
    let real = base.join("documents").join("Game");
    let upper = base.join("state/userdata/documents/upper");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&real).unwrap();
    fs::create_dir_all(&upper).unwrap();
    fs::create_dir_all(base.join("state/mods")).unwrap();
    fs::write(real.join("save.txt"), "real").unwrap();

    let session = Session::with_roots(
        &base.join("state"),
        &root,
        &root,
        vec![UserDataSet {
            id: "documents".to_string(),
            kind: Kind::Saves,
            real: real.clone(),
            mode: Mode::Redirect,
            isolated: true,
            inherit: true,
            base: real.clone(),
            upper: upper.clone(),
            store: base.join("state/userdata/documents/copy"),
        }],
    );

    let out = base.join("seen.txt");
    let appended = base.join("appended.txt");
    let probe = build_probe(
        &base,
        "redirect",
        r#"
use std::fs;
use std::io::Write;
fn main() {
    let real = std::env::args().nth(1).unwrap();
    let out = std::env::args().nth(2).unwrap();
    let appended = std::env::args().nth(3).unwrap();
    let before = fs::read_to_string(format!("{real}/save.txt")).unwrap();
    let mut file = fs::OpenOptions::new().append(true).open(format!("{real}/save.txt")).unwrap();
    file.write_all(b"+").unwrap();
    drop(file);
    let after_append = fs::read_to_string(format!("{real}/save.txt")).unwrap();
    fs::write(format!("{real}/save.txt"), "changed").unwrap();
    fs::write(format!("{real}/new.txt"), "profile").unwrap();
    fs::write(out, before).unwrap();
    fs::write(appended, after_append).unwrap();
}
"#,
    );
    let code = run(
        &session,
        &probe.display().to_string(),
        &[
            real.display().to_string(),
            out.display().to_string(),
            appended.display().to_string(),
        ],
    )
    .unwrap();
    assert_eq!(code, 0, "the redirect probe failed");
    assert_eq!(fs::read_to_string(&out).unwrap(), "real");
    assert_eq!(fs::read_to_string(&appended).unwrap(), "real+");
    assert_eq!(fs::read_to_string(real.join("save.txt")).unwrap(), "real");
    assert!(!real.join("new.txt").exists(), "the real folder got a file");
    assert_eq!(
        fs::read_to_string(upper.join("save.txt")).unwrap(),
        "changed"
    );
    assert_eq!(
        fs::read_to_string(upper.join("new.txt")).unwrap(),
        "profile"
    );
}

#[test]
#[allow(
    clippy::literal_string_with_formatting_args,
    reason = "the probe source contains format placeholders"
)]
fn a_user_data_redirect_can_start_without_the_existing_data() {
    let base = scratch!("redirect-empty-base");
    let root = base.join("game");
    let real = base.join("documents").join("Game");
    let empty = base.join("state/userdata/documents/base");
    let upper = base.join("state/userdata/documents/upper");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&real).unwrap();
    fs::create_dir_all(&empty).unwrap();
    fs::create_dir_all(&upper).unwrap();
    fs::create_dir_all(base.join("state/mods")).unwrap();
    fs::write(real.join("save.txt"), "real").unwrap();

    let session = Session::with_roots(
        &base.join("state"),
        &root,
        &root,
        vec![UserDataSet {
            id: "documents".to_string(),
            kind: Kind::Saves,
            real: real.clone(),
            mode: Mode::Redirect,
            isolated: true,
            inherit: false,
            base: empty,
            upper: upper.clone(),
            store: base.join("state/userdata/documents/copy"),
        }],
    );

    let out = base.join("seen.txt");
    let probe = build_probe(
        &base,
        "redirect-empty",
        r#"
use std::fs;
fn main() {
    let real = std::env::args().nth(1).unwrap();
    let out = std::env::args().nth(2).unwrap();
    let seen = match fs::read_to_string(format!("{real}/save.txt")) {
        Ok(value) => value,
        Err(_) => "missing".to_string(),
    };
    fs::write(format!("{real}/new.txt"), "profile").unwrap();
    fs::write(out, seen).unwrap();
}
"#,
    );
    let code = run(
        &session,
        &probe.display().to_string(),
        &[real.display().to_string(), out.display().to_string()],
    )
    .unwrap();
    assert_eq!(code, 0, "the empty-base probe failed");
    assert_eq!(fs::read_to_string(&out).unwrap(), "missing");
    assert_eq!(fs::read_to_string(real.join("save.txt")).unwrap(), "real");
    assert!(!real.join("new.txt").exists(), "the real folder got a file");
    assert_eq!(
        fs::read_to_string(upper.join("new.txt")).unwrap(),
        "profile"
    );
}

#[test]
fn a_native_crash_sends_a_dump_to_gobbo() {
    let (library, _root, target) = fixture("dump");
    let session = deploy_mod_a(&library, &target);
    let out = scratch!("dump-out");
    let source = out.join("crash.rs");
    fs::write(
        &source,
        "fn main() { std::thread::sleep(std::time::Duration::from_secs(3)); unsafe { std::ptr::write_volatile(std::ptr::null_mut::<u8>(), 1); } }\n",
    )
    .unwrap();
    let probe = out.join("crash.exe");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = std::process::Command::new(rustc)
        .args(["--edition=2021", "-O", "-o"])
        .arg(&probe)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "the crash probe did not build");

    let code = run(&session, &probe.display().to_string(), &[]).unwrap();
    assert_ne!(code, 0, "the probe must not exit cleanly");
    let dump = crate::deploy::overlay::latest_dump(&target.state)
        .expect("the out-of-process minidumper writes a dump");
    assert!(std::path::Path::new(&dump).is_file(), "{dump}");
    assert!(fs::metadata(&dump).unwrap().len() > 0, "{dump}");
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn the_overlay_probe_reports_a_clean_session() {
    let (library, root, target) = fixture("probe");
    let session = deploy_mod_a(&library, &target);
    let Some(probe) = adjacent_binary("overlay-probe.exe") else {
        eprintln!("overlay-probe.exe is not built next to the tests; skipping");
        return;
    };
    let report = target.state.join("probe.json");
    let code = run(
        &session,
        &probe.display().to_string(),
        &["--report".to_string(), report.display().to_string()],
    )
    .unwrap();
    assert_eq!(code, 0, "the probe reported a failure");
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report).unwrap()).unwrap();
    assert_eq!(report["ok"], true, "{report}");
    let probe = report["probe"].as_str().unwrap_or_default();
    assert!(!root.join(probe).exists(), "the game folder got {probe}");
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn the_32_bit_overlay_probe_reports_a_clean_session() {
    let (library, root, target) = fixture("probe32");
    let session = deploy_mod_a(&library, &target);
    let Some(probe) = adjacent_binary("gobbo_overlay_probe32.exe") else {
        eprintln!("gobbo_overlay_probe32.exe is not built next to the tests; skipping");
        return;
    };
    let report = target.state.join("probe32.json");
    let code = run(
        &session,
        &probe.display().to_string(),
        &["--report".to_string(), report.display().to_string()],
    )
    .unwrap();
    assert_eq!(code, 0, "the 32-bit probe reported a failure");
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report).unwrap()).unwrap();
    assert_eq!(report["ok"], true, "{report}");
    let probe = report["probe"].as_str().unwrap_or_default();
    assert!(!root.join(probe).exists(), "the game folder got {probe}");
    undo(&target.journal_roots()).unwrap();
}

/// Builds a small x64 probe program from `source` in `out`.
fn build_probe(out: &std::path::Path, name: &str, source: &str) -> std::path::PathBuf {
    let source_path = out.join(format!("{name}.rs"));
    fs::write(&source_path, source).unwrap();
    let probe = out.join(format!("{name}.exe"));
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = std::process::Command::new(rustc)
        .args(["--edition=2021", "-O", "-o"])
        .arg(&probe)
        .arg(&source_path)
        .status()
        .unwrap();
    assert!(status.success(), "the {name} probe did not build");
    probe
}

/// Without the out-of-process monitor, the in-process `DbgHelp` dump is the fallback.
#[test]
fn a_crash_without_a_monitor_writes_the_in_process_dump() {
    let (library, _root, target) = fixture("no-monitor-dump");
    let session = deploy_mod_a(&library, &target);
    let out = scratch!("no-monitor-dump-out");
    let probe = build_probe(
        &out,
        "crash-no-monitor",
        "fn main() { std::thread::sleep(std::time::Duration::from_secs(1)); unsafe { std::ptr::write_volatile(std::ptr::null_mut::<u8>(), 1); } }\n",
    );
    let mut running = start(
        &session,
        &probe.display().to_string(),
        &[],
        &[("GOBBO_OVERLAY_DUMPER".to_string(), "off".to_string())],
        None,
    )
    .unwrap();
    assert_ne!(
        running.wait().unwrap(),
        0,
        "the probe must not exit cleanly"
    );
    let dump = crate::deploy::overlay::latest_dump(&target.state)
        .expect("the in-process fallback writes the dump");
    assert!(std::path::Path::new(&dump).is_file(), "{dump}");
    assert!(fs::metadata(&dump).unwrap().len() > 0, "{dump}");
    undo(&target.journal_roots()).unwrap();
}

/// A helper binary next to the test executable or in the Cargo dev layout above it.
fn adjacent_binary(name: &str) -> Option<std::path::PathBuf> {
    crate::infra::binaries::adjacent(name)
}

#[test]
fn a_32_bit_crash_sends_a_dump_to_gobbo() {
    let (library, _root, target) = fixture("dump32");
    let session = deploy_mod_a(&library, &target);
    let out = scratch!("dump32-out");
    let source = out.join("crash.rs");
    fs::write(
        &source,
        "fn main() { std::thread::sleep(std::time::Duration::from_secs(3)); unsafe { std::ptr::write_volatile(std::ptr::null_mut::<u8>(), 1); } }\n",
    )
    .unwrap();
    let probe = out.join("crash32.exe");
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = std::process::Command::new(rustc)
        .args([
            "--edition=2021",
            "-O",
            "--target",
            "i686-pc-windows-msvc",
            "-o",
        ])
        .arg(&probe)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "the 32-bit crash probe did not build");

    let code = run(&session, &probe.display().to_string(), &[]).unwrap();
    assert_ne!(code, 0, "the 32-bit probe must not exit cleanly");
    let dump = crate::deploy::overlay::latest_dump(&target.state)
        .expect("the 32-bit helper writes a dump");
    assert!(std::path::Path::new(&dump).is_file(), "{dump}");
    assert!(fs::metadata(&dump).unwrap().len() > 0, "{dump}");
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn a_program_calling_delete_file_hides_the_file_and_keeps_the_game_copy() {
    // .NET deletes through `DeleteFileW`, which the hook covers directly; cmd's `del` uses the NT layer.
    let (library, root, target) = fixture("win32-delete");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let out = scratch!("win32-delete-out");
    let script = format!(
        "[IO.File]::Delete('{b}'); [IO.File]::Exists('{b}') | Out-File -Encoding ascii '{out}\\state.txt'",
        b = folder.join("b.archive").display(),
        out = out.display(),
    );
    run(
        &session,
        POWERSHELL,
        &["-NoProfile".to_string(), "-Command".to_string(), script],
    )
    .unwrap();

    assert_game_copy_kept(&out, &folder);
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn a_started_program_gets_the_extra_environment_and_its_exit_code() {
    // The game is started in this process: `start` returns a handle, and `wait` gives the exit code.
    let (library, root, target) = fixture("start-wait");
    let session = deploy_mod_a(&library, &target);
    let out = scratch!("start-wait-out");
    let script = out.join("probe.cmd");
    fs::write(
        &script,
        format!(
            "@echo off\r\necho %GOBBO_TEST_EXTRA%> \"{}\\env.txt\"\r\nexit /b 3\r\n",
            out.display()
        ),
    )
    .unwrap();

    let mut running = start(
        &session,
        r"C:\Windows\System32\cmd.exe",
        &["/c".to_string(), script.display().to_string()],
        &[("GOBBO_TEST_EXTRA".to_string(), "from-gobbo".to_string())],
        None,
    )
    .unwrap();
    assert_eq!(running.wait().unwrap(), 3);
    assert_eq!(
        fs::read_to_string(out.join("env.txt")).unwrap().trim(),
        "from-gobbo"
    );
    assert!(root.join("archive/pc/mod/a.archive").is_file());
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn links_into_the_game_folder_are_refused() {
    let (library, root, target) = fixture("links");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let out = scratch!("links-out");
    run_batch(
        &session,
        &out,
        &[
            format!("echo x > \"{}\"", out.join("src.txt").display()),
            format!(
                "mklink /H \"{}\" \"{}\" > nul 2>&1",
                out.join("ok.txt").display(),
                out.join("src.txt").display()
            ),
            format!(
                "mklink /H \"{}\" \"{}\" > nul 2>&1",
                folder.join("hard.archive").display(),
                folder.join("a.archive").display()
            ),
            format!(
                "mklink \"{}\" \"{}\" > nul 2>&1",
                folder.join("sym.archive").display(),
                folder.join("a.archive").display()
            ),
        ],
    );

    // The control: a hard link outside the game folder works, so the refusals below come from the hook.
    assert!(out.join("ok.txt").is_file());
    assert!(!folder.join("hard.archive").exists());
    assert!(!folder.join("sym.archive").exists());
    assert!(
        !session
            .overwrite
            .join("archive/pc/mod/hard.archive")
            .exists()
    );
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn replace_file_through_the_layers_keeps_the_game_copy() {
    // .NET's File.Replace calls `ReplaceFileW`: the game's file is replaced in the upper layer.
    const REPLACE: &str = r#"
$ErrorActionPreference = "Stop"
$folder = $args[0]
$out = $args[1]
$tmp = Join-Path $folder "save.tmp"
$target = Join-Path $folder "b.archive"
$backup = Join-Path $out "b.bak"
try {
[IO.File]::WriteAllText($tmp, "new")
[IO.File]::Replace($tmp, $target, $backup)
[IO.File]::ReadAllText($target) | Out-File -Encoding ascii (Join-Path $out "state.txt")
[IO.File]::Exists($tmp) | Out-File -Encoding ascii -Append (Join-Path $out "state.txt")
} catch {
$_.Exception.ToString() | Out-File -Encoding ascii (Join-Path $out "error.txt")
exit 1
}
"#;
    let (library, root, target) = fixture("replace");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let out = scratch!("replace-out");
    let script = out.join("replace.ps1");
    fs::write(&script, REPLACE).unwrap();

    run_script_file(
        &session,
        &script,
        &[folder.display().to_string(), out.display().to_string()],
    );

    let state = fs::read_to_string(out.join("state.txt")).unwrap();
    let lines: Vec<&str> = state.lines().map(str::trim).collect();
    assert_eq!(lines, ["new", "False"]);
    assert_eq!(
        fs::read_to_string(folder.join("b.archive")).unwrap(),
        "game-b"
    );
    assert_eq!(
        fs::read_to_string(session.overwrite.join("archive/pc/mod/b.archive"))
            .unwrap()
            .trim(),
        "new"
    );
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn a_file_marked_for_deletion_through_its_handle_is_hidden_too() {
    // `SetFileInformationByHandle` with the disposition info is how a program deletes through an open handle.
    const MARK: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class Handles {
[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern IntPtr CreateFileW(string name, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template);
[DllImport("kernel32.dll", SetLastError = true)]
public static extern bool SetFileInformationByHandle(IntPtr handle, int infoClass, ref byte info, uint size);
[DllImport("kernel32.dll")]
public static extern bool CloseHandle(IntPtr handle);
public static void MarkForDelete(string path) {
    IntPtr handle = CreateFileW(path, 0x10000, 0x7, IntPtr.Zero, 3, 0, IntPtr.Zero);
    if (handle == new IntPtr(-1)) throw new Win32Exception(Marshal.GetLastWin32Error());
    byte flag = 1;
    bool done = SetFileInformationByHandle(handle, 4, ref flag, 1);
    int error = Marshal.GetLastWin32Error();
    CloseHandle(handle);
    if (!done) throw new Win32Exception(error);
}
}
'@
[Handles]::MarkForDelete($args[0])
[IO.File]::Exists($args[0]) | Out-File -Encoding ascii $args[1]
"#;
    let (library, root, target) = fixture("handle-delete");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let out = scratch!("handle-delete-out");
    let script = out.join("mark.ps1");
    fs::write(&script, MARK).unwrap();

    run_script_file(
        &session,
        &script,
        &[
            folder.join("b.archive").display().to_string(),
            out.join("state.txt").display().to_string(),
        ],
    );

    assert_game_copy_kept(&out, &folder);
    undo(&target.journal_roots()).unwrap();
}

/// Lists a folder through `NtQueryDirectoryFile` (class `FileIdBothDirectoryInformation`) and returns the sorted names.
const NT_LIST: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class NtList {
[StructLayout(LayoutKind.Sequential)]
public struct IoStatus { public int Status; public IntPtr Information; }
[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern IntPtr CreateFileW(string name, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template);
[DllImport("ntdll.dll")]
public static extern int NtQueryDirectoryFile(IntPtr handle, IntPtr evt, IntPtr apc, IntPtr context, out IoStatus status, IntPtr info, uint length, uint cls, byte single, IntPtr name, byte restart);
[DllImport("kernel32.dll")]
public static extern bool CloseHandle(IntPtr handle);
public static string[] Of(string folder) {
    IntPtr handle = CreateFileW(folder, 0x1, 7, IntPtr.Zero, 3, 0x02000000, IntPtr.Zero);
    if (handle == new IntPtr(-1)) throw new Exception("open " + Marshal.GetLastWin32Error());
    var names = new List<string>();
    IntPtr buffer = Marshal.AllocHGlobal(65536);
    try {
        int status;
        byte restart = 1;
        do {
            IoStatus io;
            status = NtQueryDirectoryFile(handle, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, out io, buffer, 65536, 37, 0, IntPtr.Zero, restart);
            restart = 0;
            if (status == 0) {
                int at = 0;
                while (true) {
                    int next = Marshal.ReadInt32(buffer, at);
                    int length = Marshal.ReadInt32(buffer, at + 60);
                    names.Add(Marshal.PtrToStringUni(new IntPtr(buffer.ToInt64() + at + 104), length / 2));
                    if (next == 0) break;
                    at += next;
                }
            } else if (status != unchecked((int)0x80000006)) {
                throw new Exception("status " + status.ToString("x8"));
            }
        } while (status == 0);
    } finally {
        Marshal.FreeHGlobal(buffer);
        CloseHandle(handle);
    }
    return names.ToArray();
}
}
'@
[NtList]::Of($args[0]) | Sort-Object | Out-File -Encoding ascii $args[1]
"#;

fn nt_list(session: &Session, folder: &std::path::Path, out: &std::path::Path) -> Vec<String> {
    let script = out.join("list.ps1");
    fs::write(&script, NT_LIST).unwrap();
    run_script_file(
        session,
        &script,
        &[
            folder.display().to_string(),
            out.join("names.txt").display().to_string(),
        ],
    );
    fs::read_to_string(out.join("names.txt"))
        .unwrap()
        .lines()
        .map(|line| line.trim().to_string())
        .collect()
}

/// A DLL loaded from a layer must report the game path from `GetModuleFileNameW` and `A`, not the state folder. The
/// module is loaded by its real layer path, so the answer comes from the hook rather than from the loader's own input.
#[test]
fn a_module_loaded_from_a_layer_reports_the_game_path() {
    const MODULE: &str = "#[no_mangle]\npub extern \"system\" fn probe() -> u32 { 42 }\n";
    const ASK: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class ModulePath {
[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern IntPtr LoadLibraryW(string path);
[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern uint GetModuleFileNameW(IntPtr module, StringBuilder buffer, uint size);
[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Ansi)]
public static extern uint GetModuleFileNameA(IntPtr module, StringBuilder buffer, uint size);
static string Name(IntPtr module, bool wide) {
    var text = new StringBuilder(1024);
    uint length = wide ? GetModuleFileNameW(module, text, 1024) : GetModuleFileNameA(module, text, 1024);
    if (length == 0 || length >= 1024) throw new Exception("name " + Marshal.GetLastWin32Error());
    return text.ToString(0, (int)length);
}
public static string[] Of(string path) {
    IntPtr handle = LoadLibraryW(path);
    if (handle == IntPtr.Zero) throw new Exception("load " + Marshal.GetLastWin32Error());
    return new[] { Name(handle, true), Name(handle, false) };
}
}
'@
[ModulePath]::Of($args[0]) | Out-File -Encoding ascii $args[1]
"#;
    let (library, root, target) = fixture("module-name");
    let out = scratch!("module-name-out");
    let source = out.join("mod.rs");
    fs::write(&source, MODULE).unwrap();
    let dll = library.join(r"modA\archive\pc\mod\mod.dll");
    fs::create_dir_all(dll.parent().unwrap()).unwrap();
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = std::process::Command::new(rustc)
        .args(["--edition=2021", "--crate-type", "cdylib", "-O", "-o"])
        .arg(&dll)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success(), "the probe module did not build");

    let session = deploy_mod_a(&library, &target);
    let script = out.join("ask.ps1");
    fs::write(&script, ASK).unwrap();
    run_script_file(
        &session,
        &script,
        &[
            session
                .mods
                .join(r"archive\pc\mod\mod.dll")
                .display()
                .to_string(),
            out.join("module.txt").display().to_string(),
        ],
    );

    let reported = fs::read_to_string(out.join("module.txt")).unwrap();
    let expected = root.join(r"archive\pc\mod\mod.dll").display().to_string();
    let lines: Vec<&str> = reported.lines().map(str::trim).collect();
    assert_eq!(lines.len(), 2, "{reported}");
    for line in lines {
        assert!(line.eq_ignore_ascii_case(&expected), "{line} != {expected}");
    }
    undo(&target.journal_roots()).unwrap();
}

/// With `CreateProcessInternalW` disabled, the documented `CreateProcessW` fallback must still hook the child.
#[test]
fn the_create_process_fallback_hooks_a_child_when_the_internal_hook_is_disabled() {
    let (library, root, target) = fixture("process-fallback");
    let session = deploy_mod_a(&library, &target);
    let out = scratch!("process-fallback-out");
    let child = out.join("child.cmd");
    fs::write(
        &child,
        format!(
            "@echo off\r\necho child > \"{}\"\r\n",
            root.join(r"archive\pc\mod\fallback.txt").display()
        ),
    )
    .unwrap();
    let parent = out.join("parent.cmd");
    fs::write(
        &parent,
        format!(
            "@echo off\r\n\"{}\" /c \"{}\"\r\n",
            r"C:\Windows\System32\cmd.exe",
            child.display()
        ),
    )
    .unwrap();

    let mut running = start(
        &session,
        r"C:\Windows\System32\cmd.exe",
        &["/c".to_string(), parent.display().to_string()],
        &[(
            "GOBBO_OVERLAY_DISABLE".to_string(),
            "create_process".to_string(),
        )],
        None,
    )
    .unwrap();
    assert_eq!(running.wait().unwrap(), 0);
    assert!(
        session
            .overwrite
            .join("archive/pc/mod/fallback.txt")
            .is_file(),
        "the child was not hooked through the CreateProcessW fallback"
    );
    assert!(!root.join("archive/pc/mod/fallback.txt").exists());
    undo(&target.journal_roots()).unwrap();
}

/// A device namespace can alias the real game file; a write through it must be refused, not passed through.
#[test]
fn a_write_through_a_device_namespace_is_refused() {
    const DEVICE_WRITE: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class DeviceWrite {
[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern IntPtr CreateFileW(string name, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template);
public static string Try(string path) {
    IntPtr handle = CreateFileW(path, 0x40000000, 0, IntPtr.Zero, 2, 0x80, IntPtr.Zero);
    int error = Marshal.GetLastWin32Error();
    return (handle == new IntPtr(-1)) + " " + error;
}
}
'@
[DeviceWrite]::Try($args[0]) | Out-File -Encoding ascii $args[1]
"#;
    let (library, root, target) = fixture("device-write");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let out = scratch!("device-write-out");
    let script = out.join("device.ps1");
    fs::write(&script, DEVICE_WRITE).unwrap();

    let device = format!(r"\\.\{}", folder.join("device.txt").display());
    run_script_file(
        &session,
        &script,
        &[device, out.join("result.txt").display().to_string()],
    );

    let result = fs::read_to_string(out.join("result.txt")).unwrap();
    assert_eq!(result.trim(), "True 5", "the device write was not refused");
    assert!(!folder.join("device.txt").exists());
    assert!(!session.overwrite.join("archive/pc/mod/device.txt").exists());
    undo(&target.journal_roots()).unwrap();
}

/// `NtQueryDirectoryFile` through a handle must list the merged layers, not only the layer the handle was opened in.
#[test]
fn an_nt_directory_query_lists_the_merged_layers() {
    let (library, root, target) = fixture("nt-list");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let out = scratch!("nt-list-out");

    let names = nt_list(&session, &folder, &out);
    // A plain NTFS folder lists `.` and `..` as well; the hook gives the same answer.
    assert_eq!(names, [".", "..", "a.archive", "b.archive", "c.archive"]);
    undo(&target.journal_roots()).unwrap();
}

/// A deletion is a whiteout, and the NT listing must hide the lower entry like the Win32 listing does.
#[test]
fn an_nt_directory_query_honours_whiteouts() {
    let (library, root, target) = fixture("nt-list-whiteout");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let delete = scratch!("nt-list-whiteout-delete");
    run_batch(
        &session,
        &delete,
        &[format!("del \"{}\"", folder.join("b.archive").display())],
    );

    let out = scratch!("nt-list-whiteout-out");
    let names = nt_list(&session, &folder, &out);
    assert_eq!(names, [".", "..", "a.archive", "c.archive"]);
    undo(&target.journal_roots()).unwrap();
}

/// Lists a folder one entry at a time (`ReturnSingleEntry`), as a program that asks for single entries does, and prints
/// the names in the order the calls return them. The class is the first argument; the name offset follows from it.
const NT_LIST_SINGLE: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class NtListSingle {
[StructLayout(LayoutKind.Sequential)]
public struct IoStatus { public int Status; public IntPtr Information; }
[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern IntPtr CreateFileW(string name, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template);
[DllImport("ntdll.dll")]
public static extern int NtQueryDirectoryFile(IntPtr handle, IntPtr evt, IntPtr apc, IntPtr context, out IoStatus status, IntPtr info, uint length, uint cls, byte single, IntPtr name, byte restart);
[DllImport("kernel32.dll")]
public static extern bool CloseHandle(IntPtr handle);
public static string[] Of(string folder, uint cls) {
    IntPtr handle = CreateFileW(folder, 0x1, 7, IntPtr.Zero, 3, 0x02000000, IntPtr.Zero);
    if (handle == new IntPtr(-1)) throw new Exception("open " + Marshal.GetLastWin32Error());
    int nameAt = cls == 37 ? 104 : (cls == 3 ? 94 : 68);
    var names = new List<string>();
    IntPtr buffer = Marshal.AllocHGlobal(65536);
    try {
        int status;
        byte restart = 1;
        int calls = 0;
        do {
            IoStatus io;
            status = NtQueryDirectoryFile(handle, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, out io, buffer, 65536, cls, 1, IntPtr.Zero, restart);
            restart = 0;
            calls++;
            if (status == 0) {
                int length = Marshal.ReadInt32(buffer, 60);
                int next = Marshal.ReadInt32(buffer, 0);
                names.Add(Marshal.PtrToStringUni(new IntPtr(buffer.ToInt64() + nameAt), length / 2) + (next == 0 ? "" : " next=" + next));
            } else if (status != unchecked((int)0x80000006)) {
                throw new Exception("status " + status.ToString("x8"));
            }
            if (calls > 100) throw new Exception("too many calls");
        } while (status == 0);
    } finally {
        Marshal.FreeHGlobal(buffer);
        CloseHandle(handle);
    }
    return names.ToArray();
}
}
'@
[NtListSingle]::Of($args[0], [uint32]$args[2]) | Out-File -Encoding ascii $args[1]
"#;

/// The names one-entry queries return for a folder, in call order. Each entry must come back once, with no next offset.
fn nt_list_single(
    session: &Session,
    folder: &std::path::Path,
    out: &std::path::Path,
    class: u32,
) -> Vec<String> {
    let script = out.join("list-single.ps1");
    fs::write(&script, NT_LIST_SINGLE).unwrap();
    run_script_file(
        session,
        &script,
        &[
            folder.display().to_string(),
            out.join("single.txt").display().to_string(),
            class.to_string(),
        ],
    );
    fs::read_to_string(out.join("single.txt"))
        .unwrap()
        .lines()
        .map(|line| line.trim().to_string())
        .collect()
}

/// A one-entry NT query must return each entry once, in order, for each information class the formatter knows, as the
/// full query does.
#[test]
fn an_nt_single_entry_query_lists_each_entry_once() {
    let (library, root, target) = fixture("nt-list-single");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let out = scratch!("nt-list-single-out");

    // The order of a folder is the system's; the names and the absence of a next offset are what must match.
    for class in [2, 3, 37] {
        let mut names = nt_list_single(&session, &folder, &out, class);
        names.sort();
        assert_eq!(
            names,
            [".", "..", "a.archive", "b.archive", "c.archive"],
            "class {class}"
        );
    }
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn a_path_with_dot_dot_writes_to_the_upper_layer_like_the_plain_path() {
    let (library, root, target) = fixture("dotdot");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join("archive/pc/mod");
    let out = scratch!("dotdot-out");

    run_batch(
        &session,
        &out,
        &[format!(
            "echo x > \"{}\\..\\mod\\dotted.txt\"",
            folder.display()
        )],
    );

    assert!(!folder.join("dotted.txt").exists());
    assert!(
        session
            .overwrite
            .join("archive/pc/mod/dotted.txt")
            .is_file()
    );
    undo(&target.journal_roots()).unwrap();
}

/// Asks for the final path of three files, as `canonical` does: a file of a layer, the same file in the NT form, and
/// a file of the game folder. The arguments are the folder and the file for the answers.
const FINAL_PATH: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class FinalPath {
[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern IntPtr CreateFileW(string name, uint access, uint share, IntPtr security, uint disposition, uint flags, IntPtr template);
[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern uint GetFinalPathNameByHandleW(IntPtr handle, StringBuilder path, uint size, uint flags);
[DllImport("kernel32.dll")]
public static extern bool CloseHandle(IntPtr handle);
[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
public static extern IntPtr CreateFile2(string name, uint access, uint share, uint disposition, IntPtr parameters);
static string Query(IntPtr handle, uint flags) {
    var text = new StringBuilder(1024);
    uint length = GetFinalPathNameByHandleW(handle, text, 1024, flags);
    return text.ToString(0, (int)length);
}
public static string Of(string name, uint flags) {
    IntPtr handle = CreateFileW(name, 0x80, 7, IntPtr.Zero, 3, 0, IntPtr.Zero);
    if (handle == new IntPtr(-1)) throw new Exception("open failed " + Marshal.GetLastWin32Error());
    try { return Query(handle, flags); } finally { CloseHandle(handle); }
}
public static string Of2(string name, uint flags) {
    IntPtr handle = CreateFile2(name, 0x80, 7, 3, IntPtr.Zero);
    if (handle == new IntPtr(-1)) throw new Exception("open failed " + Marshal.GetLastWin32Error());
    try { return Query(handle, flags); } finally { CloseHandle(handle); }
}
}
'@
$folder = $args[0]
$lines = @(
[FinalPath]::Of("$folder\c.archive", 0),
[FinalPath]::Of("$folder\c.archive", 2),
[FinalPath]::Of("$folder\a.archive", 0),
[FinalPath]::Of2("$folder\c.archive", 0)
)
Set-Content -Path $args[1] -Value $lines -Encoding ascii
"#;

#[test]
fn a_file_taken_from_a_layer_reports_its_game_path_as_its_final_path() {
    // `canonical` and `relative` open a path and ask for its final path. Cyber Engine Tweaks loads its mods that way,
    // and a layer's path there reads as outside the game folder, so the mod is refused.
    let (library, root, target) = fixture("final-path");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let out = scratch!("final-path-out");
    let script = out.join("final.ps1");
    fs::write(&script, FINAL_PATH).unwrap();

    run_script_file(
        &session,
        &script,
        &[
            folder.display().to_string(),
            out.join("final.txt").display().to_string(),
        ],
    );

    let text = fs::read_to_string(out.join("final.txt")).unwrap();
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let game_folder = fs::canonicalize(&folder).unwrap().display().to_string();
    // The mod's file lies in a layer, and its final path is still the game folder's, in the DOS form.
    assert_eq!(lines[0], format!(r"{game_folder}\c.archive"));
    // The NT form starts with the device and ends with the file's place in the game folder.
    assert!(
        lines[1].starts_with(r"\Device\") && lines[1].ends_with(r"\archive\pc\mod\c.archive"),
        "{}",
        lines[1]
    );
    // A file the game folder holds is not redirected, so its path is the plain one.
    assert_eq!(lines[2], format!(r"{game_folder}\a.archive"));
    // `CreateFile2` is the call the standard library uses on current Windows, and it gives the same answer.
    assert_eq!(lines[3], lines[0]);
    undo(&target.journal_roots()).unwrap();
}

#[test]
fn folders_the_game_makes_stay_out_of_the_game_folder() {
    // The game creates folders of its own (Cyber Engine Tweaks makes its folders at start). They must appear in the
    // layered view, and the game folder must stay as it was.
    let (library, root, target) = fixture("mkdir");
    let session = deploy_mod_a(&library, &target);
    let folder = root.join(r"archive\pc\mod");
    let out = scratch!("mkdir-out");
    let script = format!(
        "New-Item -ItemType Directory -Path '{ps}' | Out-Null; \
         [IO.Directory]::CreateDirectory('{net}') | Out-Null; \
         Test-Path '{ps}','{net}','{cmd}' | Out-File -Encoding ascii '{out}\\exists.txt'",
        ps = folder.join("ps-dir").display(),
        net = folder.join("net-dir").display(),
        cmd = folder.join("cmd-dir").display(),
        out = out.display(),
    );
    run_batch(
        &session,
        &out,
        &[format!("mkdir \"{}\"", folder.join("cmd-dir").display())],
    );
    run(
        &session,
        POWERSHELL,
        &["-NoProfile".to_string(), "-Command".to_string(), script],
    )
    .unwrap();

    // The layered view sees the folders.
    let seen = fs::read_to_string(out.join("exists.txt")).unwrap();
    assert!(seen.lines().all(|line| line.trim() == "True"), "{seen}");
    // The game folder does not get them.
    for name in ["ps-dir", "net-dir", "cmd-dir"] {
        assert!(!folder.join(name).exists(), "{name} is in the game folder");
        assert!(
            session.overwrite.join("archive/pc/mod").join(name).is_dir(),
            "{name} is not in the upper layer"
        );
    }
    undo(&target.journal_roots()).unwrap();
}
