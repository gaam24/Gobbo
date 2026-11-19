//! The hook must not load a DLL before the game does, if a mod may provide its own copy of that DLL. This test reads the
//! hook's import table and fails on the names mods replace most often. `dbghelp.dll` is loaded lazily; `ws2_32.dll` is
//! the one early dependency that stays for the crash monitor and is listed as reserved.

use std::fs;
use std::path::PathBuf;

/// The common proxy DLLs a game or a mod puts next to its executable.
const FORBIDDEN: [&str; 9] = [
    "version.dll",
    "d3d11.dll",
    "dxgi.dll",
    "winmm.dll",
    "dinput8.dll",
    "xinput1_3.dll",
    "xinput9_1_0.dll",
    "dsound.dll",
    "dbghelp.dll",
];

#[test]
fn the_hook_does_not_preload_a_common_proxy_dll() {
    let Some(path) = hook_dll() else {
        eprintln!("gobbo_overlay_hook.dll is not built; skipping the import check");
        return;
    };
    let data = fs::read(&path).unwrap();
    let pe = goblin::pe::PE::parse(&data).unwrap();
    let imports: Vec<String> = pe
        .imports
        .iter()
        .map(|import| import.dll.to_ascii_lowercase())
        .collect();
    for name in FORBIDDEN {
        assert!(
            !imports.iter().any(|import| import == name),
            "the hook imports {name}, so a mod's {name} would be shadowed: {imports:?}"
        );
    }
}

/// The hook next to the test executable or one folder above it (`target/debug/deps` -> `target/debug`).
fn hook_dll() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let next = exe.with_file_name("gobbo_overlay_hook.dll");
    if next.is_file() {
        return Some(next);
    }
    let one_up = exe.parent()?.parent()?.join("gobbo_overlay_hook.dll");
    one_up.is_file().then_some(one_up)
}
