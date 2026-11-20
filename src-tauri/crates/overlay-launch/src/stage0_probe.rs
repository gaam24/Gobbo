//! Stage 0 regressions through the production installer: M2 serves a suspended child's loader calls from the layer,
//! M3 binds a system DLL name from the layer, and X5 does the same for a 32-bit child.

use crate::stage0::{Bits, STUBS, install, ntdll_export, redirect_config};

/// M2: the redirect serves the loader's file calls of a suspended child from the layer.
#[cfg(test)]
pub(super) mod m2_redirect {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::PathBuf;
    use std::ptr::null;

    use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        CREATE_SUSPENDED, CreateProcessW, GetExitCodeProcess, PROCESS_INFORMATION, ResumeThread,
        STARTUPINFOW, TerminateProcess, WaitForSingleObject,
    };

    use super::{Bits, STUBS, install, ntdll_export, redirect_config};
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
        let config = redirect_config(&game, &layer).unwrap();
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
    fn m2_a_file_missing_from_the_game_folder_is_read_from_the_layer() {
        reads_the_layer_copy("read", r"C:\Windows\System32\cmd.exe", Bits::X64);
    }

    /// X4: a 32-bit (WOW64) child reads the layer copy of a file the game folder does not have, through the x86 stage 0.
    #[test]
    fn m2_x86_a_32_bit_child_reads_the_layer_copy() {
        reads_the_layer_copy("x86-read", r"C:\Windows\SysWOW64\cmd.exe", Bits::X86);
    }

    #[test]
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
        let patched =
            install(process, redirect_config(&game, &layer).unwrap(), Bits::X64).expect("patched");
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
#[cfg(test)]
mod m3_system_name {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::{Bits, install, redirect_config};

    use super::m2_redirect::{resume_and_wait, spawn_suspended};

    /// The mod's `version.dll`: it exports the one function the game DLL imports, and returns 42.
    const LAYER_VERSION: &str = r#"
#[unsafe(no_mangle)]
pub unsafe extern "system" fn GetFileVersionInfoSizeW(_name: *const u16, _handle: *mut u32) -> u32 {
    42
}
"#;

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

    fn rustc(source: &Path, output: &Path, dll: bool, native: &Path) {
        let mut command = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        command.args(["--edition=2021", "-O", "--target", "x86_64-pc-windows-msvc"]);

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
            .expect("rustc runs");
        assert!(status.success(), "{} did not build", output.display());
    }

    /// Builds the game folder (with the game DLL and the program) and the layer (with the mod's `version.dll`).
    fn fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("gobbo-m3-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let game = base.join("game");
        let layer = base.join("mods");
        let work = base.join("work");

        for folder in [&game, &layer, &work] {
            fs::create_dir_all(folder).unwrap();
        }

        let version_source = work.join("version.rs");
        fs::write(&version_source, LAYER_VERSION).unwrap();
        rustc(&version_source, &layer.join("version.dll"), true, &work);

        let game_source = work.join("game.rs");
        fs::write(&game_source, GAME_DLL).unwrap();
        let game_dll = game.join("game.dll");
        rustc(&game_source, &game_dll, true, &work);
        // The program links the game DLL through its import library, which the DLL build wrote next to it.
        fs::copy(game_dll.with_extension("dll.lib"), work.join("game.lib")).unwrap();

        let program_source = work.join("program.rs");
        fs::write(&program_source, PROGRAM).unwrap();
        let program = game.join("program.exe");
        rustc(&program_source, &program, false, &work);
        (game, layer, program)
    }

    #[test]
    fn m3_the_layer_copy_of_a_system_name_is_the_one_bound() {
        let (game, layer, program) = fixture("bound");
        let command = format!("\"{}\"", program.display());
        let child = spawn_suspended(&command);
        let process = child.0.hProcess;
        let _patched =
            install(process, redirect_config(&game, &layer).unwrap(), Bits::X64).expect("patched");
        let code = resume_and_wait(&child);
        assert_eq!(
            code, 0,
            "the game DLL bound the system version.dll, not the layer copy"
        );
    }

    #[test]
    fn m3_control_without_the_redirect_binds_the_system_copy() {
        let (_game, _layer, program) = fixture("control");
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
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::{Bits, install, redirect_config};

    use super::m2_redirect::{resume_and_wait, spawn_suspended};

    /// The mod's `version.dll`: the stdcall export the game DLL imports, returning 42.
    const LAYER_VERSION: &str = r#"
#[unsafe(no_mangle)]
pub unsafe extern "system" fn layer_size(_name: *const u16, _handle: *mut u32) -> u32 {
    42
}
"#;

    /// The game's DLL, linked to the system `version.dll` import library of the 32-bit SDK.
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

    const PROGRAM: &str = r#"
#[link(name = "game", kind = "dylib")]
unsafe extern "C" {
    fn gobbo_game_version_size() -> u32;
}
fn main() {
    std::process::exit(if unsafe { gobbo_game_version_size() } == 42 { 0 } else { 4 });
}
"#;

    fn rustc(source: &Path, output: &Path, dll: bool, native: &Path) {
        let mut command = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        command.args(["--edition=2021", "-O", "--target", "i686-pc-windows-msvc"]);

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
            .expect("rustc runs");
        assert!(status.success(), "{} did not build", output.display());
    }

    fn fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("gobbo-x5-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let game = base.join("game");
        let layer = base.join("mods");
        let work = base.join("work");

        for folder in [&game, &layer, &work] {
            fs::create_dir_all(folder).unwrap();
        }

        let version_source = work.join("version.rs");
        fs::write(&version_source, LAYER_VERSION).unwrap();
        // The system name is exported undecorated: a `.def` file maps it to the stdcall function.
        let def = work.join("version.def");
        fs::write(
            &def,
            "LIBRARY version\nEXPORTS\n    GetFileVersionInfoSizeW=_layer_size@8\n",
        )
        .unwrap();
        let output = layer.join("version.dll");
        let mut command = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        command.args([
            "--edition=2021",
            "-O",
            "--target",
            "i686-pc-windows-msvc",
            "--crate-type",
            "cdylib",
            "-C",
        ]);
        command.arg(format!("link-arg=/DEF:{}", def.display()));
        let status = command
            .arg("-o")
            .arg(&output)
            .arg(&version_source)
            .status()
            .expect("rustc runs");
        assert!(status.success(), "the layer version.dll did not build");

        let game_source = work.join("game.rs");
        fs::write(&game_source, GAME_DLL).unwrap();
        let game_dll = game.join("game.dll");
        rustc(&game_source, &game_dll, true, &work);
        fs::copy(game_dll.with_extension("dll.lib"), work.join("game.lib")).unwrap();

        let program_source = work.join("program.rs");
        fs::write(&program_source, PROGRAM).unwrap();
        let program = game.join("program.exe");
        rustc(&program_source, &program, false, &work);
        (game, layer, program)
    }

    #[test]
    fn x5_the_layer_copy_of_a_system_name_is_bound_in_a_32_bit_child() {
        let (game, layer, program) = fixture("bound");
        let child = spawn_suspended(&format!("\"{}\"", program.display()));
        let _patched = install(
            child.0.hProcess,
            redirect_config(&game, &layer).unwrap(),
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
        let (_game, _layer, program) = fixture("control");
        let child = spawn_suspended(&format!("\"{}\"", program.display()));
        assert_eq!(
            resume_and_wait(&child),
            4,
            "the control must bind the system copy"
        );
    }
}
