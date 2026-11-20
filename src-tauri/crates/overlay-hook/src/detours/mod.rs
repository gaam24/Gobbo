//! The detours: the table of the hooked exports (`detours!`), the detours made from it, and the lookup of the exports they
//! replace (`exports`). Every detour is declared once, in the table below.

pub(crate) mod exports;

use std::ffi::OsStr;

use detour::{Threads, Transaction, TypedDetour};
use exports::detour;
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;

use crate::diag::{self, HookStatus};
use crate::text::wide;
use crate::trace::log;

/// The module that exports a detoured call: the file API is in `kernel32` or `KernelBase`, the NT layer in `ntdll`.
#[derive(Clone, Copy)]
enum Module {
    File,
    Nt,
    Advapi,
}

impl Module {
    /// The name of the module; `file` is the one that exports the file API, found at start.
    fn name(self, file: &'static str) -> &'static str {
        match self {
            Module::File => file,
            Module::Nt => "ntdll.dll",
            Module::Advapi => "advapi32.dll",
        }
    }
}

/// Declares the detours once: the field that holds each one, the function type of its export, the module that exports
/// it, the export's name, and the hook that replaces it. The struct, its creation and its enabling come from this list,
/// and the names `GOBBO_OVERLAY_DISABLE` takes are the field names.
macro_rules! detours {
    ($($field:ident: $ty:ty = $module:ident $export:literal => $hook:path;)*) => {
        /// The detours. A detour whose export is missing is `None`; the other hooks are installed regardless. The table
        /// is created once, and enabled after it is stored, so a hooked call can always find the entry it needs.
        pub(crate) struct Hooks {
            $(pub(crate) $field: Option<TypedDetour<$ty>>,)*
        }

        impl Hooks {
            /// Creates every detour it can, with `file` as the module of the file API. A missing export is a failed
            /// status for that hook only: one absent optional export must not disable the rest. The returned statuses
            /// are in table order and are passed to `enable`.
            ///
            /// # Safety
            /// Called once, while the process is starting, with the Windows file API loaded.
            unsafe fn create(file: &'static str) -> (Self, Vec<HookStatus>) {
                let mut statuses = Vec::new();
                let hooks = unsafe {
                    Self {
                        $(
                            $field: match detour::<$ty>(Module::$module.name(file), $export, $hook as $ty) {
                                Ok(hook) => {
                                    statuses.push(HookStatus::created(stringify!($field)));
                                    Some(hook)
                                }
                                Err(error) => {
                                    statuses.push(HookStatus::failed(stringify!($field), error));
                                    None
                                }
                            },
                        )*
                    }
                };
                (hooks, statuses)
            }

            /// Enables every detour that was created and that `GOBBO_OVERLAY_DISABLE` does not name. The hooks are
            /// enabled together in one transaction, with the other threads suspended; if that fails, each one is
            /// retried on its own so the trace says which hook is at fault. A failed enable leaves the original
            /// function in place.
            ///
            /// # Safety
            /// Called once, after `create` and before the program runs any code.
            pub(crate) unsafe fn enable(&self, mut statuses: Vec<HookStatus>) -> Vec<HookStatus> {
                // `GobboOverlayInit` is reached from the launcher's stub before the program's first instruction, so
                // only the main thread exists. The Toolhelp snapshot walks every thread in the system and cost about
                // 30 ms of the install; the switch is the fallback for a manual load or an unusual launcher.
                let threads = if diag::settings().safe_threads {
                    let threads = thread_count();
                    log(&format!("install sees {threads} thread(s) in the process"));
                    threads
                } else {
                    log("install assumes the single thread of the early load");
                    1
                };
                let suspend = if threads == 1 { Threads::None } else { Threads::All };
                // A disabled hook keeps its created detour, but it is not part of the transaction.
                $(
                    if skipped(stringify!($field)) {
                        mark_disabled(&mut statuses, stringify!($field));
                        log(&format!("detour left off: {}", stringify!($field)));
                    }
                )*

                let mut total = 0;
                let mut transaction = Transaction::new();
                $(
                    if let Some(hook) = &self.$field
                        && !skipped(stringify!($field))
                    {
                        total += 1;
                        transaction.enable(hook);
                    }
                )*
                if total > 0 {
                    match unsafe { transaction.commit(suspend) } {
                        Ok(()) => {
                            $(
                                if self.$field.is_some()
                                    && !skipped(stringify!($field))
                                {
                                    mark_enabled(&mut statuses, stringify!($field));
                                }
                            )*
                            return statuses;
                        }
                        Err(error) => log(&format!(
                            "could not enable the hooks together: {error}; enabling them one at a time"
                        )),
                    }
                }

                $(
                    if let Some(hook) = &self.$field
                        && !skipped(stringify!($field))
                    {
                        let mut transaction = Transaction::new();
                        transaction.enable(hook);

                        match unsafe { transaction.commit(suspend) } {
                            Ok(()) => mark_enabled(&mut statuses, stringify!($field)),
                            Err(error) => {
                                log(&format!(
                                    "could not enable the {} hook: {error}",
                                    stringify!($field)
                                ));
                                mark_failed(&mut statuses, stringify!($field), error.to_string());
                            }
                        }
                    }
                )*
                statuses
            }
        }
    };
}

/// The number of threads of this process, the install included, for `GOBBO_OVERLAY_SAFE_THREADS`. The launcher starts
/// the program suspended and installs the hook before the program's code runs, so the normal path has one thread and
/// skips this snapshot. When the count is one, no other thread can run a patched function, and the transaction need not
/// suspend any; the detour library elsewhere suspends every thread and waits for each one. A failed snapshot answers
/// `usize::MAX`, which keeps the suspension.
fn thread_count() -> usize {
    // SAFETY: the snapshot is closed below; `entry` is a plain structure that the snapshot fills in.
    unsafe {
        let process = GetCurrentProcessId();
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);

        if snapshot == INVALID_HANDLE_VALUE {
            return usize::MAX;
        }

        let mut entry: THREADENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
        let mut threads = 0usize;
        let mut more = Thread32First(snapshot, &mut entry) != 0;

        while more {
            if entry.th32OwnerProcessID == process {
                threads += 1;
            }

            more = Thread32Next(snapshot, &mut entry) != 0;
        }

        CloseHandle(snapshot);
        threads
    }
}

/// Whether a hook is left off: named by `GOBBO_OVERLAY_DISABLE`, or a tracing-only hook while tracing is off. The
/// information hooks only feed `GOBBO_OVERLAY_CALLS`; leaving them off saves their detour and their per-call work.
fn skipped(name: &str) -> bool {
    if diag::settings().disabled(name) {
        return true;
    }

    !diag::settings().calls && matches!(name, "get_file_information")
}

/// Marks the status with `name` as enabled in the list `create` built.
fn mark_enabled(statuses: &mut [HookStatus], name: &str) {
    if let Some(status) = statuses.iter_mut().find(|status| status.name == name) {
        status.mark_enabled();
    }
}

/// Marks the status with `name` as failed in the list `create` built.
fn mark_failed(statuses: &mut [HookStatus], name: &str, error: String) {
    if let Some(status) = statuses.iter_mut().find(|status| status.name == name) {
        status.mark_failed(error);
    }
}

/// Marks the status with `name` as disabled in the list `create` built.
fn mark_disabled(statuses: &mut [HookStatus], name: &str) {
    if let Some(status) = statuses.iter_mut().find(|status| status.name == name) {
        status.mark_disabled();
    }
}

detours! {
    create_file: crate::types::CreateFileWFn = File "CreateFileW" => crate::win32::create_file;
    delete_file: crate::types::DeleteFileWFn = File "DeleteFileW" => crate::win32::delete_file;
    remove_directory: crate::types::RemoveDirectoryWFn = File "RemoveDirectoryW" => crate::win32::remove_directory;
    move_file_ex: crate::types::MoveFileExWFn = File "MoveFileExW" => crate::win32::move_file_ex;
    move_file_with_progress: crate::types::MoveFileWithProgressWFn = File "MoveFileWithProgressW" => crate::win32::move_file_with_progress;
    replace_file: crate::types::ReplaceFileWFn = File "ReplaceFileW" => crate::win32::replace_file;
    create_hard_link: crate::types::CreateHardLinkWFn = File "CreateHardLinkW" => crate::win32::create_hard_link;
    create_symbolic_link: crate::types::CreateSymbolicLinkWFn = File "CreateSymbolicLinkW" => crate::win32::create_symbolic_link;
    find_first: crate::types::FindFirstFileWFn = File "FindFirstFileW" => crate::search::find_first;
    find_first_ex: crate::types::FindFirstFileExWFn = File "FindFirstFileExW" => crate::search::find_first_ex;
    find_next: crate::types::FindNextFileWFn = File "FindNextFileW" => crate::search::find_next;
    find_close: crate::types::FindCloseFn = File "FindClose" => crate::search::find_close;
    nt_create_file: crate::types::NtCreateFileFn = Nt "NtCreateFile" => crate::nt::nt_create_file;
    nt_open_file: crate::types::NtOpenFileFn = Nt "NtOpenFile" => crate::nt::nt_open_file;
    nt_query_attributes_file: crate::types::NtQueryAttributesFileFn = Nt "NtQueryAttributesFile" => crate::nt::nt_query_attributes_file;
    nt_query_full_attributes_file: crate::types::NtQueryAttributesFileFn = Nt "NtQueryFullAttributesFile" => crate::nt::nt_query_full_attributes_file;
    nt_set_information_file: crate::types::NtSetInformationFileFn = Nt "NtSetInformationFile" => crate::nt::nt_set_information_file;
    nt_query_directory_file: crate::types::NtQueryDirectoryFileFn = Nt "NtQueryDirectoryFile" => crate::nt::nt_query_directory_file;
    nt_query_directory_file_ex: crate::types::NtQueryDirectoryFileExFn = Nt "NtQueryDirectoryFileEx" => crate::nt::nt_query_directory_file_ex;
    create_process: crate::types::CreateProcessWFn = File "CreateProcessInternalW" => crate::process::create_process;
    create_process_w: crate::types::CreateProcessPlainFn = File "CreateProcessW" => crate::process::create_process_w;
    create_process_as_user: crate::types::CreateProcessAsUserWFn = Advapi "CreateProcessAsUserW" => crate::process::create_process_as_user;
    get_module_file_name: crate::types::GetModuleFileNameWFn = File "GetModuleFileNameW" => crate::win32::get_module_file_name;
    get_module_file_name_a: crate::types::GetModuleFileNameAFn = File "GetModuleFileNameA" => crate::win32::get_module_file_name_a;
    get_module_file_name_ex: crate::types::GetModuleFileNameExWFn = File "GetModuleFileNameExW" => crate::win32::get_module_file_name_ex;
    get_module_file_name_ex_a: crate::types::GetModuleFileNameExAFn = File "GetModuleFileNameExA" => crate::win32::get_module_file_name_ex_a;
    get_final_path: crate::types::GetFinalPathNameByHandleWFn = File "GetFinalPathNameByHandleW" => crate::handles::final_path;
    get_mapped_file_name: crate::types::GetMappedFileNameWFn = File "GetMappedFileNameW" => crate::handles::mapped_file_name;
    get_file_information: crate::types::GetFileInformationByHandleFn = File "GetFileInformationByHandle" => crate::file_info::get_file_information;
    nt_query_information_file: crate::types::NtQueryInformationFileFn = Nt "NtQueryInformationFile" => crate::file_info::nt_query_information_file;
}

/// The detours of the file API, in `module`, and of the NT layer in `ntdll`. `KernelBase` exports the file API from
/// Windows 8 on, and `kernel32` only forwards to it. A program that imports from `KernelBase` skips `kernel32`, so the
/// hooks go where every call ends up.
///
/// # Safety
/// Called once, while the process is starting, with the Windows file API loaded.
pub(crate) unsafe fn create_hooks() -> (Hooks, Vec<HookStatus>) {
    let file = if unsafe { GetModuleHandleW(wide(OsStr::new("KernelBase.dll")).as_ptr()) }.is_null()
    {
        "kernel32.dll"
    } else {
        "KernelBase.dll"
    };
    log(&format!("the file API is in {file}"));

    unsafe { Hooks::create(file) }
}
