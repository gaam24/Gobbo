// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// The F6 allocator: the status and plan paths allocate heavily, and mimalloc measured about 16% faster on Linux and
/// about 30% faster on Windows. It is a default feature; `--no-default-features` builds the system-allocator binary
/// for a size comparison.
#[cfg(feature = "mimalloc")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    // `gobbo run <game> -- <program>` works without a window, so Steam can call it from the launch options.
    if let Some(code) = gobbo_lib::run_cli() {
        std::process::exit(code);
    }

    gobbo_lib::run();
}
