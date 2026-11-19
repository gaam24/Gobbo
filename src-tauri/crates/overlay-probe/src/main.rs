//! `overlay-probe`: a matrix of file operations that runs inside an overlay session and reports what the hook did.
//!
//! The program is a separate process, started with the overlay layers in its environment. It writes only under a unique
//! `__gobbo_probe_<pid>` folder, checks the upper and staging copies, and writes a JSON report. A clean report is the
//! "doctor" of a new game: create, read, append, copy-up, whiteout, rename, list, final path and a refused link.
//!
//! It is not loaded as a DLL and has no dependency on Gobbo. Run it through `gobbo run` or set the three `GOBBO_OVERLAY_*`
//! variables yourself.

#[cfg(windows)]
mod probe;

fn main() {
    #[cfg(windows)]
    probe::main();
    #[cfg(not(windows))]
    {
        eprintln!("overlay-probe needs Windows: the overlay hook is a Windows DLL.");
        std::process::exit(2);
    }
}
