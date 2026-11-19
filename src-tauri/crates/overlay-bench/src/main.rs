//! `overlay-bench`: a file-operation benchmark for the Windows overlay hook.
//!
//! The binary runs the same operations three ways: in a plain process (`baseline`), in a hooked process whose files are
//! outside the game folder (`pass-through`), and in a hooked process whose files are inside the game folder (`active`).
//! It reports p50/p95/p99 and the average allocations per operation, from the worker and from the hook DLL.
//!
//! Build the hook with the counting allocator and the stats first, or the hook columns are zero:
//!
//! ```text
//! cargo build -p gobbo-overlay-hook --features "bench-alloc bench-stats"
//! cargo run -p gobbo-overlay-bench -- --iterations 5000
//! ```
//!
//! On a platform other than Windows the binary only prints that it needs Windows.

#[cfg(windows)]
mod bench;

fn main() {
    #[cfg(windows)]
    bench::main();
    #[cfg(not(windows))]
    {
        eprintln!("overlay-bench needs Windows: the overlay hook is a Windows DLL.");
        std::process::exit(2);
    }
}
