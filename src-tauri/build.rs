use std::fs;
use std::path::PathBuf;

fn main() {
    let paths = command_paths();
    let names: Vec<String> = paths
        .iter()
        .map(|path| path.rsplit("::").next().unwrap_or(path).to_string())
        .collect();

    // The names are written out for the tests, which check the capability file against them.
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    fs::write(out.join("app-commands.txt"), names.join("\n"))
        .expect("could not write the command names");

    // `commands::builder` includes this file. The command list lives only in commands.txt: this generated invocation
    // is the single place the Rust macro sees it.
    let invocation = format!(
        "collect_commands![\n{}\n]\n",
        paths
            .iter()
            .map(|path| format!("    {path},"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    fs::write(out.join("collected-commands.rs"), invocation)
        .expect("could not write the command macro invocation");

    // Only the commands the window may call get a permission: `allow-$command` in the capability file.
    let names: &'static [&'static str] = Box::leak(
        names
            .into_iter()
            .map(|name| &*Box::leak(name.into_boxed_str()))
            .collect::<Vec<&'static str>>()
            .into_boxed_slice(),
    );
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(names));
    tauri_build::try_build(attributes).expect("failed to run tauri-build");

    #[cfg(windows)]
    depend_on_common_controls_v6();
}

/// The `alias::name` paths in the canonical command registry. `build.rs` and `scripts/check-generated.ts` read the
/// same lines, so a command cannot be registered in one place and forgotten in the other.
fn command_paths() -> Vec<String> {
    let dir = PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"),
    );
    let registry = dir.join("commands.txt");
    let module = dir.join("src/commands/mod.rs");
    println!("cargo:rerun-if-changed={}", registry.display());
    println!("cargo:rerun-if-changed={}", module.display());
    let text = fs::read_to_string(&registry).expect("could not read commands.txt");
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

/// The test executables do not get the manifest that `tauri_build` embeds in the app. Without it Windows loads
/// the old comctl32 (5.82), which has no `TaskDialogIndirect`, and the test binary stops with
/// `STATUS_ENTRYPOINT_NOT_FOUND` before any test runs. The linker adds this dependency to every manifest it
/// writes, so the version 6 common controls are used.
#[cfg(windows)]
fn depend_on_common_controls_v6() {
    println!(
        "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
    );
}
