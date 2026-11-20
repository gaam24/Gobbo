//! The crash report of a dev build: where the program stopped, and the last trace lines before that. It is written for an
//! exception that ends a process, and for a Rust panic. The hooks are `extern "system"` functions, so a panic inside one
//! ends the process; the panic hook writes the report before that happens.
//!
//! A 64-bit hook connects to the minidumper in Gobbo; a 32-bit hook connects to the 32-bit helper
//! (`GOBBO_OVERLAY_DUMPER32`), because the crash context carries pointer-sized fields. When no monitor is available,
//! the hook falls back to the in-process DbgHelp dump (`GOBBO_OVERLAY_DUMP`).
//!
//! The exception handler sees first-chance exceptions. A program that handles an access violation itself also gets a
//! report for it, and the report says "first-chance", so such a case is not mistaken for the crash.

use std::ffi::{OsStr, c_void};
use std::os::windows::io::AsRawHandle;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use windows_sys::Win32::System::Diagnostics::Debug::SetUnhandledExceptionFilter;
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS,
    MINIDUMP_EXCEPTION_INFORMATION, MiniDumpNormal,
};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId};

use crate::text::wide;

use super::module::locate_cached;
use super::settings::settings;
use super::trace_buffer::recent_lines;
use super::writer::append_file;

/// The minidumper client of this process, connected once at install.
static DUMPER: OnceLock<Option<minidumper::Client>> = OnceLock::new();

/// The crash handler that asks the minidumper for a dump. It must stay alive for the process, or it detaches.
static DUMP_HANDLER: OnceLock<crash_handler::CrashHandler> = OnceLock::new();

/// Whether the out-of-process dump path is established: the client connected and its handler attached. Set at install
/// by the successful attach, not by the filter's re-arm thread, so a crash inside the first 250 ms still takes only the
/// out-of-process path.
static DUMPER_ATTACHED: AtomicBool = AtomicBool::new(false);

/// Set once a dump was requested, so a filter and an abort handler do not ask for two.
static DUMP_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The DbgHelp entry point, loaded only when an in-process dump is actually written. Keeping it out of the import table
/// means `dbghelp.dll` is not loaded before the game's own DLLs, so a mod can still provide a file with that name.
type MiniDumpWriteDumpFn = unsafe extern "system" fn(
    *mut c_void,
    u32,
    *mut c_void,
    i32,
    *const MINIDUMP_EXCEPTION_INFORMATION,
    *const c_void,
    *const c_void,
) -> i32;

/// The one DbgHelp function this process uses. The module is never freed, so the pointer stays valid until exit.
struct DbgHelp {
    write_dump: MiniDumpWriteDumpFn,
}

static DBGHELP: OnceLock<Option<DbgHelp>> = OnceLock::new();

/// `MiniDumpWriteDump`, loaded lazily from `System32` only.
fn dbghelp() -> Option<&'static DbgHelp> {
    DBGHELP
        .get_or_init(|| {
            let name = wide(OsStr::new("dbghelp.dll"));
            // SAFETY: `name` is a NUL-terminated path, and the flags limit the search to System32.
            let module = unsafe {
                LoadLibraryExW(
                    name.as_ptr(),
                    std::ptr::null_mut(),
                    LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            };

            if module.is_null() {
                return None;
            }
            // SAFETY: `module` is a loaded module and the name is a NUL-terminated export.
            let address = unsafe { GetProcAddress(module, c"MiniDumpWriteDump".as_ptr().cast()) }?;
            // SAFETY: `MiniDumpWriteDump` has the signature of the type. The module stays loaded until exit.
            let write_dump = unsafe { std::mem::transmute_copy(&address) };
            Some(DbgHelp { write_dump })
        })
        .as_ref()
}

/// The filter that was installed before ours, as a function address, called after the dump request so the game keeps
/// its own crash behavior. Zero means there was none. An atomic address keeps the crash path lock-free.
type ExceptionFilter = unsafe extern "system" fn(*const EXCEPTION_POINTERS) -> i32;
static PREVIOUS_FILTER: AtomicUsize = AtomicUsize::new(0);

/// How long after the hooks are ready the filter is put on top again. A runtime such as .NET or a game engine installs
/// its own filter while it starts, and would otherwise replace ours; repeating the install catches a late start too.
const REARM_DELAYS: [Duration; 5] = [
    Duration::from_millis(250),
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(10),
    Duration::from_secs(60),
];

/// The exception code of an access violation.
const ACCESS_VIOLATION: u32 = 0xC000_0005;

/// The exceptions that end a process, with the name the report gives each one.
const CRASH_CODES: [(u32, &str); 6] = [
    (ACCESS_VIOLATION, "access violation"),
    (0xC000_00FD, "stack overflow"),
    (0xC000_0374, "heap corruption"),
    (0xC000_001D, "illegal instruction"),
    (0xC000_0094, "integer division by zero"),
    (0xC000_0409, "stack buffer overrun"),
];

/// The name of an exception code that ends a process, or `None` for any other exception.
fn crash_name(code: u32) -> Option<&'static str> {
    CRASH_CODES
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, name)| *name)
}

/// What an access violation says about the access: the parameters are the kind of access (0 read, 1 write, 8 data
/// execution) and the address it touched. Empty when the parameters are missing.
fn access_text(parameters: &[usize]) -> String {
    match parameters {
        [kind, address, ..] => {
            let kind = match kind {
                0 => "read",
                1 => "write",
                8 => "data execution",
                _ => "access",
            };
            format!(" ({kind} at 0x{address:x})")
        }
        _ => String::new(),
    }
}

/// The text of a crash report: the headline, then the last trace lines, oldest first.
fn crash_report(headline: &str, events: &[String]) -> String {
    let mut text = format!(
        "=== {headline}\nlast {} trace lines, oldest first:\n",
        events.len()
    );

    for event in events {
        text.push_str(event);
        text.push('\n');
    }

    let dropped = super::writer::dropped_lines();

    if dropped > 0 {
        text.push_str(&format!("trace lines dropped: {dropped}\n"));
    }

    text.push('\n');
    text
}

/// Installs the crash reporting when `GOBBO_OVERLAY_CRASH` names a file, `GOBBO_OVERLAY_DUMP` names an in-process dump,
/// or `GOBBO_OVERLAY_DUMPER` names the out-of-process monitor. A last-chance filter sends the dump. The first-chance
/// report is opt-in with `GOBBO_OVERLAY_CRASH_FIRST`: a vectored handler sees every exception, including the ones a
/// program handles itself. The panic hook writes its report and then calls the hook that was there before, so the panic
/// is still printed.
pub(crate) fn install() {
    let switches = settings();

    if switches.crash.is_none() && switches.dump.is_none() && switches.dumper.is_none() {
        return;
    }

    if switches.crash.is_some() || switches.dump.is_some() {
        if switches.crash_first {
            // SAFETY: the handler has the signature the call takes, and it lives as long as the process.
            unsafe { AddVectoredExceptionHandler(1, Some(on_exception)) };
        }

        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            report_panic(info);
            previous(info);
        }));
    }
    // The in-process dump and the out-of-process monitor both end in the last-chance filter; the monitor re-arms it
    // after the program installs its own handlers.
    if switches.dump.is_some() || switches.dumper.is_some() {
        arm_filter();
        REARM_WANTED.store(true, Ordering::Release);
    }

    install_dumper();
}

/// Set when the filter thread is wanted. The thread starts with `start_background`, after the detours are enabled.
static REARM_WANTED: AtomicBool = AtomicBool::new(false);

/// Starts the filter thread once it is wanted. The install enables the detours with one thread in the process (see
/// `detours::enable`); this thread is started after that. Its first wake-up is after 250 ms, so the filter it puts back
/// is the same as if it had started earlier.
pub(crate) fn start_background() {
    if REARM_WANTED.swap(false, Ordering::AcqRel) {
        spawn_rearm();
    }
}

/// Puts the last-chance filter back on top after the program has had time to install its own, and again later for a
/// slow start. A runtime such as .NET replaces the filter while it starts.
fn spawn_rearm() {
    let armed = thread::Builder::new()
        .name("overlay-crash".to_string())
        .spawn(|| {
            for delay in REARM_DELAYS {
                thread::sleep(delay);
                arm_filter();
            }

            loop {
                thread::sleep(Duration::from_secs(60));
                arm_filter();
            }
        });
    if let Err(error) = armed {
        crate::trace::log(&format!("could not start the crash filter thread: {error}"));
    }
}

/// Connects to Gobbo's minidumper and installs the handler for an exception that no one else takes. A failure is logged
/// and leaves the in-process path, or no dump at all; it never stops the game.
fn install_dumper() {
    let Some(path) = settings().dumper.as_deref() else {
        return;
    };
    let client = match minidumper::Client::with_name(minidumper::SocketName::Path(path)) {
        Ok(client) => client,
        Err(error) => {
            crate::trace::log(&format!(
                "could not connect to the minidumper {}: {error}",
                path.display()
            ));
            return;
        }
    };

    if DUMPER.set(Some(client)).is_err() {
        return;
    }
    // SAFETY: the callback only sends the crash context to Gobbo and returns. It runs in a compromised context, so it
    // does no allocation of its own and no reporting; the minidumper client was opened before any hooked call.
    let event = unsafe {
        crash_handler::make_crash_event(|context| {
            crash_handler::CrashEventResult::Handled(send_context(context))
        })
    };

    match crash_handler::CrashHandler::attach(event) {
        Ok(handler) => {
            if DUMP_HANDLER.set(handler).is_ok() {
                arm_filter();
                DUMPER_ATTACHED.store(true, Ordering::Release);
                crate::trace::log("minidumper: connected, the out-of-process dump is armed");
            }
        }
        Err(error) => {
            crate::trace::log(&format!(
                "could not install the minidumper handler: {error}"
            ));
        }
    }
}

/// Installs `on_unhandled` and remembers the filter it replaced, unless that was already this one.
fn arm_filter() {
    // SAFETY: the filter has the signature Windows takes and only sends the context to the monitor.
    let previous = unsafe { SetUnhandledExceptionFilter(Some(on_unhandled)) };
    let address = previous.map_or(0, |filter| filter as *const () as usize);

    if address != on_unhandled as *const () as usize {
        PREVIOUS_FILTER.store(address, Ordering::Release);
        crate::trace::log(&format!(
            "unhandled filter armed; previous {}",
            if address == 0 { "none" } else { "set" }
        ));
    }
}

/// Whether the out-of-process dump path is armed, so the in-process fallback must stay quiet.
fn dumper_armed() -> bool {
    DUMPER_ATTACHED.load(Ordering::Acquire)
}

/// The last-chance filter: Windows calls it when no handler took the exception. It sends the context to Gobbo and lets
/// the process terminate as it would have.
///
/// # Safety
/// Windows passes the exception record of the crashing thread; it is valid for the duration of the call.
unsafe extern "system" fn on_unhandled(pointers: *const EXCEPTION_POINTERS) -> i32 {
    // The in-process fallback only writes when no out-of-process monitor is armed; a dump failure must not stop the
    // crash path.
    write_dump(pointers.cast_mut());
    // SAFETY: Windows passes the record of the exception being raised.
    let record = unsafe { pointers.as_ref() }
        .and_then(|pointers| unsafe { pointers.ExceptionRecord.as_ref() });
    if let Some(record) = record {
        let context = crash_handler::CrashContext {
            exception_pointers: pointers.cast(),
            // SAFETY: both calls only read the id of the calling process and thread.
            process_id: unsafe { GetCurrentProcessId() },
            thread_id: crate::win32::thread_id(),
            exception_code: record.ExceptionCode,
        };
        let _ = send_context(&context);
    }

    let previous = PREVIOUS_FILTER.load(Ordering::Acquire);

    if previous != 0 {
        // SAFETY: the address was a filter Windows had before this one, and it takes the same pointers.
        let previous: ExceptionFilter = unsafe { std::mem::transmute(previous) };
        return unsafe { previous(pointers) };
    }

    EXCEPTION_CONTINUE_SEARCH
}

/// Whether the out-of-process minidumper of this process is connected. The session report names it. The client alone is
/// not enough: a failed handler attach leaves the in-process path in charge, so that must report disconnected.
pub(super) fn dumper_connected() -> bool {
    DUMPER_ATTACHED.load(Ordering::Acquire)
}

/// Sends one crash context to Gobbo. Only the first request is sent; a failure lets a later path try again.
fn send_context(context: &crash_handler::CrashContext) -> bool {
    if DUMP_REQUESTED.swap(true, Ordering::AcqRel) {
        return false;
    }

    let Some(client) = DUMPER.get().and_then(Option::as_ref) else {
        DUMP_REQUESTED.store(false, Ordering::Release);
        return false;
    };
    let sent = client.request_dump(context).is_ok();

    if !sent {
        DUMP_REQUESTED.store(false, Ordering::Release);
    }

    sent
}

/// The vectored exception handler. It reports the exceptions that end a process, and passes every exception on.
///
/// # Safety
/// Called by Windows with the exception record of the thread that raised the exception.
unsafe extern "system" fn on_exception(pointers: *mut EXCEPTION_POINTERS) -> i32 {
    // SAFETY: Windows passes the record of the exception being raised, valid for the duration of the handler.
    let record = unsafe { pointers.as_ref() }
        .and_then(|pointers| unsafe { pointers.ExceptionRecord.as_ref() });
    if let Some(record) = record {
        let code = record.ExceptionCode as u32;

        if let Some(name) = crash_name(code) {
            write_dump(pointers);
            let count = (record.NumberParameters as usize).min(record.ExceptionInformation.len());
            let detail = if code == ACCESS_VIOLATION {
                access_text(&record.ExceptionInformation[..count])
            } else {
                String::new()
            };
            let place = locate_cached(record.ExceptionAddress as usize);
            report(&format!(
                "first-chance exception 0x{code:08X} ({name}){detail} at {place} on thread {}",
                crate::win32::thread_id()
            ));
        }
    }

    EXCEPTION_CONTINUE_SEARCH
}

/// Writes the report of a panic: where it happened, what it says, and where the call stack was.
fn report_panic(info: &std::panic::PanicHookInfo<'_>) {
    let place = info.location().map_or_else(
        || "an unknown place".to_string(),
        |at| format!("{}:{}", at.file(), at.line()),
    );
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|text| text.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_string());
    let stack = std::backtrace::Backtrace::force_capture();
    write_dump(std::ptr::null_mut());
    report(&format!(
        "panic at {place} on thread {}: {message}\n{stack}",
        crate::win32::thread_id()
    ));
}

/// Writes a minidump when `GOBBO_OVERLAY_DUMP` names a file. This is an in-process dump (DbgHelp), kept as a development
/// aid; an out-of-process `minidumper` is the robust production path. The exception pointers may be null for a panic.
fn write_dump(pointers: *mut EXCEPTION_POINTERS) {
    if dumper_armed() {
        return;
    }

    let Some(path) = &settings().dump else {
        return;
    };
    let Ok(file) = std::fs::File::create(path) else {
        // The crash path must not log through the trace locks here; a failed dev dump is silent.
        return;
    };
    let information = MINIDUMP_EXCEPTION_INFORMATION {
        ThreadId: crate::win32::thread_id(),
        ExceptionPointers: pointers,
        ClientPointers: 0,
    };
    let information = if pointers.is_null() {
        std::ptr::null()
    } else {
        &raw const information
    };
    let Some(dbghelp) = dbghelp() else {
        return;
    };
    // SAFETY: the file stays open for the call, the pointers came from the exception handler or are null, and the
    // process and the dump flags are the ones DbgHelp expects.
    unsafe {
        (dbghelp.write_dump)(
            GetCurrentProcess(),
            GetCurrentProcessId(),
            file.as_raw_handle(),
            MiniDumpNormal,
            information,
            std::ptr::null(),
            std::ptr::null(),
        );
    }
}

/// Writes one report to the crash file, with the last trace lines. The trace and the counts stop first, so a crash inside
/// them does not wait for the lock it holds.
fn report(headline: &str) {
    let Some(path) = &settings().crash else {
        return;
    };
    super::start_crash_report();
    let _ = append_file(path, &crash_report(headline, &recent_lines()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_access_violation_and_the_other_process_ending_codes_are_crashes() {
        assert_eq!(crash_name(0xC000_0005), Some("access violation"));
        assert_eq!(crash_name(0xC000_00FD), Some("stack overflow"));
        // A breakpoint is raised by debuggers and by the runtime on purpose, and it does not end the program.
        assert_eq!(crash_name(0x8000_0003), None);
    }

    #[test]
    fn an_access_violation_says_what_it_touched() {
        assert_eq!(access_text(&[0, 0x1234]), " (read at 0x1234)");
        assert_eq!(access_text(&[1, 0x20]), " (write at 0x20)");
        assert_eq!(access_text(&[]), "");
    }

    #[test]
    fn the_report_names_the_crash_and_lists_the_lines_oldest_first() {
        let events = [
            "#1 0ms thread=1 call A".to_string(),
            "#2 1ms thread=1 call B".to_string(),
        ];
        let text = crash_report("access violation at game.exe+0x10", &events);
        assert_eq!(
            text,
            "=== access violation at game.exe+0x10\nlast 2 trace lines, oldest first:\n#1 0ms thread=1 call A\n#2 1ms thread=1 call B\n\n"
        );
    }
}
