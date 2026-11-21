# Platforms

Gobbo runs on Windows and Linux. macOS is not supported. Windows x64 and x86 are covered; ARM64 Windows is **experimental** — it must keep compiling, but it hasn't been tested on real hardware. For the player-facing guide, see [Linux and Steam](docs/user/linux.md).

[Features](#feature-matrix) · [Architecture](#architecture) · [Requirements](#runtime-requirements) · [Verification](#verification)

Every platform difference has one owner: process and path handling in `src-tauri/src/platform/`, overlay details in `src-tauri/src/deploy/overlay/`, and everything else behind a whole-item `#[cfg]`. A module doesn't need its own `#[cfg(windows)]` when the difference already belongs in one of those two places.

## Feature matrix

Here's what each platform supports today.

| Area                                  | Windows                             | Linux                                                         |
| ------------------------------------- | ----------------------------------- | ------------------------------------------------------------- |
| Deploy                                | Overlay only: hook DLL, x64 and x86 | Overlay only: overlayfs in a user namespace                   |
| User data: isolated redirects         | Yes, through the hook               | Copy fallback, no redirect                                    |
| User data: shared                     | Yes                                 | Yes                                                           |
| Launch: native executable             | Yes                                 | Yes                                                           |
| Launch: Proton                        | No                                  | Yes                                                           |
| Tools: Proton prerequisites           | No                                  | Yes                                                           |
| Process list and stop                 | Toolhelp/Windows APIs               | `/proc` and the overlay session                               |
| Deep links (`nxm://`, future schemes) | Yes, registered by the app          | Yes, registered by the app                                    |
| OS keyring                            | Yes                                 | Yes, the desktop keyring must be running                      |
| System tray                           | Icon, menu and left-click window    | Icon and menu; click events aren't delivered, so use the menu |
| System notifications                  | Yes; click opens Working            | Yes; click support depends on the desktop's notification server |
| First-start storage dialog            | `rfd`                               | GTK on the main thread                                        |
| Mod picture cache                     | Yes                                 | Yes                                                           |
| Overlay probe                         | x64 built in, x86 helper            | Not applicable: no hook DLL                                   |
| Minidumper                            | x64 in process, x86 helper          | Not applicable                                                |

## Architecture

The pieces that actually differ between systems:

| Piece          | Windows                                                  | Linux                        |
| -------------- | -------------------------------------------------------- | ---------------------------- |
| Hook           | `gobbo_overlay_hook.dll`, `gobbo_overlay_hook32.dll`     | None; the crate is empty     |
| Launcher       | `gobbo_overlay_inject.exe`, `gobbo_overlay_inject32.exe` | overlayfs command            |
| Static imports | Rewritten in the suspended process                       | Every file is in the overlay |
| 32-bit game    | hook32 + inject32, required                              | Not applicable               |
| ARM64          | Experimental, must compile                               | Not applicable               |

The game folder is never written by a hooked call. If the hook can't prove a path is safe, it fails closed. Never copy a mod DLL into the game folder as a fallback.

## Runtime requirements

- **Windows:** WebView2, the Visual C++ runtime used by the build, and a working keyring.
- **Linux:** WebKitGTK/GTK, a running keyring (Secret Service), and a kernel with overlayfs and unprivileged user namespaces. Overlay is the only deployment method; without it Gobbo can't deploy mods.
- **Proton:** a Steam installation and a Proton build. A game runs with the Proton build Steam assigns to it by default. The game settings list the Proton builds found on the computer and let the user pick one; the choice is saved per game.

## Verification

Run these from `src-tauri/`:

Linux:

```bash
cargo test --workspace
cargo check -p gobbo --all-targets
cargo test -p gobbo --lib deploy::overlay
```

Windows:

```bash
cargo test --workspace
cargo build -p gobbo-overlay-hook
cargo build -p gobbo-overlay-hook --target i686-pc-windows-msvc
cargo build -p gobbo-overlay-launch --bin gobbo_overlay_inject32 --target i686-pc-windows-msvc
cargo test -p gobbo-overlay-hook
cargo test -p gobbo --lib deploy::overlay::windows
```

ARM64 (compile only):

```bash
cargo check -p gobbo-overlay-hook -p gobbo-overlay-launch --target aarch64-pc-windows-msvc --all-targets
```

A change that touches a platform path must pass the Linux and Windows checks. A feature that exists on only one platform is gated as a whole item, never by a runtime branch inside shared code.

Back to the [documentation map](docs/README.md).
