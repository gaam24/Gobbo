//! The 32-bit half of the overlay launcher. A 64-bit process cannot set the context of a 32-bit thread, so the 64-bit
//! launcher starts this helper under `SysWOW64`, and the helper points the suspended thread at the x86 stub. The helper
//! only sets the context; the caller resumes the thread and waits for the hook's readiness event.
//!
//! Usage: `gobbo_overlay_inject32 <pid> <tid> <hook32.dll>`.

#[cfg(windows)]
fn main() {
    use std::path::PathBuf;

    let mut args = std::env::args_os().skip(1);
    let Some(pid) = args
        .next()
        .and_then(|value| value.to_str().and_then(|text| text.parse::<u32>().ok()))
    else {
        eprintln!("usage: gobbo_overlay_inject32 <pid> <tid> <hook32.dll>");
        std::process::exit(2);
    };
    let Some(tid) = args
        .next()
        .and_then(|value| value.to_str().and_then(|text| text.parse::<u32>().ok()))
    else {
        eprintln!("usage: gobbo_overlay_inject32 <pid> <tid> <hook32.dll>");
        std::process::exit(2);
    };
    let Some(hook) = args.next().map(PathBuf::from) else {
        eprintln!("usage: gobbo_overlay_inject32 <pid> <tid> <hook32.dll>");
        std::process::exit(2);
    };

    if let Err(error) = gobbo_overlay_launch::inject_early_by_tid(pid, tid, &hook) {
        eprintln!("gobbo: could not set the 32-bit hook stub in {pid}: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {}
