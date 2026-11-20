//! Builds the stage 0 DLLs (`stage0/lib.rs`) for the launcher to map into a child: one for x64 and one for x86. The DLLs
//! need `panic=abort`, no C runtime, no imports and link-time optimisation to drop the library code they do not use, which
//! the workspace profile cannot give one crate, so they are compiled here with `rustc` directly and the launcher embeds
//! the files. On another OS, or for another architecture, an empty file keeps the crate buildable; the launcher then
//! refuses to map a stage 0 it does not have.

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// The Windows targets the stage 0 DLL is built for, and the file name each one gets in `OUT_DIR`.
const TARGETS: [(&str, &str, &str); 2] = [
    (
        "x86_64",
        "x86_64-pc-windows-msvc",
        "gobbo_overlay_stage0_x64.dll",
    ),
    (
        "x86",
        "i686-pc-windows-msvc",
        "gobbo_overlay_stage0_x86.dll",
    ),
];

fn main() {
    println!("cargo:rerun-if-changed=stage0");
    println!("cargo:rerun-if-changed=build.rs");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let on_windows = env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let architecture = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let rustc = env::var_os("RUSTC").expect("cargo sets RUSTC");

    for (arch, triple, name) in TARGETS {
        let dll = out.join(name);
        // The launcher's own target decides which DLL it can use. A build for another target gets an empty file.
        if !on_windows || architecture != "x86_64" && architecture != "x86" {
            std::fs::write(&dll, b"").expect("writes the placeholder");
            continue;
        }
        // Both DLLs are built on Windows, whatever the launcher's architecture: each child takes the DLL of its own
        // bitness, and a 64-bit launcher starts 32-bit children too.
        let status = Command::new(&rustc)
            .args([
                "--edition=2024",
                "--crate-type=cdylib",
                "--crate-name=gobbo_overlay_stage0",
                "-C",
                "panic=abort",
                "-C",
                "opt-level=2",
                "-C",
                "lto=fat",
                "-C",
                "codegen-units=1",
                "-C",
                "link-arg=/NOENTRY",
                "-C",
                "link-arg=/NODEFAULTLIB",
                "-C",
                "link-arg=/SAFESEH:NO",
                "--target",
                triple,
                "-o",
            ])
            .arg(&dll)
            .arg("stage0/lib.rs")
            .status()
            .expect("runs rustc for the stage 0 DLL");
        assert!(status.success(), "the stage 0 DLL for {arch} did not build");
    }
}
