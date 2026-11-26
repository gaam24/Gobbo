//! The 32-bit side of the overlay minidumper. Gobbo starts it before a game whose hook is 32-bit and kills it after the
//! game exits. The minidumper protocol carries pointer-sized fields, so a 32-bit hook cannot use the x64 server.
//!
//! Usage: `gobbo_overlay_dump32 <state folder> <socket> <ready file>`.

use std::ffi::OsString;

/// The three paths the helper needs, in order. Extra arguments are ignored, as they were by the original CLI.
#[cfg(any(windows, test))]
fn arguments(mut args: impl Iterator<Item = OsString>) -> Option<(OsString, OsString, OsString)> {
    Some((args.next()?, args.next()?, args.next()?))
}

#[cfg(windows)]
fn main() {
    use gobbo_overlay_dump::{READY_FILE, serve};

    let usage = || eprintln!("usage: gobbo_overlay_dump32 <state> <socket> {READY_FILE}");
    // `args_os`, not `args`: a state or socket path with a non-Unicode name must reach the server unchanged.
    let Some((state, socket, ready)) = arguments(std::env::args_os().skip(1)) else {
        usage();
        std::process::exit(2);
    };

    if let Err(error) = serve(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_paths_are_parsed_and_extras_are_ignored() {
        let args = [
            OsString::from("state"),
            OsString::from("socket"),
            OsString::from("ready"),
            OsString::from("extra"),
        ]
        .into_iter();
        assert_eq!(
            arguments(args),
            Some((
                OsString::from("state"),
                OsString::from("socket"),
                OsString::from("ready")
            ))
        );
        assert_eq!(arguments(std::iter::once(OsString::from("state"))), None);
    }

    #[cfg(windows)]
    #[test]
    fn a_path_with_an_unpaired_surrogate_is_kept() {
        use std::os::windows::ffi::OsStringExt;

        let odd = OsString::from_wide(&[0xD800]);
        let args = [
            odd.clone(),
            OsString::from("socket"),
            OsString::from("ready"),
        ]
        .into_iter();
        assert_eq!(
            arguments(args).map(|(state, _, _)| state),
            Some(odd),
            "a non-Unicode path must not be lost or cause a panic"
        );
    }
}
