# Repository layout

| Path                                  | What it is                                                                                                     |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `README.md`                           | The landing page: what Gobbo is, why it is useful, how to install it and where to go next.                     |
| `docs/`                               | Documentation: player guides in `docs/user/`, architecture, UX, release notes and agent notes.                 |
| `docs/user/`                          | Player guides: install and data, games, Linux, providers, packs and troubleshooting.                           |
| `docs/assets/`                        | Brand images and the screenshots the README embeds.                                                            |
| `src/`                                | React frontend, locales, bindings generated from Rust.                                                         |
| `src-tauri/`                          | Rust backend: Tauri app, commands, modules, domain, infra, games, overlay crates.                              |
| `src-tauri/crates/overlay-core/`      | Pure overlay rules: layers, whiteouts, path normalization, listings. No Windows, no `unsafe`.                  |
| `src-tauri/crates/overlay-decisions/` | The file-backed ring of overlay decisions, shared by the hook (writer) and Gobbo (reader).                     |
| `src-tauri/crates/overlay-bench/`     | The file-operation benchmark for the hook: baseline, pass-through and active, with allocations.                |
| `src-tauri/crates/overlay-probe/`     | The in-session doctor: a matrix of file operations and a JSON report, x64 and x86.                             |
| `src-tauri/crates/overlay-dump/`      | The minidumper server shared by Gobbo and the 32-bit helper.                                                   |
| `src-tauri/crates/overlay-launch/`    | Windows launcher: starts a program suspended and loads the hook. May use `unsafe`.                             |
| `src-tauri/crates/overlay-pe/`        | Reads the import table and the bitness of an executable from its bytes. Plain logic, tested on every platform. |
| `src-tauri/crates/overlay-hook/`      | Windows DLL loaded into the game; hooks file APIs. May use `unsafe`.                                           |
| `games/`                              | One folder per supported game with `game.toml` (data only; no branches on a game id).                          |
| `loaders/`                            | Shared loader presets in `<id>.toml` (data only; no branches on a loader id).                                  |
| `scripts/`                            | Build/package scripts, the overlay hook DLL, and the release measurement script.                               |
| `.agents/`                            | The procedural layer: a few focused skills. Rules stay in the AGENTS pyramid.                                  |
| `plans/`                              | Local, git-ignored plans and roadmaps.                                                                         |
