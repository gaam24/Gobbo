//! The worker: the same file operations in a plain process, a pass-through hooked process, or an active hooked process.

use std::alloc::{GlobalAlloc, Layout, System};
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use gobbo_overlay_core::wide;
use hdrhistogram::Histogram;

use std::os::windows::io::AsRawHandle;

use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFinalPathNameByHandleW, OPEN_EXISTING,
};
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleW, GetProcAddress,
};

use super::{HOOK_BUILD_COMMAND, MAX_COPY_UP, MAX_LIST, MAX_WHITEOUT, Mode, Row, layout};

/// Every allocation of the worker process since the last reset.
static WORKER_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

/// Counts the worker's own allocations, so a benchmark row shows both sides of the hook.
struct CountingAllocator;

// SAFETY: every function forwards the exact pointer and layout to `System`; the counter does not change the pointer.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        WORKER_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller passes a valid layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout came from this allocator.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        WORKER_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the pointer and layout came from this allocator and `new_size` is valid.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// The counters exported by a hook built with the `bench-alloc` and `bench-stats` features.
#[derive(Clone, Copy)]
struct HookCounters {
    allocations: unsafe extern "C" fn() -> u64,
    bytes: Option<unsafe extern "C" fn() -> u64>,
    reset: unsafe extern "C" fn(),
    spelling: unsafe extern "C" fn() -> u64,
    probes: unsafe extern "C" fn() -> u64,
    locks: unsafe extern "C" fn() -> u64,
    io: unsafe extern "C" fn() -> u64,
    reset_stats: unsafe extern "C" fn(),
}

/// The error for a hook that is loaded but cannot report the counters the budget checks read.
fn missing_counter(name: &str) -> Box<dyn Error> {
    format!("the loaded hook is missing {name}; build it with `{HOOK_BUILD_COMMAND}`").into()
}

impl HookCounters {
    /// Finds the counters in the loaded hook DLL. `None` when no hook is loaded (the baseline); fails hard when a hook
    /// is loaded without the benchmark features, so a zeroed counter cannot pass a budget check.
    fn find() -> Result<Option<Self>, Box<dyn Error>> {
        let name: Vec<u16> = "gobbo_overlay_hook.dll\0".encode_utf16().collect();
        // SAFETY: `name` is a NUL-terminated wide string.
        let module = unsafe { GetModuleHandleW(name.as_ptr()) };

        if module.is_null() {
            return Ok(None);
        }

        // SAFETY: the addresses are looked up by their stable exported names in the loaded DLL.
        let allocations = unsafe { export(module, b"gobbo_overlay_bench_allocations\0") }
            .ok_or_else(|| missing_counter("gobbo_overlay_bench_allocations"))?;
        // SAFETY: the byte counter is optional; an older hook leaves it as `None`.
        let bytes = unsafe { export(module, b"gobbo_overlay_bench_alloc_bytes\0") };
        // SAFETY: as `allocations`.
        let reset = unsafe { export(module, b"gobbo_overlay_bench_reset\0") }
            .ok_or_else(|| missing_counter("gobbo_overlay_bench_reset"))?;
        // SAFETY: as `allocations`.
        let spelling = unsafe { export(module, b"gobbo_overlay_bench_spelling\0") }
            .ok_or_else(|| missing_counter("gobbo_overlay_bench_spelling"))?;
        // SAFETY: as `allocations`.
        let probes = unsafe { export(module, b"gobbo_overlay_bench_probes\0") }
            .ok_or_else(|| missing_counter("gobbo_overlay_bench_probes"))?;
        // SAFETY: as `allocations`.
        let locks = unsafe { export(module, b"gobbo_overlay_bench_locks\0") }
            .ok_or_else(|| missing_counter("gobbo_overlay_bench_locks"))?;
        // SAFETY: as `allocations`.
        let io = unsafe { export(module, b"gobbo_overlay_bench_io\0") }
            .ok_or_else(|| missing_counter("gobbo_overlay_bench_io"))?;
        // SAFETY: as `allocations`.
        let reset_stats = unsafe { export(module, b"gobbo_overlay_bench_reset_stats\0") }
            .ok_or_else(|| missing_counter("gobbo_overlay_bench_reset_stats"))?;

        Ok(Some(Self {
            allocations,
            bytes,
            reset,
            spelling,
            probes,
            locks,
            io,
            reset_stats,
        }))
    }
}

/// Looks up an exported C function by name. The returned type is chosen by the caller.
///
/// # Safety
/// `module` must be a loaded module, and `name` a NUL-terminated function name exported by it. The target type must be
/// an `extern "C"` function with the correct signature.
unsafe fn export<T: Copy>(module: *mut std::ffi::c_void, name: &[u8]) -> Option<T> {
    // SAFETY: the caller guarantees `module` and the name.
    let function = unsafe { GetProcAddress(module, name.as_ptr()) }?;
    // SAFETY: the caller chose `T` to match the exported function; both are function pointers.
    Some(unsafe { std::mem::transmute_copy(&function) })
}

/// Counts the entries of `folder` for a listing measurement. A changed or unreadable fixture is reported as an error,
/// never as a panic inside the timed operation.
fn count_directory(folder: &Path, expected: usize) -> Result<(), Box<dyn Error>> {
    let entries = fs::read_dir(folder)
        .map_err(|error| format!("could not list {}: {error}", folder.display()))?;
    let count = entries.count();
    if count != expected {
        return Err(format!(
            "the {} fixture changed: expected {expected} entries, got {count}",
            folder.display()
        )
        .into());
    }
    Ok(())
}

/// Runs every operation and writes the result file.
pub(super) fn run(
    root: &Path,
    iterations: u64,
    warmup: u64,
    result: &Path,
    mode: Mode,
) -> Result<(), Box<dyn Error>> {
    /// Closes a raw directory handle when it goes out of scope, including on an early error.
    struct HandleGuard(*mut std::ffi::c_void);

    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                // SAFETY: the handle is owned by this guard and no other owner closes it.
                unsafe { CloseHandle(self.0) };
            }
        }
    }

    let hook = if mode == Mode::Baseline {
        None
    } else {
        match HookCounters::find()? {
            Some(hook) => Some(hook),
            None => {
                return Err(
                    "the hook DLL is not loaded in a hooked worker; check the launch report".into(),
                );
            }
        }
    };

    let game_folder = root.join(layout::GAME);
    let outside_folder = root.join(layout::OUTSIDE);
    let (input, expected_list, game_nt_row) = match mode {
        Mode::Baseline => (game_folder.clone(), 64, true),
        Mode::PassThrough => (outside_folder.clone(), 64, false),
        Mode::Active => (game_folder.clone(), 80, true),
    };

    // Make the relative-path operation resolve against the mode's input folder, not the benchmark's own folder.
    std::env::set_current_dir(&input).map_err(|error| {
        format!(
            "the benchmark could not change to {}: {error}",
            input.display()
        )
    })?;
    let read = input.join(layout::READ).join("a.bin");
    let write_path = input.join(layout::WRITE).join("w.bin");
    let missing = input.join("missing").join("a.bin");
    let list = input.join(layout::LIST);
    let lower: Vec<PathBuf> = (0..iterations.min(MAX_COPY_UP))
        .map(|number| input.join(layout::LOWER).join(format!("l{number}.bin")))
        .collect();
    let deletes: Vec<PathBuf> = (0..iterations.min(MAX_WHITEOUT))
        .map(|number| input.join(layout::DELETE).join(format!("d{number}.bin")))
        .collect();
    let buffer = [b'x'; 4096];

    let mut out = BufWriter::new(File::create(result)?);
    write_header(&mut out)?;

    let row = measure("open_read", iterations, warmup, hook, |_| {
        let _ = File::open(&read);
        Ok(())
    })?;
    row.write_csv(&mut out)?;

    let row = measure("open_read_relative", iterations, warmup, hook, |_| {
        let _ = File::open("read/a.bin");
        Ok(())
    })?;
    row.write_csv(&mut out)?;

    // A file that only the game folder has: the overlay read stays in the root layer and must not re-encode its name.
    let root_read = input.join(layout::WRITE).join("w.bin");
    let row = measure("open_read_root", iterations, warmup, hook, |_| {
        let _ = File::open(&root_read);
        Ok(())
    })?;
    row.write_csv(&mut out)?;

    // A handle path of a file in this mode's tree. The hook answers it from the layer mapping (active) or passes the
    // program's path through (outside); neither should allocate a `String` or call Windows twice.
    if let Ok(file) = File::open(&read) {
        let handle = file.as_raw_handle();
        let mut path = [0u16; 520];
        let row = measure("get_final_path", iterations, warmup, hook, |_| {
            // SAFETY: `handle` is the open file, and `path` is a writable buffer of `path.len()` units.
            let _ = unsafe {
                GetFinalPathNameByHandleW(handle, path.as_mut_ptr(), path.len() as u32, 0)
            };
            Ok(())
        })?;
        row.write_csv(&mut out)?;
    }

    // A handle outside every mount: the hook must write the program's path back without a `String`, a second system
    // call, or an allocation (the pass-through budget).
    if let Ok(file) = File::open(outside_folder.join(layout::READ).join("a.bin")) {
        let handle = file.as_raw_handle();
        let mut path = [0u16; 520];
        let row = measure("get_final_path_outside", iterations, warmup, hook, |_| {
            // SAFETY: `handle` is the open file, and `path` is a writable buffer of `path.len()` units.
            let _ = unsafe {
                GetFinalPathNameByHandleW(handle, path.as_mut_ptr(), path.len() as u32, 0)
            };
            Ok(())
        })?;
        row.write_csv(&mut out)?;
    }

    // The current program's module path: outside every mount, so it is the no-allocation path of the module hook.
    let mut module = [0u16; 1024];
    let row = measure("get_module_file_name", iterations, warmup, hook, |_| {
        // SAFETY: a null module asks for this program, and `module` is writable for `module.len()` units.
        let _ = unsafe {
            GetModuleFileNameW(
                std::ptr::null_mut(),
                module.as_mut_ptr(),
                module.len() as u32,
            )
        };
        Ok(())
    })?;
    row.write_csv(&mut out)?;

    // NtCreateFile with a directory handle as the root: the name is joined onto the handle's path. The handle to a folder
    // outside every mount must not allocate in the hook; the handle to the game folder is redirected (active).
    if let Some(create) = nt_create_file() {
        let mut rows = vec![("open_nt_relative_outside", &outside_folder)];

        if game_nt_row {
            rows.push(("open_nt_relative_game", &game_folder));
        }

        for (operation, folder) in rows {
            let root = open_folder(folder);

            if root.is_null() || root == INVALID_HANDLE_VALUE {
                return Err(format!("the benchmark could not open {}", folder.display()).into());
            }
            let _root = HandleGuard(root);

            let name: Vec<u16> = r"read\a.bin".encode_utf16().collect();
            let row = measure(operation, iterations, warmup, hook, |_| {
                let _ = nt_open_relative(create, root, &name);
                Ok(())
            })?;
            row.write_csv(&mut out)?;
        }
    }

    let row = measure("metadata", iterations, warmup, hook, |_| {
        let _ = fs::metadata(&read);
        Ok(())
    })?;
    row.write_csv(&mut out)?;

    let row = measure("missing", iterations, warmup, hook, |_| {
        let _ = fs::metadata(&missing);
        Ok(())
    })?;
    row.write_csv(&mut out)?;

    let row = measure("open_write", iterations, warmup, hook, |_| {
        let _ = OpenOptions::new().write(true).open(&write_path);
        Ok(())
    })?;
    row.write_csv(&mut out)?;

    let row = measure("write_4k", iterations, warmup, hook, |_| {
        if let Ok(mut file) = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&write_path)
        {
            let _ = file.write_all(&buffer);
        }
        Ok(())
    })?;
    row.write_csv(&mut out)?;

    let row = measure("copy_up", lower.len() as u64, 0, hook, |index| {
        if let Some(path) = lower.get(index as usize) {
            let _ = OpenOptions::new().append(true).open(path);
        }
        Ok(())
    })?;
    row.write_csv(&mut out)?;

    let row = measure("whiteout", deletes.len() as u64, 0, hook, |index| {
        if let Some(path) = deletes.get(index as usize) {
            let _ = fs::remove_file(path);
        }
        Ok(())
    })?;
    row.write_csv(&mut out)?;

    // The active game folder merges 64 game files and 16 mod files; the baseline process and the pass-through folder
    // see only the 64 game files.
    let row = measure("list", iterations.min(MAX_LIST), warmup, hook, |_| {
        count_directory(&list, expected_list)
    })?;
    row.write_csv(&mut out)?;

    // A folder of 1000 files that only the game folder has (or only the folder outside it): the listing is merged
    // from the layers, and the root layer is read entry by entry.
    let big = input.join(layout::BIG);
    let row = measure("list_1000", iterations.min(MAX_LIST), warmup, hook, |_| {
        count_directory(&big, 1000)
    })?;
    row.write_csv(&mut out)?;

    // The same 1000 game names, with 200 of them overridden by the mod layer. Every override asks the lower layers for
    // the spelling the game keeps, which is the cost the spelling rule adds to a modded listing.
    let modded = input.join(layout::BIG_MODDED);
    let row = measure(
        "list_modded",
        iterations.min(MAX_LIST),
        warmup,
        hook,
        |_| count_directory(&modded, 1000),
    )?;
    row.write_csv(&mut out)?;

    // The same 1000 game names with only ten overrides: exact lower-layer searches are cheaper than one root pass
    // when the root folder is much larger than the number of modded entries.
    let modded_few = input.join(layout::BIG_MODDED_FEW);
    let row = measure(
        "list_modded_few",
        iterations.min(MAX_LIST),
        warmup,
        hook,
        |_| count_directory(&modded_few, 1000),
    )?;
    row.write_csv(&mut out)?;

    // Polish names exercise the fold rule without a `String` on the common path: a prefix classification, a whiteout
    // check, and a duplicate check in the listing.
    let nonascii = input.join(layout::NONASCII);
    let row = measure("metadata_nonascii", iterations, warmup, hook, |_| {
        let _ = fs::metadata(nonascii.join("ąć.bin"));
        Ok(())
    })?;
    row.write_csv(&mut out)?;
    let row = measure(
        "list_nonascii",
        iterations.min(MAX_LIST),
        warmup,
        hook,
        |_| count_directory(&nonascii, 65),
    )?;
    row.write_csv(&mut out)?;

    // The NT directory query reads a folder through `NtQueryDirectoryFile`, which the hook merges from the layers and
    // streams in batches instead of materializing the whole folder.
    if let Some(query) = nt_query_directory_file() {
        let folder = open_directory(&big)?;
        let _folder = HandleGuard(folder);
        let mut status_block = [0usize; 2];
        let mut entries = vec![0u8; 4096];
        let row = measure(
            "list_nt_1000",
            iterations.min(MAX_LIST),
            warmup,
            hook,
            |_| {
                let count = nt_count(query, folder, &mut entries, &mut status_block);
                if count != 1000 {
                    return Err(format!(
                        "the NT listing fixture changed: expected 1000 entries, got {count}"
                    )
                    .into());
                }
                Ok(())
            },
        )?;
        row.write_csv(&mut out)?;
        // The same folder with 200 overrides: the merged snapshot, not the plain handle.
        let modded_handle = open_directory(&modded)?;
        let _modded = HandleGuard(modded_handle);
        let row = measure(
            "list_nt_modded",
            iterations.min(MAX_LIST),
            warmup,
            hook,
            |_| {
                let count = nt_count(query, modded_handle, &mut entries, &mut status_block);
                if count != 1000 {
                    return Err(format!(
                        "the modded NT listing fixture changed: expected 1000 entries, got {count}"
                    )
                    .into());
                }
                Ok(())
            },
        )?;
        row.write_csv(&mut out)?;
    }

    out.flush()?;
    Ok(())
}

/// Measures `name` `iterations` times, with the worker and hook allocation counters reset around each call. It returns
/// the one schema row the caller writes, so the measured values never pass through a second set of fields.
/// An operation that reports an error stops the worker with that error instead of unwinding through the measurement.
fn measure(
    name: &str,
    iterations: u64,
    warmup: u64,
    hook: Option<HookCounters>,
    mut operation: impl FnMut(u64) -> Result<(), Box<dyn Error>>,
) -> Result<Row, Box<dyn Error>> {
    let mut histogram = Histogram::<u64>::new(3).map_err(|error| error.to_string())?;
    let mut worker_total = 0u64;
    let mut hook_total = 0u64;
    let mut bytes_total = 0u64;
    let mut spelling_total = 0u64;
    let mut probes_total = 0u64;
    let mut locks_total = 0u64;
    let mut io_total = 0u64;

    for _ in 0..warmup {
        operation(0)?;
    }

    for index in 0..iterations {
        WORKER_ALLOCATIONS.store(0, Ordering::Relaxed);

        if let Some(hook) = hook {
            // SAFETY: the exported resets take no arguments.
            unsafe {
                (hook.reset)();
                (hook.reset_stats)();
            }
        }

        let start = Instant::now();
        operation(index)?;
        let elapsed = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
        worker_total = worker_total.saturating_add(WORKER_ALLOCATIONS.load(Ordering::Relaxed));

        if let Some(hook) = hook {
            // SAFETY: the exported counters take no arguments.
            hook_total = hook_total.saturating_add(unsafe { (hook.allocations)() });

            if let Some(read) = hook.bytes {
                // SAFETY: the exported counter takes no arguments.
                bytes_total = bytes_total.saturating_add(unsafe { read() });
            }

            // SAFETY: the exported counters take no arguments.
            spelling_total = spelling_total.saturating_add(unsafe { (hook.spelling)() });
            // SAFETY: as `spelling`.
            probes_total = probes_total.saturating_add(unsafe { (hook.probes)() });
            // SAFETY: as `spelling`.
            locks_total = locks_total.saturating_add(unsafe { (hook.locks)() });
            // SAFETY: as `spelling`.
            io_total = io_total.saturating_add(unsafe { (hook.io)() });
        }

        histogram
            .record(elapsed.max(1))
            .map_err(|error| error.to_string())?;
    }

    let count = iterations.max(1) as f64;
    Ok(Row {
        operation: name.to_string(),
        p50: histogram.value_at_quantile(0.50),
        p95: histogram.value_at_quantile(0.95),
        p99: histogram.value_at_quantile(0.99),
        mean: histogram.mean() as u64,
        worker_allocs: worker_total as f64 / count,
        hook_allocs: hook_total as f64 / count,
        hook_bytes: bytes_total as f64 / count,
        spelling: spelling_total as f64 / count,
        probes: probes_total as f64 / count,
        locks: locks_total as f64 / count,
        io: io_total as f64 / count,
    })
}

/// Writes the CSV header from the one shared column list.
fn write_header(out: &mut impl Write) -> std::io::Result<()> {
    writeln!(out, "{}", Row::columns().join(","))
}

/// The `UNICODE_STRING` and `OBJECT_ATTRIBUTES` of an NT open. The pointers are plain pointers, so both bitnesses lay the
/// structures out as the system does.
#[repr(C)]
struct NtUnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

#[repr(C)]
struct NtObjectAttributes {
    length: u32,
    root_directory: *mut std::ffi::c_void,
    object_name: *const NtUnicodeString,
    attributes: u32,
    security_descriptor: *const std::ffi::c_void,
    security_quality_of_service: *const std::ffi::c_void,
}

/// `NtCreateFile`, as ntdll exports it.
type NtCreateFileFn = unsafe extern "system" fn(
    *mut *mut std::ffi::c_void,
    u32,
    *const NtObjectAttributes,
    *mut [usize; 2],
    *const i64,
    u32,
    u32,
    u32,
    u32,
    *const std::ffi::c_void,
    u32,
) -> i32;

/// The `NtCreateFile` of this process's ntdll, when it is there.
fn nt_create_file() -> Option<NtCreateFileFn> {
    let name: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
    // SAFETY: `name` is NUL-terminated and ntdll is loaded in every process.
    let module = unsafe { GetModuleHandleW(name.as_ptr()) };

    if module.is_null() {
        return None;
    }
    // SAFETY: the export name is NUL-terminated, and the type matches the Windows declaration of `NtCreateFile`.
    unsafe { export::<NtCreateFileFn>(module.cast(), b"NtCreateFile\0") }
}

/// `NtQueryDirectoryFile`, as ntdll exports it.
type NtQueryDirectoryFileFn = unsafe extern "system" fn(
    *mut std::ffi::c_void,
    *mut std::ffi::c_void,
    *mut std::ffi::c_void,
    *mut std::ffi::c_void,
    *mut [usize; 2],
    *mut std::ffi::c_void,
    u32,
    u32,
    u8,
    *const NtUnicodeString,
    u8,
) -> i32;

/// The `NtQueryDirectoryFile` of this process's ntdll, when it is there.
fn nt_query_directory_file() -> Option<NtQueryDirectoryFileFn> {
    let name: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
    // SAFETY: `name` is NUL-terminated and ntdll is loaded in every process.
    let module = unsafe { GetModuleHandleW(name.as_ptr()) };

    if module.is_null() {
        return None;
    }
    // SAFETY: the export name is NUL-terminated, and the type matches the Windows declaration of the call.
    unsafe { export::<NtQueryDirectoryFileFn>(module.cast(), b"NtQueryDirectoryFile\0") }
}

/// Counts the entries of one NT directory, restarting at the beginning. `.` and `..` are not counted: a plain NT
/// folder does not return them, while a merged snapshot adds them.
fn nt_count(
    query: NtQueryDirectoryFileFn,
    folder: *mut std::ffi::c_void,
    entries: &mut [u8],
    status_block: &mut [usize; 2],
) -> usize {
    let mut restart = 1u8;
    let mut total = 0usize;

    loop {
        // SAFETY: the handle is open for the measurement, the buffer is writable for its length, and every other
        // pointer is null, which the NT call accepts for a synchronous query.
        let nt_status = unsafe {
            query(
                folder,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                status_block,
                entries.as_mut_ptr().cast(),
                entries.len() as u32,
                FILE_NAMES_INFORMATION,
                0,
                std::ptr::null(),
                restart,
            )
        };
        restart = 0;

        if nt_status < 0 {
            break;
        }

        let mut offset = 0usize;

        loop {
            if offset > entries.len().saturating_sub(12) {
                break;
            }

            let next = u32::from_ne_bytes(entries[offset..offset + 4].try_into().unwrap_or([0; 4]))
                as usize;
            let name_length = u32::from_ne_bytes(
                entries[offset + 8..offset + 12]
                    .try_into()
                    .unwrap_or([0; 4]),
            ) as usize;
            let start = offset + 12;

            if start > entries.len().saturating_sub(name_length) {
                break;
            }

            let name = &entries[start..start + name_length];

            if name != [46, 0] && name != [46, 0, 46, 0] {
                total += 1;
            }

            if next == 0 {
                break;
            }

            offset = offset.saturating_add(next);
        }
    }

    total
}

/// Opens a folder for a directory query. The handle needs `FILE_LIST_DIRECTORY`, unlike `open_folder`'s attribute-only
/// handle.
fn open_directory(folder: &Path) -> Result<*mut std::ffi::c_void, Box<dyn Error>> {
    let name = wide(folder.as_os_str());
    // SAFETY: `name` is NUL-terminated and the call only opens the folder for reading its entries.
    let handle = unsafe {
        CreateFileW(
            name.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };

    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return Err(format!("the benchmark could not open {}", folder.display()).into());
    }

    Ok(handle)
}

/// `FILE_LIST_DIRECTORY`: the access a directory query needs.
const FILE_LIST_DIRECTORY: u32 = 0x1;
/// `FileNamesInformation`: the smallest directory entry class, enough for a listing benchmark.
const FILE_NAMES_INFORMATION: u32 = 12;

/// Opens a folder as a directory handle, for a relative open. `None` is returned as null; the benchmark stops on it.
fn open_folder(folder: &Path) -> *mut std::ffi::c_void {
    let name = wide(folder.as_os_str());
    // SAFETY: `name` is NUL-terminated and the call only opens the folder for reading its attributes.
    unsafe {
        CreateFileW(
            name.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    }
}

/// Opens `name` relative to the directory handle `root` with `NtCreateFile`, and closes the file again. Returns the status.
fn nt_open_relative(create: NtCreateFileFn, root: *mut std::ffi::c_void, name: &[u16]) -> i32 {
    let unicode = NtUnicodeString {
        length: (name.len() * 2) as u16,
        maximum_length: (name.len() * 2) as u16,
        buffer: name.as_ptr(),
    };
    let attributes = NtObjectAttributes {
        length: std::mem::size_of::<NtObjectAttributes>() as u32,
        root_directory: root,
        object_name: &raw const unicode,
        attributes: 0,
        security_descriptor: std::ptr::null(),
        security_quality_of_service: std::ptr::null(),
    };
    let mut handle = std::ptr::null_mut();
    let mut status_block = [0usize; 2];
    // SAFETY: every pointer is valid for the call: `unicode` and `name` live until it returns, and the status block is
    // writable. The access and options are the read-only file open of a benchmark.
    let status = unsafe {
        create(
            &raw mut handle,
            FILE_GENERIC_READ,
            &raw const attributes,
            &raw mut status_block,
            std::ptr::null(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            NT_FILE_OPEN,
            NT_FILE_NON_DIRECTORY_FILE | NT_FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };

    if status >= 0 && !handle.is_null() {
        // SAFETY: the handle came from the successful open above and is closed once.
        unsafe { CloseHandle(handle) };
    }

    status
}

/// `FILE_OPEN`: open an existing file (the NT disposition).
const NT_FILE_OPEN: u32 = 1;
/// `FILE_NON_DIRECTORY_FILE`: the open fails on a folder (the NT create options).
const NT_FILE_NON_DIRECTORY_FILE: u32 = 0x40;
/// `FILE_SYNCHRONOUS_IO_NONALERT`: the I/O is synchronous (the NT create options).
const NT_FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x20;

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row() -> Row {
        Row {
            operation: "open_read".to_string(),
            p50: 1_250,
            p95: 2_500,
            p99: 3_750,
            mean: 2_000,
            worker_allocs: 1.25,
            hook_allocs: 2.5,
            hook_bytes: 4.75,
            spelling: 1.0,
            probes: 3.25,
            locks: 2.0,
            io: 0.5,
        }
    }

    #[test]
    fn a_written_row_parses_back_to_the_same_stats() {
        let mut out = Vec::new();
        let row = sample_row();
        row.write_csv(&mut out).unwrap();
        let line = String::from_utf8(out).unwrap();

        assert_eq!(Row::from_csv(line.trim_end()).unwrap(), row);
    }

    #[test]
    fn a_row_with_the_wrong_field_count_is_refused() {
        let mut out = Vec::new();
        sample_row().write_csv(&mut out).unwrap();
        let line = String::from_utf8(out).unwrap();
        let short = line.trim_end().rsplit_once(',').unwrap().0;

        let error = Row::from_csv(short).unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&Row::columns().len().to_string())
        );
        assert!(error.to_string().contains(short));
    }

    #[test]
    fn mode_names_parse_into_the_worker_modes() {
        assert_eq!(Mode::parse("baseline").unwrap(), Mode::Baseline);
        assert_eq!(Mode::parse("pass-through").unwrap(), Mode::PassThrough);
        assert_eq!(Mode::parse("active").unwrap(), Mode::Active);
        assert!(Mode::parse("other").is_err());
    }

    #[test]
    fn a_changed_listing_fixture_is_an_error_not_a_panic() {
        let folder =
            std::env::temp_dir().join(format!("overlay-bench-listing-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("a.bin"), b"x").unwrap();

        let error = count_directory(&folder, 2).unwrap_err();
        let _ = fs::remove_dir_all(&folder);

        assert!(error.to_string().contains("expected 2 entries, got 1"));
    }

    #[test]
    fn measure_propagates_a_warmup_error() {
        let result = measure("test", 1, 2, None, |_| Err("warmup failed".into()));
        let Err(error) = result else {
            panic!("a failing warm-up must fail the measurement");
        };
        assert!(error.to_string().contains("warmup failed"));
    }

    #[test]
    fn measure_propagates_a_measured_operation_error() {
        let calls = std::cell::Cell::new(0);
        let result = measure("test", 3, 0, None, |_| {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                Err("operation failed".into())
            } else {
                Ok(())
            }
        });
        let Err(error) = result else {
            panic!("a failing operation must fail the measurement");
        };

        assert!(error.to_string().contains("operation failed"));
        assert_eq!(calls.get(), 2);
    }
}
