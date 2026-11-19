//! The 32-bit side of the overlay minidumper. Gobbo starts it before a game whose hook is 32-bit and kills it after the
//! game exits. The minidumper protocol carries pointer-sized fields, so a 32-bit hook cannot use the x64 server.
//!
//! Usage: `gobbo_overlay_dump32 <state folder> <socket> <ready file>`.

#[cfg(windows)]
fn main() {
    let mut args = std::env::args().skip(1);
    let Some(state) = args.next() else {
        eprintln!("usage: gobbo_overlay_dump32 <state> <socket> <ready>");
        std::process::exit(2);
    };
    let Some(socket) = args.next() else {
        eprintln!("usage: gobbo_overlay_dump32 <state> <socket> <ready>");
        std::process::exit(2);
    };
    let Some(ready) = args.next() else {
        eprintln!("usage: gobbo_overlay_dump32 <state> <socket> <ready>");
        std::process::exit(2);
    };
    if let Err(error) = gobbo_overlay_dump::serve(
        std::path::Path::new(&state),
        std::path::Path::new(&socket),
        std::path::Path::new(&ready),
    ) {
        eprintln!("gobbo_overlay_dump32: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("gobbo_overlay_dump32 needs Windows.");
    std::process::exit(2);
}
