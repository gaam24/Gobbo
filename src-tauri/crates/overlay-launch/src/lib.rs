//! Starts a program with the overlay hook loaded before its first instruction. The program starts suspended, its main
//! thread is pointed at a stub that loads the hook and calls its `GobboOverlayInit` export, and only then is the thread
//! resumed. The hook is in place before the Windows loader resolves the program's static imports. The launcher also
//! rewrites the static import descriptors whose DLL is in the overlay layer, because the loader resolves those before the
//! hook's detours exist; a DLL next to the program can then come from the layer without a file in the game folder.
//!
//! Like `overlay-hook`, this crate calls Win32 directly and uses `unsafe`. On other platforms the crate is empty.

#![cfg(windows)]

mod inject;
pub mod stage0;
#[cfg(test)]
mod stage0_probe;
pub mod static_imports;

pub use inject::{inject_early, inject_early_by_tid};
pub use static_imports::imports;

use std::ffi::{OsStr, OsString, c_void};
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::RawHandle;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::time::Instant;

use gobbo_overlay_core::wide;
use windows_sys::Win32::Foundation::{
    CloseHandle, FALSE, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation, TRUE, WAIT_FAILED,
    WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateEventW, CreateProcessW,
    DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, INFINITE,
    InitializeProcThreadAttributeList, IsWow64Process, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, ResetEvent, ResumeThread,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForMultipleObjects, WaitForSingleObject,
};

/// How long the hook may take to load. The game stays suspended until it has, so a load that never returns would
/// leave a process with no window and no error. The hook's child wait uses the same budget through `ReadyEvent`.
pub const HOOK_LOAD_TIMEOUT_MS: u32 = 20_000;

/// What to start, and the hook DLL to load into it.
pub struct Launch<'a> {
    /// The program to start, with its full path.
    pub program: &'a Path,
    /// The arguments, passed as they are; each one is quoted for the command line.
    pub args: &'a [String],
    /// The hook DLL for the program's architecture, as a full path.
    pub hook: &'a Path,
    /// The hook DLL for a 32-bit program. `None` means a 32-bit program cannot be hooked; the launch fails instead of
    /// running it unhooked.
    pub hook32: Option<&'a Path>,
    /// Static imports whose DLL is in the mod layer, by the import name. The launcher points the import descriptor at
    /// the layer path before the loader resolves it.
    pub imports: &'a [(String, PathBuf)],
    /// Variables added to the environment the program inherits from Gobbo.
    pub env: &'a [(String, String)],
    /// The folder the program starts in. `None` means the folder of the program.
    pub cwd: Option<&'a Path>,
    /// The write end of a pipe that takes the program's standard output and error. `None` keeps the streams of Gobbo.
    pub output: Option<RawHandle>,
    /// The early redirect of the program's file calls to the mod layer (`stage0`). It is set up before the loader runs, with
    /// the x64 or the x86 DLL for the program's bitness; the hook reads the handover file and puts the stubs back. `None`
    /// leaves the loader's calls alone.
    pub early: Option<stage0::Early<'a>>,
}

/// Where the launcher spent its time before the program's first instruction, in microseconds. The steps that depend on
/// the architecture or fail leave zero values when they did not run.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    /// `CreateProcessW` with the program suspended.
    pub create_process_us: u64,
    /// `IsWow64Process`.
    pub detect_us: u64,
    /// The early redirect (`stage0`), including its handover file.
    pub stage0_us: u64,
    /// The event the hook signals when its hooks are installed.
    pub ready_event_us: u64,
    /// The stub that loads the hook, and the wait for the hook to start.
    pub inject_us: u64,
    /// Rewriting the static import descriptors of the suspended program.
    pub imports_us: u64,
    /// `ResumeThread`, the point where the program's first instruction may run.
    pub resume_us: u64,
    /// The wait for the hook's ready event after the resume.
    pub ready_wait_us: u64,
}

/// Microseconds since `started`, saturating instead of wrapping on a very long step.
fn micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// A started program with the hook loaded. Dropping it closes the handle; the program keeps running.
pub struct Hooked {
    process: HANDLE,
    timings: Timings,
}

// SAFETY: the handle is only used through `wait`, which only waits on it and reads the exit code.
unsafe impl Send for Hooked {}

impl Hooked {
    /// Where the start spent its time; the program has already run since `spawn_hooked` returned.
    pub const fn timings(&self) -> Timings {
        self.timings
    }

    /// Waits until the program exits, and returns its exit code.
    pub fn wait(&self) -> io::Result<u32> {
        // SAFETY: `process` is a process handle this struct owns until it is dropped.
        unsafe {
            if WaitForSingleObject(self.process, INFINITE) == u32::MAX {
                return Err(io::Error::last_os_error());
            }

            let mut code = 0;

            if GetExitCodeProcess(self.process, &mut code) == FALSE {
                return Err(io::Error::last_os_error());
            }

            Ok(code)
        }
    }
}

impl Drop for Hooked {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by `spawn_hooked` and is closed only here.
        unsafe {
            CloseHandle(self.process);
        }
    }
}

/// Starts the program suspended, loads the hook into it, and resumes it.
pub fn spawn_hooked(launch: &Launch) -> io::Result<Hooked> {
    let cwd = launch
        .cwd
        .or_else(|| launch.program.parent())
        .map(|dir| wide(dir.as_os_str()));
    let mut command = command_line(launch.program, launch.args);
    let environment = environment_block(launch.env);

    // SAFETY: every pointer passed to Win32 is valid for the call: the buffers live until it returns, and the
    // structures are zeroed and sized as the API expects.
    let output: Option<HANDLE> = launch.output;
    // With an output the program gets the null device as its input, so it never waits on a console of Gobbo.
    let input = if output.is_some() {
        Some(File::open("NUL")?)
    } else {
        None
    };
    let input: Option<HANDLE> = input.as_ref().map(|file| file.as_raw_handle());
    // The handles the program inherits: its standard streams, and nothing else. Without a list every inheritable handle
    // of Gobbo would reach the game.
    let inherited: Vec<HANDLE> = output.into_iter().chain(input).collect();
    let _inheritable = Inheritable::new(&inherited)?;

    unsafe {
        let mut startup: STARTUPINFOW = std::mem::zeroed();
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        let mut list = None;

        if output.is_some() {
            startup.dwFlags = STARTF_USESTDHANDLES;
            startup.hStdOutput = output.unwrap_or_default();
            startup.hStdError = output.unwrap_or_default();
            startup.hStdInput = input.unwrap_or_default();
            list = Some(HandleList::new(&inherited)?);
        }

        let mut extended: STARTUPINFOEXW = std::mem::zeroed();
        extended.StartupInfo = startup;
        extended.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        let (flags, startup_info) = match list.as_mut() {
            Some(list) => {
                extended.lpAttributeList = list.as_ptr();
                (
                    CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                    (&raw const extended).cast::<STARTUPINFOW>(),
                )
            }
            None => (
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT,
                &raw const startup,
            ),
        };
        let mut info: PROCESS_INFORMATION = std::mem::zeroed();
        let mut timings = Timings::default();
        eprintln!("gobbo: starting {} suspended", launch.program.display());
        let started = Instant::now();
        let created = CreateProcessW(
            null(),
            command.as_mut_ptr(),
            null(),
            null(),
            if output.is_some() { TRUE } else { FALSE },
            flags,
            environment.as_ptr().cast::<c_void>(),
            cwd.as_ref().map_or(null(), |dir| dir.as_ptr()),
            startup_info,
            &mut info,
        );
        timings.create_process_us = micros(started);
        // Read before anything else runs, so the error is the one the start left.
        let start_error = io::Error::last_os_error();

        if created == FALSE {
            return Err(start_error);
        }

        let started = Instant::now();
        let wow64 = match is_wow64(info.hProcess) {
            Ok(wow64) => wow64,
            Err(error) => {
                eprintln!(
                    "gobbo: cannot ask whether pid {} is 32-bit: {error}",
                    info.dwProcessId
                );
                return Err(stop_suspended(&info, error));
            }
        };
        timings.detect_us = micros(started);
        let hook = if wow64 {
            match launch.hook32 {
                Some(hook) => hook,
                None => {
                    let error = io::Error::new(
                        io::ErrorKind::NotFound,
                        "the game is 32-bit and the 32-bit overlay hook is missing; build gobbo_overlay_hook32.dll",
                    );
                    eprintln!("gobbo: cannot hook pid {}: {error}", info.dwProcessId);
                    return Err(stop_suspended(&info, error));
                }
            }
        } else {
            launch.hook
        };
        eprintln!(
            "gobbo: started pid {} suspended, loading the hook {}",
            info.dwProcessId,
            hook.display()
        );
        // The early redirect is patched before the loader runs, so the loader's first file calls already see the layer.
        // A 32-bit program takes the x86 DLL and its own stub shape.
        let started = Instant::now();

        if let Some(early) = launch.early.as_ref() {
            let bits = if wow64 {
                stage0::Bits::X86
            } else {
                stage0::Bits::X64
            };
            let patched = stage0::install(info.hProcess, early.config, bits).map_err(|error| {
                eprintln!(
                    "gobbo: the early redirect failed in pid {}: {error}",
                    info.dwProcessId
                );
                stop_suspended(&info, error)
            })?;

            if let Err(error) = std::fs::write(early.handover, patched.handover()) {
                eprintln!("gobbo: cannot write the early redirect handover: {error}");
                return Err(stop_suspended(&info, error));
            }
        }

        timings.stage0_us = micros(started);
        // The event is created before the hook can signal it. The hook signals after its hooks are installed.
        let started = Instant::now();
        let ready = match ReadyEvent::create(info.dwProcessId) {
            Ok(ready) => ready,
            Err(error) => {
                return Err(stop_suspended(&info, error));
            }
        };
        timings.ready_event_us = micros(started);
        let started = Instant::now();
        let injected = if wow64 {
            inject32(info.dwProcessId, info.dwThreadId, hook)
        } else {
            // `info` is the process and thread `CreateProcessW` returned; the thread is suspended.
            inject_early(info.hProcess, info.hThread, hook)
        };

        if let Err(error) = injected {
            eprintln!(
                "gobbo: the hook failed in pid {}, stopping the game: {error}",
                info.dwProcessId
            );
            return Err(stop_suspended(&info, error));
        }

        timings.inject_us = micros(started);
        // The loader resolves the program's static imports after this function resumes the thread. Point the descriptors
        // whose DLL is in the mod layer at the layer path, so the loader does not look for a file in the game folder.
        // `info` is the suspended process `CreateProcessW` returned.
        let started = Instant::now();

        if let Err(error) = static_imports::rewrite(info.hProcess, launch.program, launch.imports) {
            eprintln!(
                "gobbo: the static imports of pid {} could not be redirected: {error}",
                info.dwProcessId
            );
            return Err(stop_suspended(&info, error));
        }

        eprintln!(
            "gobbo: hook stub in pid {}, resuming the game",
            info.dwProcessId
        );
        timings.imports_us = micros(started);
        let started = Instant::now();

        if ResumeThread(info.hThread) == u32::MAX {
            // A game that stays suspended would never show a window: it is stopped with the error instead.
            let error = io::Error::last_os_error();
            return Err(stop_suspended(&info, error));
        }

        timings.resume_us = micros(started);
        let started = Instant::now();

        if let Err(error) = ready.wait(Some(info.hProcess)) {
            eprintln!(
                "gobbo: the hook did not become ready in pid {}, stopping the game: {error}",
                info.dwProcessId
            );
            return Err(stop_suspended(&info, error));
        }

        timings.ready_wait_us = micros(started);
        CloseHandle(info.hThread);
        Ok(Hooked {
            process: info.hProcess,
            timings,
        })
    }
}

/// Marks handles inheritable for one process start. They are private again when this is dropped, so a handle that
/// is only passed to the program does not stay inheritable for the rest of Gobbo's life.
struct Inheritable(Vec<HANDLE>);

impl Inheritable {
    fn new(handles: &[HANDLE]) -> io::Result<Self> {
        let mut marked = Self(Vec::new());

        for &handle in handles {
            // SAFETY: the handle is one Gobbo opened; `SetHandleInformation` only changes its inheritance flag.
            if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) }
                == FALSE
            {
                return Err(io::Error::last_os_error());
            }

            marked.0.push(handle);
        }

        Ok(marked)
    }
}

impl Drop for Inheritable {
    fn drop(&mut self) {
        for &handle in &self.0 {
            // SAFETY: the handles were marked by `new`, and they are still open while the start is.
            unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
        }
    }
}

/// The attribute list that limits what a started process inherits to the given handles.
struct HandleList(Vec<usize>);

impl HandleList {
    fn new(handles: &[HANDLE]) -> io::Result<Self> {
        let mut size = 0;
        // SAFETY: this first call only asks for the size of the list; it fails by design.
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size) };
        // The list needs pointer alignment, so it is kept in words.
        let mut list = Self(vec![0; size.div_ceil(size_of::<usize>())]);
        let attributes = list.as_ptr();
        // SAFETY: the buffer is `size` bytes long, as the call above asked.
        if unsafe { InitializeProcThreadAttributeList(attributes, 1, 0, &mut size) } == FALSE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `handles` lives until the call returns, and the list holds its own copy of the handle values.
        let updated = unsafe {
            UpdateProcThreadAttribute(
                attributes,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                std::mem::size_of_val(handles),
                null_mut(),
                null_mut(),
            )
        };

        if updated == FALSE {
            return Err(io::Error::last_os_error());
        }

        Ok(list)
    }

    fn as_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.0.as_mut_ptr().cast()
    }
}

impl Drop for HandleList {
    fn drop(&mut self) {
        // SAFETY: the list was initialized by `new` and is deleted only here.
        unsafe { DeleteProcThreadAttributeList(self.as_ptr()) };
    }
}

/// Sets the stub in the main thread of a 32-bit process through the 32-bit helper. A 64-bit process cannot set the
/// context of a 32-bit thread, so the helper (built for x86) opens the thread and writes the x86 stub. The helper only
/// sets the context and exits; the caller resumes the thread and waits for the hook's readiness event.
pub fn inject32(pid: u32, tid: u32, hook: &Path) -> io::Result<()> {
    let helper = hook.with_file_name("gobbo_overlay_inject32.exe");

    if !helper.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "the 32-bit overlay injector is missing at {}",
                helper.display()
            ),
        ));
    }

    let mut child = std::process::Command::new(&helper)
        .arg(pid.to_string())
        .arg(tid.to_string())
        .arg(hook)
        .spawn()?;
    // The game stays suspended until this helper has set the x86 context. A helper that hangs must not leave it that
    // way forever; the launcher stops the process on the error below. Waiting on the process handle instead of polling
    // `try_wait` keeps the same deadline without waking every 10 ms while the helper works.
    let deadline =
        Instant::now() + std::time::Duration::from_millis(u64::from(HOOK_LOAD_TIMEOUT_MS));
    let timed_out = || {
        io::Error::new(
            io::ErrorKind::TimedOut,
            format!("the 32-bit overlay injector did not finish in pid {pid}"),
        )
    };
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        let _ = child.kill();
        let _ = child.wait();
        return Err(timed_out());
    }
    let millis = u32::try_from(remaining.as_millis()).unwrap_or(HOOK_LOAD_TIMEOUT_MS);
    // SAFETY: `as_raw_handle` is the process handle `child` owns until it is reaped by `wait` below. One wait covers the
    // whole remaining budget, so there is no polling loop.
    let status = match unsafe { WaitForSingleObject(child.as_raw_handle(), millis) } {
        WAIT_OBJECT_0 => match child.wait() {
            Ok(status) => status,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        },
        WAIT_TIMEOUT => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(timed_out());
        }
        WAIT_FAILED => {
            let error = io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        unexpected => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other(format!(
                "waiting for the 32-bit overlay injector in pid {pid} returned {unexpected}"
            )));
        }
    };
    if !status.success() {
        return Err(io::Error::other(format!(
            "the 32-bit overlay injector failed in pid {pid} ({status})"
        )));
    }

    Ok(())
}

/// Whether the process is a 32-bit program running under WOW64 on 64-bit Windows.
///
/// # Safety
/// `process` must be a valid process handle owned by the caller for the duration of the call.
pub unsafe fn is_wow64(process: HANDLE) -> io::Result<bool> {
    let mut wow64 = FALSE;
    // SAFETY: `process` is a process handle owned by the caller, and `wow64` is a valid BOOL for the call.
    if unsafe { IsWow64Process(process, &mut wow64) } == FALSE {
        return Err(io::Error::last_os_error());
    }

    Ok(wow64 != FALSE)
}

/// Stops the suspended process and closes the handles, for a launch that cannot continue.
fn stop_suspended(info: &PROCESS_INFORMATION, error: io::Error) -> io::Error {
    // SAFETY: `info` is the process and thread `CreateProcessW` returned, and the process is suspended.
    unsafe {
        TerminateProcess(info.hProcess, 1);
        CloseHandle(info.hThread);
        CloseHandle(info.hProcess);
    }

    error
}

/// The event the hook signals after it has installed its hooks. The launcher and the hook create it by process id before
/// the DLL is loaded, so the hook can open it from its initialization thread and the program is not resumed before the
/// hooks are in place.
pub struct ReadyEvent(HANDLE);

impl ReadyEvent {
    /// Creates the event for `pid`, unsignalled.
    pub fn create(pid: u32) -> io::Result<Self> {
        let name = ready_event_name(pid);
        // SAFETY: `name` is a NUL-terminated name and the event is ours; it is closed by `Drop`.
        let event = unsafe { CreateEventW(null(), TRUE, FALSE, name.as_ptr()) };

        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        // A stale event with a reused pid may be signalled; the new process must start from the unsignalled state.
        // SAFETY: `event` is an event handle opened above.
        unsafe { ResetEvent(event) };
        Ok(Self(event))
    }

    /// Waits until the hook reports that its installation finished. When `process` is given, the wait ends early if that
    /// process exits first; a child started by the hook has no such handle yet and only waits for its own event.
    pub fn wait(&self, process: Option<HANDLE>) -> io::Result<()> {
        let waited = match process {
            Some(process) => {
                let handles = [self.0, process];
                // SAFETY: both handles are valid for the duration of the call.
                unsafe { WaitForMultipleObjects(2, handles.as_ptr(), FALSE, HOOK_LOAD_TIMEOUT_MS) }
            }
            None => {
                // SAFETY: `self.0` is an event handle that lives until this value is dropped.
                unsafe { WaitForSingleObject(self.0, HOOK_LOAD_TIMEOUT_MS) }
            }
        };
        match (waited, process) {
            (WAIT_OBJECT_0, _) => Ok(()),
            (value, Some(process)) if value == WAIT_OBJECT_0 + 1 => {
                let mut code = 0;
                // SAFETY: `process` is the handle the caller owns.
                unsafe { GetExitCodeProcess(process, &mut code) };
                Err(io::Error::other(format!(
                    "the game exited before the overlay hook was ready (code {code})"
                )))
            }
            (WAIT_TIMEOUT, _) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "the overlay hook did not report ready within {} seconds",
                    HOOK_LOAD_TIMEOUT_MS / 1000
                ),
            )),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

impl Drop for ReadyEvent {
    fn drop(&mut self) {
        // SAFETY: the handle was created by `create` and is closed only here.
        unsafe { CloseHandle(self.0) };
    }
}

/// The name of the readiness event of the process `pid`.
pub fn ready_event_name(pid: u32) -> Vec<u16> {
    wide(OsStr::new(&format!("Local\\GobboOverlayReady_{pid}")))
}

/// The command line: the program and each argument, quoted so that spaces and quotes survive. Everything stays in
/// UTF-16 so a Windows path that is not valid UTF-8 reaches `CreateProcessW` unchanged.
fn command_line(program: &Path, args: &[String]) -> Vec<u16> {
    let mut line = quote(&program.as_os_str().encode_wide().collect::<Vec<_>>());

    for arg in args {
        line.push(u16::from(b' '));
        line.extend(quote_argument(&arg.encode_utf16().collect::<Vec<_>>()));
    }

    line.push(0);
    line
}

/// One argument for the command line. A switch such as `/c` or `-windowed` stays unquoted: `cmd.exe` treats a quoted
/// `/c` as a program name and fails. Everything else goes through `quote`, which keeps spaces and embedded quotes.
fn quote_argument(arg: &[u16]) -> Vec<u16> {
    let is_switch = arg
        .first()
        .is_some_and(|unit| *unit == u16::from(b'/') || *unit == u16::from(b'-'))
        && !char::decode_utf16(arg.iter().copied())
            .any(|character| character.is_ok_and(|c| c.is_whitespace() || c == '"'));
    if is_switch { arg.to_vec() } else { quote(arg) }
}

/// One argument, quoted by the rules the C runtime uses to split a command line. The rules only look at the ASCII
/// backslash and quote, so operating on UTF-16 units gives the same bytes as operating on characters.
fn quote(arg: &[u16]) -> Vec<u16> {
    let backslash = u16::from(b'\\');
    let quote_unit = u16::from(b'"');
    let mut out = Vec::with_capacity(arg.len() + 2);
    out.push(quote_unit);
    let mut backslashes = 0;

    for &unit in arg {
        if unit == backslash {
            backslashes += 1;
            continue;
        }

        if unit == quote_unit {
            out.extend(std::iter::repeat_n(backslash, backslashes * 2 + 1));
            out.push(quote_unit);
        } else {
            out.extend(std::iter::repeat_n(backslash, backslashes));
            out.push(unit);
        }

        backslashes = 0;
    }

    out.extend(std::iter::repeat_n(backslash, backslashes * 2));
    out.push(quote_unit);
    out
}

/// The environment of the program: Gobbo's own variables, with `extra` added or replaced. The block is UTF-16
/// `NAME=value` pairs, each ended by NUL, and the whole block ended by one more NUL.
fn environment_block(extra: &[(String, String)]) -> Vec<u16> {
    // `vars_os`, not `vars`: a variable with a value that is not Unicode would make `vars` panic, and the game must
    // still start with its environment as it is.
    let mut block: Vec<u16> = Vec::new();
    let inherited = std::env::vars_os().filter(|(name, _)| {
        !extra
            .iter()
            .any(|(key, _)| OsStr::new(key).eq_ignore_ascii_case(name))
    });
    let added = extra
        .iter()
        .map(|(name, value)| (OsString::from(name), OsString::from(value)));
    for (name, value) in inherited.chain(added) {
        block.extend(name.encode_wide());
        block.push(u16::from(b'='));
        block.extend(value.encode_wide());
        block.push(0);
    }

    block.push(0);
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_with_spaces_and_quotes_survive_the_command_line() {
        let units = |text: &str| text.encode_utf16().collect::<Vec<_>>();
        assert_eq!(quote(&units("plain")), units(r#""plain""#));
        assert_eq!(quote(&units(r#"say "hi""#)), units(r#""say \"hi\"""#));
        assert_eq!(
            quote(&units(r"C:\Games\Dir\")),
            units(r#""C:\Games\Dir\\""#)
        );
    }

    #[test]
    fn a_switch_is_not_quoted_for_cmd() {
        let units = |text: &str| text.encode_utf16().collect::<Vec<_>>();
        assert_eq!(quote_argument(&units("/c")), units("/c"));
        assert_eq!(quote_argument(&units("-windowed")), units("-windowed"));
        assert_eq!(
            quote_argument(&units("/path with spaces")),
            units(r#""/path with spaces""#)
        );
        assert_eq!(
            quote_argument(&units("echo hello")),
            units(r#""echo hello""#)
        );
    }

    #[test]
    fn a_program_path_with_an_unpaired_surrogate_reaches_the_command_line_unchanged() {
        use std::os::windows::ffi::OsStringExt;

        let mut name = OsStr::new(r"C:\Games\").encode_wide().collect::<Vec<_>>();
        name.push(0xD800);
        name.extend(OsStr::new(r"\game.exe").encode_wide());
        let program = PathBuf::from(OsString::from_wide(&name));

        let mut expected = vec![u16::from(b'"')];
        expected.extend_from_slice(&name);
        expected.push(u16::from(b'"'));
        expected.push(0);

        assert_eq!(command_line(&program, &[]), expected);
    }

    #[test]
    fn the_environment_block_replaces_names_case_insensitively() {
        let block = environment_block(&[("GOBBO_TEST_OVERLAY".to_string(), "value".to_string())]);
        let text = String::from_utf16_lossy(&block);
        assert!(text.contains("GOBBO_TEST_OVERLAY=value\0"));
        assert!(block.ends_with(&[0, 0]));
    }
}
