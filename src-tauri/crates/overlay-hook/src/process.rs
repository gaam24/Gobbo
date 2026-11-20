//! Child processes. A game starts its helper programs (for example the script compiler) through `CreateProcess`. The hook
//! sees every such start: the child starts suspended, this DLL is loaded into it, and then it runs, so the child is
//! redirected like the game. A caller that asked for a suspended start gets the child suspended, as it asked.

use std::ffi::c_void;
use std::path::PathBuf;
use std::ptr::null;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use gobbo_overlay_core::{Layer, join_rel, relative_to};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_DLL_INIT_FAILED, ERROR_INVALID_FUNCTION, FALSE, TRUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CreateEventW, ResumeThread, TerminateProcess, WaitForSingleObject,
};

use crate::text::display_name;
use crate::trace::log;
use crate::win32::PreserveLastError;

/// How long a resumed child may take to report that its hooks are installed. The child runs no game code before that
/// report, so a timeout means the hook is not working and the child must not continue unhooked.
const CHILD_READY_TIMEOUT_MS: u32 = 20_000;

/// The hook DLLs of this installation: the native one and the 32-bit one for WOW64 children.
struct HookPaths {
    native: PathBuf,
    wow64: PathBuf,
}

/// The hook paths, from the hook init.
static HOOK_PATHS: OnceLock<HookPaths> = OnceLock::new();
/// Whether `CreateProcessInternalW` was installed and enabled. The documented fallbacks pass a start to it only when it
/// can do the work. A failed enable leaves the fallback in charge instead of silently running the child unhooked.
static INTERNAL_HOOK: AtomicBool = AtomicBool::new(false);

/// Records whether the `CreateProcessInternalW` detour is active. Called once from `install`, before the game runs.
pub(crate) fn set_internal_hook(enabled: bool) {
    INTERNAL_HOOK.store(enabled, Ordering::Release);
}

/// Keeps the paths of this DLL and its 32-bit sibling, from the install thread.
pub(crate) fn remember_hook(path: PathBuf) {
    let native = sibling_or_parent(&path, "gobbo_overlay_hook.dll");
    let wow64 = sibling_or_parent(&path, "gobbo_overlay_hook32.dll");
    let _ = HOOK_PATHS.set(HookPaths { native, wow64 });
}

/// `name` next to `dll`. In a debug build it may also be one folder above: the cargo test layout loads the hook from
/// `target/debug/deps` while staging the 32-bit DLL in `target/debug`. A release build keeps only the sibling lookup,
/// because production stages both DLLs in one folder; a test layout has no branch there.
fn sibling_or_parent(dll: &std::path::Path, name: &str) -> PathBuf {
    let sibling = dll.with_file_name(name);

    if sibling.is_file() {
        return sibling;
    }

    #[cfg(debug_assertions)]
    {
        if let Some(folder) = dll.parent().and_then(std::path::Path::parent) {
            let above = folder.join(name);

            if above.is_file() {
                return above;
            }
        }
    }

    sibling
}

/// `PROCESS_INFORMATION` as `CreateProcessW` fills it.
#[repr(C)]
struct ProcessInformation {
    process: *mut c_void,
    thread: *mut c_void,
    process_id: u32,
    thread_id: u32,
}

/// Whether `CreateProcessInternalW` is hooked and enabled. When it is, the documented `CreateProcessW` and
/// `CreateProcessAsUserW` hooks leave the start to it, so a process is suspended and injected once.
fn internal_present() -> bool {
    !crate::diag::settings().disabled(crate::detours::PROCESS_INTERNAL)
        && INTERNAL_HOOK.load(Ordering::Acquire)
}

/// Which `CreateProcess` entry point received the call. The internal detour is the injector and always injects here; a
/// documented fallback injects only when the internal detour is absent, so a new caller cannot decide this on its own.
enum CallSite {
    Internal,
    Fallback,
}

/// `CreateProcessInternalW`: the child starts suspended, the hook is loaded into it, and then it runs, unless the caller
/// asked for a suspended start.
///
/// # Safety
/// Called by the program with the arguments of `CreateProcessInternalW`.
pub(crate) unsafe extern "system" fn create_process(
    token: *mut c_void,
    application: *const u16,
    command: *mut u16,
    attributes: *const c_void,
    thread_attributes: *const c_void,
    inherit: i32,
    flags: u32,
    environment: *const c_void,
    directory: *const u16,
    startup: *const c_void,
    information: *mut c_void,
    new_token: *mut c_void,
) -> i32 {
    let original = original!(create_process, {
        crate::win32::fail(ERROR_INVALID_FUNCTION)
    });

    unsafe {
        start_child_or_call(
            CallSite::Internal,
            application,
            command,
            flags,
            information,
            |flags| {
                original.call(
                    token,
                    application,
                    command,
                    attributes,
                    thread_attributes,
                    inherit,
                    flags,
                    environment,
                    directory,
                    startup,
                    information,
                    new_token,
                )
            },
        )
    }
}

/// `CreateProcessW`: only does the work when `CreateProcessInternalW` is not exported. The call is then the outermost
/// one, so the child is suspended and injected here.
///
/// # Safety
/// Called by the program with the arguments of `CreateProcessW`.
pub(crate) unsafe extern "system" fn create_process_w(
    application: *const u16,
    command: *mut u16,
    attributes: *const c_void,
    thread_attributes: *const c_void,
    inherit: i32,
    flags: u32,
    environment: *const c_void,
    directory: *const u16,
    startup: *const c_void,
    information: *mut c_void,
) -> i32 {
    let original = original!(create_process_w, {
        crate::win32::fail(ERROR_INVALID_FUNCTION)
    });

    unsafe {
        start_child_or_call(
            CallSite::Fallback,
            application,
            command,
            flags,
            information,
            |flags| {
                original.call(
                    application,
                    command,
                    attributes,
                    thread_attributes,
                    inherit,
                    flags,
                    environment,
                    directory,
                    startup,
                    information,
                )
            },
        )
    }
}

/// `CreateProcessAsUserW`: the fallback for a start with another user's token when `CreateProcessInternalW` is missing.
///
/// # Safety
/// Called by the program with the arguments of `CreateProcessAsUserW`.
pub(crate) unsafe extern "system" fn create_process_as_user(
    token: *mut c_void,
    application: *const u16,
    command: *mut u16,
    attributes: *const c_void,
    thread_attributes: *const c_void,
    inherit: i32,
    flags: u32,
    environment: *const c_void,
    directory: *const u16,
    startup: *const c_void,
    information: *mut c_void,
) -> i32 {
    let original = original!(create_process_as_user, {
        crate::win32::fail(ERROR_INVALID_FUNCTION)
    });

    unsafe {
        start_child_or_call(
            CallSite::Fallback,
            application,
            command,
            flags,
            information,
            |flags| {
                original.call(
                    token,
                    application,
                    command,
                    attributes,
                    thread_attributes,
                    inherit,
                    flags,
                    environment,
                    directory,
                    startup,
                    information,
                )
            },
        )
    }
}

/// Starts a child suspended, loads the hook into it, and resumes it unless the caller asked for a suspended start.
/// `start` is the original call with the modified flags and the same information pointer.
unsafe fn start_child(
    application: *const u16,
    command: *const u16,
    flags: u32,
    information: *mut c_void,
    start: impl FnOnce(u32, *mut c_void) -> i32,
) -> i32 {
    let Some(_guard) = crate::guard::Guard::enter() else {
        // A hook (the 32-bit injector launcher) is starting this process. Do not suspend it and do not inject: the
        // helper is 32-bit, but the current hook may be 64-bit, and the caller is already inside the hook.
        return start(flags, information);
    };
    let wants_suspended = flags & CREATE_SUSPENDED != 0;
    let started = start(flags | CREATE_SUSPENDED, information);

    if started == 0 {
        return started;
    }

    let _last_error = PreserveLastError::capture();
    // SAFETY: the call succeeded, so the structure the caller passed is filled in.
    let child = unsafe { &*(information as *const ProcessInformation) };
    // SAFETY: the strings are the ones the program passed to `CreateProcess`, and a null one shows as `<none>`.
    let (application_text, command_text) =
        unsafe { (display_name(application), display_name(command)) };
    log(&format!(
        "child process {} started: application {application_text}, command {command_text}",
        child.process_id
    ));
    // A child that the caller leaves suspended gets its stub now and installs the hook when the caller resumes it.
    // A child that is about to run has an event the hook signals after installation, so this function can verify it.
    let ready = (!wants_suspended)
        .then(|| ChildReady::create(child.process_id))
        .flatten();
    let hooked = HOOK_PATHS
        .get()
        .is_some_and(|paths| inject_child(paths, child, application, command));
    if !hooked {
        if !crate::diag::settings().allow_unhooked_children {
            log(&format!(
                "child process {} could not be hooked; stopping it (set GOBBO_OVERLAY_ALLOW_UNHOOKED_CHILDREN=1 to allow it)",
                child.process_id
            ));
            drop(_last_error);
            return stop_unhooked(child);
        }

        log(&format!(
            "child process {} could not be hooked; running it unhooked by request",
            child.process_id
        ));
    }

    if !wants_suspended {
        // SAFETY: the thread handle is the one `CreateProcessW` returned for the child.
        unsafe { ResumeThread(child.thread) };

        if hooked
            && let Some(ready) = &ready
            && !ready.wait()
        {
            log(&format!(
                "child process {} did not report its hook ready",
                child.process_id
            ));

            if !crate::diag::settings().allow_unhooked_children {
                drop(_last_error);
                return stop_unhooked(child);
            }
        }
    }

    started
}

/// Starts the child through the hook, or passes the call to the original when the internal detour will inject it.
///
/// # Safety
/// The arguments must be the ones of the `CreateProcess` call, and `call` the original function with the flags.
unsafe fn start_child_or_call(
    site: CallSite,
    application: *const u16,
    command: *const u16,
    flags: u32,
    information: *mut c_void,
    call: impl FnOnce(u32) -> i32,
) -> i32 {
    // The internal detour is this call path, so it injects here. A fallback defers to it when it is active; otherwise
    // the inner hook would never see this start and the fallback must inject.
    let internal_present = match site {
        CallSite::Internal => false,
        CallSite::Fallback => internal_present(),
    };
    if internal_present {
        return call(flags);
    }
    // SAFETY: the caller guarantees the arguments belong to the CreateProcess call being hooked.
    unsafe {
        start_child(application, command, flags, information, |flags, _| {
            call(flags)
        })
    }
}

/// The readiness event of a child process. The hook opens it by name and signals it after its hooks are installed.
struct ChildReady(windows_sys::Win32::Foundation::HANDLE);

impl ChildReady {
    /// Creates the event for `pid`, or `None` when Windows refuses. A missing event only means the parent cannot
    /// verify the child; the child's hook still runs.
    fn create(pid: u32) -> Option<Self> {
        let name = gobbo_overlay_launch::ready_event_name(pid);
        // SAFETY: `name` is a NUL-terminated name; the event is owned by this value and closed in `Drop`.
        let event = unsafe { CreateEventW(null(), TRUE, FALSE, name.as_ptr()) };

        if event.is_null() {
            log(&format!(
                "could not create the readiness event of child process {pid}: {}",
                std::io::Error::last_os_error()
            ));
            return None;
        }
        // A stale event with a reused pid may be signalled; the new child must start unsignalled.
        // SAFETY: `event` is an event handle opened above.
        unsafe { windows_sys::Win32::System::Threading::ResetEvent(event) };
        Some(Self(event))
    }

    /// Waits until the child's hook reports ready.
    fn wait(&self) -> bool {
        // SAFETY: `self.0` is an event handle that lives until this value is dropped.
        unsafe { WaitForSingleObject(self.0, CHILD_READY_TIMEOUT_MS) == WAIT_OBJECT_0 }
    }
}

impl Drop for ChildReady {
    fn drop(&mut self) {
        // SAFETY: the handle was created by `create` and is closed only here.
        unsafe { CloseHandle(self.0) };
    }
}

/// Loads the matching hook into `child`. `false` when the child must not run unhooked; the caller decides whether the
/// explicit opt-out allows it.
fn inject_child(
    paths: &HookPaths,
    child: &ProcessInformation,
    application: *const u16,
    command: *const u16,
) -> bool {
    // SAFETY: `child.process` is the process handle `CreateProcess` returned for the child, valid for this call.
    let wow64 = match unsafe { gobbo_overlay_launch::is_wow64(child.process) } {
        Ok(wow64) => wow64,
        Err(error) => {
            log(&format!(
                "could not ask whether a child process is 32-bit: {error}"
            ));
            return false;
        }
    };

    if cfg!(target_pointer_width = "32") && !wow64 {
        // A 32-bit hook cannot create a remote thread with a 64-bit start address in a 64-bit child.
        log("a 32-bit hook cannot load into a 64-bit child");
        return false;
    }
    // The child's own static imports may name a DLL from the mod layer. The child is still suspended, so its import
    // descriptors can be pointed at the layer before it runs, exactly as for the game itself.
    if let Some(program) = child_program(application, command) {
        let imports = child_imports(&program);

        if !imports.is_empty() {
            // SAFETY: `child.process` is the suspended process `CreateProcess` returned.
            if let Err(error) = unsafe {
                gobbo_overlay_launch::static_imports::rewrite(child.process, &program, &imports)
            } {
                log(&format!(
                    "could not redirect the static imports of a child process: {error}"
                ));
                return false;
            }
        }
    }

    if wow64 && cfg!(target_pointer_width = "64") {
        // A 64-bit hook cannot set the context of a 32-bit thread. The 32-bit helper does it. The guard keeps the
        // helper's own process start out of this hook.
        let _guard = crate::guard::Guard::enter();
        return match gobbo_overlay_launch::inject32(child.process_id, child.thread_id, &paths.wow64)
        {
            Ok(()) => true,
            Err(error) => {
                log(&format!(
                    "could not set the 32-bit hook stub in a child process: {error}"
                ));
                false
            }
        };
    }

    let hook = if wow64 { &paths.wow64 } else { &paths.native };
    // SAFETY: the process and thread handles are the ones `CreateProcess` returned, and the thread is suspended.
    match unsafe { gobbo_overlay_launch::inject_early(child.process, child.thread, hook) } {
        Ok(()) => true,
        Err(error) => {
            log(&format!(
                "could not set the hook stub in a child process: {error}"
            ));
            false
        }
    }
}

/// The program a child starts: the application name, or the first token of the command line.
fn child_program(application: *const u16, command: *const u16) -> Option<PathBuf> {
    let application = unsafe { display_name(application) };

    if application != "<none>" && !application.is_empty() {
        return Some(PathBuf::from(application));
    }

    let command = unsafe { crate::text::text(command) }?;
    let command = command.trim_start();
    let program = if let Some(rest) = command.strip_prefix('"') {
        rest.split('"').next()
    } else {
        command.split_whitespace().next()
    }?;
    Some(PathBuf::from(program))
}

/// The static imports of a child whose DLL is in the mod layer. An empty list leaves the child alone; the hook still
/// covers every DLL it loads after its first instruction.
fn child_imports(program: &std::path::Path) -> Vec<(String, PathBuf)> {
    let Some(layers) = crate::state::layers() else {
        return Vec::new();
    };
    let Some(folder) = program.parent() else {
        return Vec::new();
    };
    let Some(relative) = relative_to(
        &layers.root.display().to_string(),
        &folder.display().to_string(),
    ) else {
        return Vec::new();
    };
    let whiteouts = crate::state::whiteouts_snapshot();
    let mut imports = Vec::new();

    for import in gobbo_overlay_launch::imports(program).unwrap_or_default() {
        if import.name.is_empty() {
            continue;
        }

        let game_rel = join_rel(&relative, &import.name.replace('\\', "/"));
        let Some((layer, path)) =
            layers.resolve_visible(&game_rel, std::path::Path::is_file, &whiteouts)
        else {
            continue;
        };

        if layer != Layer::Root {
            imports.push((import.name, path));
        }
    }

    imports
}

/// Stops a child that could not receive the hook, and closes the handles the caller no longer receives. The call fails
/// with `ERROR_DLL_INIT_FAILED`, so a game cannot silently run an unhooked helper against the real folder.
fn stop_unhooked(child: &ProcessInformation) -> i32 {
    // SAFETY: the process and thread handles are the ones `CreateProcess` returned, and each is closed once. A child
    // that survives termination is never resumed; only the report of the failure changes.
    unsafe {
        if TerminateProcess(child.process, ERROR_DLL_INIT_FAILED) == FALSE {
            log(&format!(
                "could not terminate unhooked child process {}: {}",
                child.process_id,
                std::io::Error::last_os_error()
            ));
        }
        CloseHandle(child.thread);
        CloseHandle(child.process);
    }

    crate::win32::set_error(ERROR_DLL_INIT_FAILED);
    0
}
