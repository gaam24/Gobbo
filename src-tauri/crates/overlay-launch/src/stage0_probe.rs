//! Stage 0 regressions through the production installer: M2 serves a suspended child's loader calls from the layer,
//! M3 binds a system DLL name from the layer, and X5 does the same for a 32-bit child.

use crate::stage0::{Bits, install, redirect_config};
#[cfg(target_arch = "x86_64")]
use crate::stage0::{STUBS, ntdll_export};

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The game's DLL: it imports the system `version.dll` and returns what that function returns for a missing file.
/// Without a redirect the system copy answers 0.
const GAME_DLL: &str = r#"
#[link(name = "version")]
unsafe extern "system" {
    fn GetFileVersionInfoSizeW(name: *const u16, handle: *mut u32) -> u32;
}
#[unsafe(no_mangle)]
pub extern "C" fn gobbo_game_version_size() -> u32 {
    let name: Vec<u16> = "C:/gobbo-no-such-file.txt".encode_utf16().chain(core::iter::once(0)).collect();
    unsafe { GetFileVersionInfoSizeW(name.as_ptr(), core::ptr::null_mut()) }
}
"#;

/// The program: exits with 0 when the game DLL reports the mod's answer.
const PROGRAM: &str = r#"
#[link(name = "game", kind = "dylib")]
unsafe extern "C" {
    fn gobbo_game_version_size() -> u32;
}
fn main() {
    std::process::exit(if unsafe { gobbo_game_version_size() } == 42 { 0 } else { 4 });
}
"#;

/// A program that writes one file named by its argument, through the ordinary Win32 file API.
const WRITER: &str = r#"
fn main() {
    let path = std::env::args().nth(1).expect("a path");
    std::fs::write(path, "early\r\n").expect("the write");
}
"#;

/// Compiles one of the fixture sources. `def` is the `.def` file that maps the stdcall export of the 32-bit layer DLL
/// to the undecorated system name.
fn rustc(source: &Path, output: &Path, target: &str, dll: bool, native: &Path, def: Option<&Path>) {
    let mut command = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
    command.args(["--edition=2021", "-O", "--target", target]);

    if dll {
        command.args(["--crate-type", "cdylib"]);
    }

    command
        .arg("-L")
        .arg(format!("native={}", native.display()));

    if let Some(def) = def {
        command.args(["-C", &format!("link-arg=/DEF:{}", def.display())]);
    }

    let status = command
        .arg("-o")
        .arg(output)
        .arg(source)
        .status()
        .expect("rustc runs");
    assert!(status.success(), "{} did not build", output.display());
}

/// Creates an empty upper folder beside `folder` and returns it. The read tests do not write, but the configuration
/// always carries the three layers.
fn upper_beside(folder: &Path) -> PathBuf {
    let upper = folder.parent().unwrap().join("upper");
    fs::create_dir_all(&upper).unwrap();
    upper
}

/// Builds the game folder with the game DLL and the program, and the mod layer with a `version.dll` made from
/// `layer_source`. `def_export`, when set, is the `.def` mapping line that gives the layer DLL the undecorated system
/// name; the game DLL and the program are built the same way on both bitnesses.
fn binding_fixture(
    name: &str,
    target: &str,
    layer_source: &str,
    def_export: Option<&str>,
) -> (PathBuf, PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("gobbo-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let game = base.join("game");
    let layer = base.join("mods");
    let work = base.join("work");

    for folder in [&game, &layer, &work] {
        fs::create_dir_all(folder).unwrap();
    }

    let version_source = work.join("version.rs");
    fs::write(&version_source, layer_source).unwrap();
    let def = def_export.map(|export| {
        let def = work.join("version.def");
        fs::write(&def, format!("LIBRARY version\nEXPORTS\n    {export}\n")).unwrap();
        def
    });
    rustc(
        &version_source,
        &layer.join("version.dll"),
        target,
        true,
        &work,
        def.as_deref(),
    );

    let game_source = work.join("game.rs");
    fs::write(&game_source, GAME_DLL).unwrap();
    let game_dll = game.join("game.dll");
    rustc(&game_source, &game_dll, target, true, &work, None);
    // The program links the game DLL through its import library, which the DLL build wrote next to it.
    fs::copy(game_dll.with_extension("dll.lib"), work.join("game.lib")).unwrap();

    let program_source = work.join("program.rs");
    fs::write(&program_source, PROGRAM).unwrap();
    let program = game.join("program.exe");
    rustc(&program_source, &program, target, false, &work, None);
    (game, layer, program)
}

/// M2: the redirect serves the loader's file calls of a suspended child from the layer.
#[cfg(test)]
pub(super) mod m2_redirect {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::PathBuf;
    use std::ptr::null;

    #[cfg(target_arch = "x86_64")]
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        CREATE_SUSPENDED, CreateProcessW, GetExitCodeProcess, PROCESS_INFORMATION, ResumeThread,
        STARTUPINFOW, TerminateProcess, WaitForSingleObject,
    };

    use super::{Bits, install, redirect_config, upper_beside};
    #[cfg(target_arch = "x86_64")]
    use super::{STUBS, ntdll_export};
    use super::{WRITER, rustc};
    use crate::wide;

    /// A suspended child, terminated when dropped.
    pub(super) struct Child(pub(super) PROCESS_INFORMATION);

    impl Drop for Child {
        fn drop(&mut self) {
            unsafe {
                TerminateProcess(self.0.hProcess, 0);
                CloseHandle(self.0.hThread);
                CloseHandle(self.0.hProcess);
            }
        }
    }

    pub(super) fn spawn_suspended(command: &str) -> Child {
        let mut line = wide(OsStr::new(command));
        let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
        startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        let created = unsafe {
            CreateProcessW(
                null(),
                line.as_mut_ptr(),
                null(),
                null(),
                FALSE,
                CREATE_SUSPENDED,
                null(),
                null(),
                &startup,
                &mut info,
            )
        };
        assert_ne!(created, FALSE, "the child did not start");
        Child(info)
    }

    /// A scratch folder pair: the game folder with `other.txt`, and the mod layer with `hello.txt`.
    fn folders(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("gobbo-m2-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let game = base.join("game");
        let layer = base.join("mods");
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&layer).unwrap();
        fs::write(game.join("other.txt"), "from-game\r\n").unwrap();
        fs::write(layer.join("hello.txt"), "from-layer\r\n").unwrap();
        (game, layer)
    }

    /// Runs `cmd` suspended with stage 0 for `bits`, and checks that it read the layer copy of a file the game folder
    /// lacks, read a game file, and wrote nothing into the game folder.
    fn reads_the_layer_copy(name: &str, cmd: &str, bits: Bits) {
        let (game, layer) = folders(name);
        let out = game.parent().unwrap().join("out.txt");
        let command = format!(
            r#"{cmd} /c type "{game}\hello.txt" > "{out}" & type "{game}\other.txt" >> "{out}""#,
            game = game.display(),
            out = out.display(),
        );
        let child = spawn_suspended(&command);
        let process = child.0.hProcess;
        let config = redirect_config(&game, &layer, &upper_beside(&game)).unwrap();
        let _patched = install(process, config, bits).expect("the stage 0 DLL is patched in");
        assert_ne!(unsafe { ResumeThread(child.0.hThread) }, u32::MAX);
        assert_eq!(
            unsafe { WaitForSingleObject(process, 30_000) },
            WAIT_OBJECT_0
        );
        let mut code = 0;

        unsafe { GetExitCodeProcess(process, &mut code) };
        assert_eq!(code, 0, "the redirected child failed");
        let text = fs::read_to_string(&out).expect("the child wrote its output");
        assert!(
            text.contains("from-layer"),
            "the layer copy was not read: {text:?}"
        );
        assert!(
            text.contains("from-game"),
            "the game file was not read: {text:?}"
        );
        assert!(
            !game.join("hello.txt").exists(),
            "the game folder was written"
        );
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn m2_a_file_missing_from_the_game_folder_is_read_from_the_layer() {
        reads_the_layer_copy("read", r"C:\Windows\System32\cmd.exe", Bits::X64);
    }

    /// X4: a 32-bit (WOW64) child reads the layer copy of a file the game folder does not have, through the x86 stage 0.
    #[test]
    fn m2_x86_a_32_bit_child_reads_the_layer_copy() {
        reads_the_layer_copy("x86-read", r"C:\Windows\SysWOW64\cmd.exe", Bits::X86);
    }

    /// A loader DLL runs before the hook when the loader resolves the import tree. A write it makes under its own layer
    /// path must land in `upper`, not in the mod file; a read of the same path still sees the layer.
    fn writes_to_upper(name: &str, cmd: &str, bits: Bits) {
        let (game, layer) = folders(name);
        let upper = upper_beside(&game);
        let log = layer.join("early.log");
        fs::write(&log, "old\r\n").unwrap();
        let command = format!(r#"{cmd} /c echo early> "{}""#, log.display());
        let child = spawn_suspended(&command);
        let process = child.0.hProcess;
        let config = redirect_config(&game, &layer, &upper).unwrap();
        let _patched = install(process, config, bits).expect("the stage 0 DLL is patched in");
        assert_ne!(unsafe { ResumeThread(child.0.hThread) }, u32::MAX);
        assert_eq!(
            unsafe { WaitForSingleObject(process, 30_000) },
            WAIT_OBJECT_0
        );
        let mut code = 0;

        unsafe { GetExitCodeProcess(process, &mut code) };
        assert_eq!(code, 0, "the redirected child failed");
        assert_eq!(
            fs::read_to_string(upper.join("early.log")).unwrap(),
            "early\r\n"
        );
        assert_eq!(
            fs::read_to_string(&log).unwrap(),
            "old\r\n",
            "the write changed the layer file"
        );
        assert!(
            !game.join("early.log").exists(),
            "the game folder was written"
        );
    }

    /// A writer program, so the test also covers a Win32 call whose path is not built by `cmd`.
    fn writer_program(game: &std::path::Path) -> std::path::PathBuf {
        let work = game.parent().unwrap().join("work");
        fs::create_dir_all(&work).unwrap();
        let source = work.join("writer.rs");
        fs::write(&source, WRITER).unwrap();
        let output = game.join("writer.exe");
        rustc(
            &source,
            &output,
            "x86_64-pc-windows-msvc",
            false,
            &work,
            None,
        );
        output
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn m2_a_write_under_the_layer_goes_to_the_upper_layer() {
        writes_to_upper("write", r"C:\Windows\System32\cmd.exe", Bits::X64);
    }

    #[test]
    fn m2_x86_a_write_under_the_layer_goes_to_the_upper_layer() {
        writes_to_upper("x86-write", r"C:\Windows\SysWOW64\cmd.exe", Bits::X86);
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn m2_a_win32_write_under_the_layer_goes_to_the_upper_layer() {
        let (game, layer) = folders("writer");
        let upper = upper_beside(&game);
        let log = layer.join("early.log");
        fs::write(&log, "old\r\n").unwrap();
        let writer = writer_program(&game);
        let command = format!("\"{}\" \"{}\"", writer.display(), log.display());
        let child = spawn_suspended(&command);
        let process = child.0.hProcess;
        let config = redirect_config(&game, &layer, &upper).unwrap();
        let _patched = install(process, config, Bits::X64).expect("the stage 0 DLL is patched in");
        assert_ne!(unsafe { ResumeThread(child.0.hThread) }, u32::MAX);
        assert_eq!(
            unsafe { WaitForSingleObject(process, 30_000) },
            WAIT_OBJECT_0
        );
        let mut code = 0;

        unsafe { GetExitCodeProcess(process, &mut code) };
        assert_eq!(code, 0, "the writer failed");
        assert_eq!(
            fs::read_to_string(upper.join("early.log")).unwrap(),
            "early\r\n"
        );
        assert_eq!(fs::read_to_string(&log).unwrap(), "old\r\n");
        assert!(!game.join("early.log").exists());
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn m2_restore_puts_the_stubs_back_and_turns_the_redirect_off() {
        let (game, layer) = folders("restore");
        let child = spawn_suspended(r"C:\Windows\System32\cmd.exe /c exit 0");
        let process = child.0.hProcess;
        let originals: Vec<(usize, Vec<u8>)> = STUBS
            .iter()
            .map(|name| {
                let address = ntdll_export(name).unwrap();
                (address, read_bytes(process, address, 22))
            })
            .collect();
        let patched = install(
            process,
            redirect_config(&game, &layer, &upper_beside(&game)).unwrap(),
            Bits::X64,
        )
        .expect("patched");
        for (address, original) in &originals {
            assert_ne!(
                &read_bytes(process, *address, 22),
                original,
                "a stub was not patched"
            );
        }

        patched.restore().expect("restored");

        for (address, original) in &originals {
            assert_eq!(
                &read_bytes(process, *address, 22),
                original,
                "a stub was not restored"
            );
        }
    }

    pub(super) fn resume_and_wait(child: &Child) -> i32 {
        let process = child.0.hProcess;
        assert_ne!(unsafe { ResumeThread(child.0.hThread) }, u32::MAX);
        assert_eq!(
            unsafe { WaitForSingleObject(process, 60_000) },
            WAIT_OBJECT_0,
            "the child did not finish"
        );
        let mut code = 0;

        unsafe { GetExitCodeProcess(process, &mut code) };
        code as i32
    }

    #[cfg(target_arch = "x86_64")]
    fn read_bytes(process: HANDLE, address: usize, length: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; length];
        let mut done = 0;
        let ok = unsafe {
            windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory(
                process,
                address as *const _,
                bytes.as_mut_ptr().cast(),
                length,
                &mut done,
            )
        };
        assert_ne!(ok, FALSE);
        bytes
    }
}

/// M3: a game DLL imports a system name (`version.dll`) that the layer also has. The layer copy must be the one bound.
/// It builds and starts an x64 program, so it only runs in an x64 test binary; X5 covers the x86 case.
#[cfg(all(test, target_arch = "x86_64"))]
mod m3_system_name {
    use super::{Bits, binding_fixture, install, redirect_config, upper_beside};

    use super::m2_redirect::{resume_and_wait, spawn_suspended};

    /// The mod's `version.dll`: it exports the one function the game DLL imports, and returns 42.
    const LAYER_VERSION: &str = r#"
#[unsafe(no_mangle)]
pub unsafe extern "system" fn GetFileVersionInfoSizeW(_name: *const u16, _handle: *mut u32) -> u32 {
    42
}
"#;

    #[test]
    fn m3_the_layer_copy_of_a_system_name_is_the_one_bound() {
        let (game, layer, program) =
            binding_fixture("m3-bound", "x86_64-pc-windows-msvc", LAYER_VERSION, None);
        let command = format!("\"{}\"", program.display());
        let child = spawn_suspended(&command);
        let process = child.0.hProcess;
        let _patched = install(
            process,
            redirect_config(&game, &layer, &upper_beside(&game)).unwrap(),
            Bits::X64,
        )
        .expect("patched");
        let code = resume_and_wait(&child);
        assert_eq!(
            code, 0,
            "the game DLL bound the system version.dll, not the layer copy"
        );
    }

    #[test]
    fn m3_control_without_the_redirect_binds_the_system_copy() {
        let (_game, _layer, program) =
            binding_fixture("m3-control", "x86_64-pc-windows-msvc", LAYER_VERSION, None);
        let command = format!("\"{}\"", program.display());
        let child = spawn_suspended(&command);
        let code = resume_and_wait(&child);
        assert_eq!(
            code, 4,
            "the control must bind the system copy, or the test proves nothing"
        );
    }
}

/// X5: the same binding test for 32-bit: the game DLL and the program are x86, the layer copy exports the stdcall name the
/// system `version.dll` exports.
#[cfg(test)]
mod x5_system_name32 {
    use super::{Bits, binding_fixture, install, redirect_config, upper_beside};

    use super::m2_redirect::{resume_and_wait, spawn_suspended};

    /// The mod's `version.dll`: the stdcall export the game DLL imports, returning 42.
    const LAYER_VERSION: &str = r#"
#[unsafe(no_mangle)]
pub unsafe extern "system" fn layer_size(_name: *const u16, _handle: *mut u32) -> u32 {
    42
}
"#;

    #[test]
    fn x5_the_layer_copy_of_a_system_name_is_bound_in_a_32_bit_child() {
        let (game, layer, program) = binding_fixture(
            "x5-bound",
            "i686-pc-windows-msvc",
            LAYER_VERSION,
            Some("GetFileVersionInfoSizeW=_layer_size@8"),
        );
        let child = spawn_suspended(&format!("\"{}\"", program.display()));
        let _patched = install(
            child.0.hProcess,
            redirect_config(&game, &layer, &upper_beside(&game)).unwrap(),
            Bits::X86,
        )
        .expect("patched");
        assert_eq!(
            resume_and_wait(&child),
            0,
            "the 32-bit game DLL bound the system version.dll"
        );
    }

    #[test]
    fn x5_control_without_the_redirect_binds_the_system_copy() {
        let (_game, _layer, program) = binding_fixture(
            "x5-control",
            "i686-pc-windows-msvc",
            LAYER_VERSION,
            Some("GetFileVersionInfoSizeW=_layer_size@8"),
        );
        let child = spawn_suspended(&format!("\"{}\"", program.display()));
        assert_eq!(
            resume_and_wait(&child),
            4,
            "the control must bind the system copy"
        );
    }
}
