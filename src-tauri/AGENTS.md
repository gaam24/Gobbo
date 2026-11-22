# AGENTS.md: the Rust backend of Gobbo

This file describes how `src-tauri` is organized and the rules that keep it that way. Read it before you add, move or
rename code here, and keep it current when the structure changes. The root [`AGENTS.md`](../AGENTS.md) holds the global
rules; this file is the backend layer of the pyramid. For work inside `src-tauri/crates/overlay-hook/`, stop here and
read [`crates/overlay-hook/ARCHITECTURE.md`](crates/overlay-hook/ARCHITECTURE.md) — it is the binding contract for the hook.

Gobbo is a mod manager for PC games. The backend imports mod archives into a library, keeps a mod list (profiles),
computes a deployment plan, deploys the mods into an overlay layer, and starts the game. The React frontend in `../src` calls the commands in `src/commands`.

## Layers

Code depends on the layers below it only. Nothing lower depends on anything higher.

| Layer          | Folder                                                                                                                                                                                         | Responsibility                                                                                                                                                                        | May use                                                                         |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| Composition    | `app/`, `main.rs`, `lib.rs`                                                                                                                                                                    | Startup, plugins, window setup, command-line mode, `bindings.ts` check. `lib.rs` only declares modules and re-exports `run` and `run_cli`.                                            | everything                                                                      |
| IPC            | `commands/`                                                                                                                                                                                    | The Tauri handler list and the shared `blocking`, `for_game` and `change` helpers. No command bodies.                                                                                 | every module, `error`                                                           |
| Modules        | `acquire/`, `games/`, `library/`, `deploy/`, `import/`, `launch/`, `profiles/`, `packs/`, `collections/`, `sources/`, `status/`, `settings/`, `diagnostics/`, `overwrite/`, `mods/`, `update/` | Vertical modules: each owns its commands, use cases and engine. A module may use a lower module's public API; no cycles.                                                              | `context`, `domain`, `infra`, `platform`, `providers`, `error`                  |
| Context        | `context/`                                                                                                                                                                                     | `Data`, loaders, settings, path resolution, the live game table, the library index (`context/library/`) and per-profile data.                                                         | `domain`, `infra`, `platform`, `games` (the definitions), `migrations`, `error` |
| Migrations     | `migrations/`                                                                                                                                                                                  | Idempotent steps that move data from an older file layout to the current one. No version marker; each step checks the layout.                                                         | `infra`, `error`                                                                |
| Providers      | `providers/`                                                                                                                                                                                   | External mod services behind `SourceProvider`; Nexus and Thunderstore are the adapters.                                                                                               | `domain`, `infra`, `platform`, `error`                                          |
| Domain         | `domain/`                                                                                                                                                                                      | Pure logic: mod list, deployment plan (`resolver`), names, paths keys, versions, argument parsing. No disk, no network, no Tauri.                                                     | `error` only                                                                    |
| Infrastructure | `infra/`                                                                                                                                                                                       | Archive reading (zip, 7z, rar), JSON and atomic file writes, hashing, HTTP, and the file of the profiles (`infra/profiles_file.rs`, which adds `read` and `save` to the domain type). | `error`, and the domain types it stores                                         |
| Platform       | `platform/`                                                                                                                                                                                    | Process list, Proton lookup, and the other Linux or Windows differences, behind one set of names.                                                                                     | `error`                                                                         |
| Errors         | `error.rs`                                                                                                                                                                                     | `AppError` (the code the frontend sees), `ErrorKind`, `FileError`.                                                                                                                    | nothing                                                                         |

The backend tests live in the test-only `tests/` module (`src/tests.rs` and `src/tests/`). Do not add production code
there; a new use case goes into the module that owns it.

## Start here

| What you need                                                                                       | First file                                                                                                                                                                                                                        |
| --------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Add or change a Tauri command                                                                       | `commands/mod.rs`, then the `commands.rs` of its module                                                                                                                                                                           |
| Change `game.toml`, a loader preset or a game rule                                                  | `games/definition/`, `games/loaders.rs`, `games/validate/`, `domain/deploy.rs`                                                                                                                                                    |
| Change how official definitions are fetched, gated or installed                                     | `games/remote/` (`manifest.rs` the gates, `install.rs` the download), then `games/commands.rs`                                                                                                                                    |
| Change the library index or a mod record                                                            | `domain/library/model.rs`, `context/library/index/mod.rs`, `context/library/index/scan.rs`                                                                                                                                        |
| Change the deployment plan or undo                                                                  | `deploy/mod.rs`, `deploy/journal.rs`                                                                                                                                                                                              |
| Remove a mod that may be deployed                                                                   | `library/delete.rs`, `deploy/remove.rs`                                                                                                                                                                                           |
| Change the games the user added (folder, add, remove, restore, storage)                             | `settings/managed.rs`, `context/managed.rs`                                                                                                                                                                                       |
| Change how old data moves to the current layout                                                     | `migrations/mod.rs`, `migrations/game_ids.rs`, `migrations/games_list.rs`                                                                                                                                                         |
| Change how a game starts, stops or runs a tool                                                      | `launch/commands.rs` (`GAME_EXITED_EVENT`), `launch/mod.rs`, `launch/start.rs`                                                                                                                                                     |
| Change how a loader package is shown or installed                                                   | `setup/`, `status/preflight.rs`, `games/definition/install.rs`                                                                                                                                                                    |
| Change how a mod archive gets into the library (link, collection, update, requirement, picked file) | `acquire/mod.rs`                                                                                                                                                                                                                  |
| Change importing or FOMOD                                                                           | `import/flow/` (`import_batch`, the production entry point every import takes; `import_archive` is the single-import wrapper used by tests and benches, and `ImportRequest` the request type), `import/install/`, `import/fomod/` |
| Change which library mod a pack mod, waiting entry or collection member is                          | `domain/library/view/find.rs` (`LibraryView::find`), `domain/pack.rs` (`PackMod::is_same_mod`)                                                                                                                                    |
| Change how a known provider file is downloaded                                                      | `providers/fetch.rs` (`fetch` and `fetch_with_progress`, shared by collections, packs, updates, requirements and the deep-link download queue), `providers/download.rs` (archive target and body-error rules the adapters share)  |
| Change launch, Proton or tools                                                                      | `launch/start.rs`, `launch/proton.rs`, `launch/tools.rs`                                                                                                                                                                          |
| Add a provider or change Nexus                                                                      | `providers/mod.rs`, `providers/nexus/mod.rs`, `providers/testing.rs`                                                                                                                                                              |
| Change Thunderstore or its `ror2mm://` links                                                        | `providers/thunderstore/mod.rs`, `providers/thunderstore/link.rs`                                                                                                                                                                 |
| Change how download links are registered                                                            | `settings/links.rs`, `providers/mod.rs`, `tauri.conf.json`                                                                                                                                                                        |
| Change the download queue or its auto-install                                                       | `sources/downloads/queue/`, `sources/downloads/model.rs`                                                                                                                                                                          |
| Change shared state or settings                                                                     | `context/data/mod.rs`, `context/data/edit.rs`, `context/settings.rs`                                                                                                                                                              |
| Change per-profile data (copy list, user-data sets, switching, history, lock)                       | `context/profile/state.rs`, `context/profile/user_data.rs`, `context/profile/history.rs`, `context/profile/lock.rs`                                                                                                               |
| Change update checks                                                                                | `sources/update/check.rs`, `install.rs`, `providers/nexus/update/`                                                                                                                                                                |
| Change collection install, a collection source or its profile state                                 | `collections/`, `domain/collection/`, `providers/nexus/collections/`                                                                                                                                                              |
| Change what the window sees                                                                         | `status/views/` (`mod.rs`, `source.rs`, `version.rs`), `status/mod.rs`                                                                                                                                                              |
| Change the game stage or the next action                                                            | `status/stage.rs`                                                                                                                                                                                                                 |
| Change the system tray's status, menu or actions                                                    | `tray/view.rs`, `tray/menu.rs`, `tray/actions.rs`                                                                                                                                                                                 |
| Change the setup checklist or a preflight check                                                     | `status/preflight.rs`, `infra/disk.rs`                                                                                                                                                                                            |
| Change an OS difference                                                                             | `platform/` or `deploy/overlay/`                                                                                                                                                                                                  |

## Module map

The module map moved to [docs/module-map.md](docs/module-map.md). Read the entry for the module you touch; the
`Start here` table above is the dispatch for a new task.

## Rules

### Dependencies and imports

- Import the module that defines a name (`crate::domain::profiles::Profiles`), not a re-export from a module root.
- No glob imports (`use x::*`) in non-test code. `use super::*;` is allowed only inside `#[cfg(test)] mod tests`.
- Do not add `pub use submodule::*` to a module root. Re-export single names when a caller needs a shorter path.
- Do not add a cycle between modules. A lower module may expose a public API to a higher one (for example `deploy`
  reads `library`); a shared rule goes into `domain` or `infra`, and shared state into `context`.
- One mechanism, one owner. Every mod archive a feature brings in (a link download, a collection member, an update, a
  requirement, a picked file) goes through `acquire::acquire` (or `acquire::acquire_batch`), which checks the library
  for the exact file, fetches, picks the FOMOD choices, imports and cleans up the download; underneath, every import
  goes through `import::import_batch` (`import::import_archive` is its single-archive wrapper for tests and benches);
  every match of a wanted mod against
  the library goes through `LibraryView::find` — or `find_with` with a `FindQuery` from
  `domain::library::view::find`, whose `MatchPolicy` covers the strict, folded and key rules (a collection member
  becomes a `PackMod` with `CollectionItem::wanted`); every download of a known provider file goes through
  `providers::fetch` (`fetch`, or `fetch_with_progress` when the caller keeps its own byte meter). A
  feature adds its own steps around these calls, never its own copy of them. The same holds for the small rules: a
  free name is `domain::naming::first_free`, a percentage is `domain::progress::percent`, a stop check is
  `infra::tasks::stop_if` and a stop result is `AppError::is_cancelled`, a long command for one game is
  `commands::run_task`, a recipe's new profile is `Profiles::create_unique`, a launch that lets the caller report the
  exit is `launch::launch_game_with_notice` (the command emits `GAME_EXITED_EVENT`), and a stop that records a user
  stop is `launch::stop_game_with_notice`.
- A lower layer does not call up for a value. When it needs something only a higher one knows, the caller passes it in:
  `context::open` takes the archive reader (`providers::archive_name_from`) to name the archives of the libraries at
  startup; the index never parses a file name itself.

### Errors

- Every module that can fail has its own error type, or returns `AppError` directly. Module error types derive
  `thiserror::Error` and convert into `AppError` with `From`. `DeployError` and `InstallError` wrap `FileError` with
  `#[from]`.
- A file or directory failure is always `FileError` (`FileError::new(path, source)`). Use `io_at(path)` in a
  `map_err` that returns `AppError`.
- The codes of `AppError` are stable strings such as `deploy.blocked`. The frontend translates them from its locale
  files. Never rename a code without updating the locales.
- Use `ErrorKind` to say what the user can do: `UserAction`, `Retryable`, `Permanent` or `Bug`.
- No `unwrap()` or `expect()` in code that runs in the app. They are fine in tests. The app crate denies
  `clippy::unwrap_used`, `clippy::expect_used` and `clippy::panic` outside `cfg(test)`, so a regression fails clippy.

### Global state

Process-wide state is kept to these, which each guard one process-wide resource:

- `DEFINITION_DIRS` in `games/mod.rs`: the game and loader definition folders, set once at startup.
- `INDEX_LOCK` in `context/library/lock.rs` (through `lock_index`), and its thread-local `HOLDS_INDEX`: serialize changes to
  `library.json`.
- `PROFILES_LOCK` in `context/profile/lock.rs` (through `lock_profiles`), and its thread-local depth: serialize changes to
  `profiles.json` and the list history from the read to the last write. The mutex is reentrant on one thread; a nested
  change takes no second guard.
- `SETTINGS_WRITE` in `context/settings.rs` (through `lock_settings`): serializes the writes of a game's settings file,
  so a whole save and a one-list edit (`set_proton_option`, `add_profile_data`) never write over each other. It is
  taken alone, never while holding another lock.
- `RESERVED` and `RENAME_LOCK` in `infra/store.rs`: archive names held by an import, and the serialization of atomic
  file replaces.
- `LIVE` in `context/running.rs`: the games this process started and that are still running.
- The advisory `gobbo.lock` file lock in `context/state_lock.rs` (through `lock_state`), reentrant on one thread: it
  serializes the file-moving operations across threads and across Gobbo processes (the window and `gobbo run`).
- `LOG` in `infra/log.rs`: the open `gobbo.log` file, set once at startup; a missing log never fails a caller.
- The `OnceLock` agent in `infra/http/`, and the counter behind `unique_suffix` in `infra/store.rs`: they hold
  configuration and a counter, not game state.

Do not add another static, `OnceLock` or thread-local for game state. Pass state through `Data` or as a parameter.

**Lock order.** `gobbo.lock` → `INDEX_LOCK` → `PROFILES_LOCK` → `RESERVED`/`RENAME_LOCK`. A change reads the library
before it takes `PROFILES_LOCK`, then re-reads `profiles.json` under the lock; it never takes the index lock while
holding the profiles lock. The rule covers the read-modify-write, not only the save: a function that loads profiles
before `lock_profiles` has already lost an edit made in between. `lock_state` is reentrant on one thread; a second
thread or process gets `state.busy` instead of waiting behind a long apply.

### Platforms

- Differences between Windows and Linux live in `platform/`, in `deploy/overlay/` and in items gated with
  `#[cfg(windows)]` or `#[cfg(not(windows))]`. Gate the whole item, not single lines inside it.
- Gate an import with the same cfg as the code that uses it. A Windows build cannot see Linux-only uses, so check
  both: `cargo clippy` on Windows, and a Linux check in CI.
- `unsafe` is forbidden in `gobbo` and `gobbo-overlay-core` (`#![forbid(unsafe_code)]` at the crate root). The hook and
  the launcher (`crates/overlay-hook`, `crates/overlay-launch`) call Win32 and may use it.

### Files and functions

- A module that grows past about 400 lines of production code becomes a folder: `mod.rs` keeps the public entry points
  and the submodules, and each submodule holds one part (see `collections/session/` and
  `deploy/overlay/windows/`).
- Tests live next to the code: an inline `#[cfg(test)] mod tests` for small modules, or `tests.rs` with a `tests/`
  folder for large ones. Shared helpers go in `test_support.rs`. Fixtures go in `tests/fixtures/`.
- Put a doc comment on every public item. A doc comment must sit directly above its item; a comment that is separated
  from it by an `use` is a bug.
- Keep `domain/` free of I/O. If a function needs the disk, it belongs in `context`, `infra` or a feature module, and
  the decision it makes goes into a pure function in `domain` that a test can call with plain data.

### IPC and bindings

- A command is a thin function: read the arguments, call a feature, return the result. A command that runs one use case
  for one game is declared with `crate::commands::game_commands!` (`query` answers with the result, `change` with the
  game's fresh status); one that needs the app handle, a channel or another state is written out as a plain function.
- `get_status` is a pure read. `status` brings the profiles in step with the library in memory and never writes;
  `sync_profiles` is the change that saves that step. A read helper such as `Profiles::read_or_default` must not create
  the file.
- The command list lives in `commands.txt` at the crate root, one `alias::name` per line. `build.rs` reads it, generates
  the `collect_commands![...]` invocation that `commands::builder` includes, and gives each command an `allow-<name>`
  permission (underscores become hyphens). `scripts/check-generated.ts` reads the same file and checks that
  `capabilities/default.json` lists every one with no stale extras; otherwise the window cannot call that command. The
  test `app::tests::every_command_has_a_permission_in_the_main_capability` checks the generated artifact too. Add a new
  command to `commands.txt` and to the capability in the same change.
- Every type that crosses IPC derives `specta::Type` (and `Serialize` or `Deserialize`).
- After you change a command or an IPC type, regenerate `../src/bindings.ts`: run the app in debug mode, or run
  `cargo test --lib write_bindings -- --ignored`. The test `app::bindings::tests::bindings_match_the_commands` fails
  while the committed file is out of date. Commit the regenerated file with the change.

### API design

The API shape and error rules are in [docs/adding.md](docs/adding.md#api-design).

### Adding a provider or a feature

The step-by-step checklists are in [docs/adding.md](docs/adding.md). A new provider implements the port and adds a
`Case` to `providers/testing.rs`; a new feature starts in its own module and calls the layers below it.

## Known exceptions

The current exceptions and the layout history are in [docs/history.md](docs/history.md). Do not add an exception
without a reason and a test; a new one belongs in that file.

## Workspace and lints

- `Cargo.toml` at this folder is the workspace. `members = ["crates/*"]`. Versions, the edition and the license are set in
  `[workspace.package]`; shared dependencies in `[workspace.dependencies]`; lints in `[workspace.lints]`. The root
  package opts in with `[lints] workspace = true`.
- `crates/overlay-pe` reads the static import table and the bitness of an executable from its bytes, with checked offsets.
  It is plain logic with no Windows calls and no `unsafe`, so its property tests run on Linux too. The launcher reads the
  file and asks it.
- `crates/overlay-decisions` is the ring format shared by the hook and the app. Keep it plain `std`, with no Windows calls
  and no `unsafe`, so the app reads it on every platform.
- `crates/overlay-hook` reads the mount set from the JSON file in `GOBBO_OVERLAY_MOUNTS` (falling back to the three
  legacy variables for one game mount). Each mount has its own whiteouts; a move/replace/rename may not cross mounts.
  The same hooks serve the game and the profile's user-data redirects; do not add a hook for user data.
- `crates/overlay-launch` is the launcher: it starts a program suspended, writes the x64/x86 stub before
  `LdrInitializeThunk`, rewrites the program's static import descriptors to layer paths, and sets the thread context. It
  never copies a mod DLL into the game folder; a 64-bit launcher uses `gobbo_overlay_inject32.exe` for 32-bit threads.
- `crates/overlay-bench` is a measurement tool, not part of a release. It builds the hook with
  `--features "bench-alloc bench-stats"` to count allocations and probe/lock/I/O stats inside the DLL; never enable
  those features in a shipped hook.
- `crates/overlay-probe` is the doctor. It runs as a normal program inside an overlay session and writes a JSON report;
  it is built for x64 and x86, because a 32-bit game needs the 32-bit hook to be checked.
- `crates/overlay-dump` is the minidumper server. Gobbo uses it in-process for x64 and starts its
  `gobbo_overlay_dump32.exe` binary for x86; keep the two architectures separate, because the protocol is pointer-sized.
- The overlay crates do not opt into the pedantic lint group, because the hook is Win32 code. Changing that would add
  many warnings to `crates/overlay-hook`.
- Lint level is `warn` in the manifest and `-D warnings` in the gates. Do not add `#[allow]` without a reason in the code.
- The release profile uses `opt-level = 2` and the default feature `mimalloc`, measured on the status benchmark; a size
  comparison can use `cargo build --release --no-default-features`. Keep both numbers when changing either setting.

## Verify before you finish

Run these from `src-tauri`. They are the same gates as `bun run check:rust` and CI:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --features bench --locked
```

`--features bench` adds the benchmark fixtures and the overlay allocation budget test. On Windows, run
`bun run build:overlay-hook` once first: the overlay tests load the hook binaries it stages in `target/debug`.

A focused data-format change can be checked first with `cargo test --lib migrations` in `src-tauri`; the full workspace
run above covers it before you finish.

`bun run lint` from the repository root is the six-stage merge gate: Biome, oxlint, the TypeScript check (`tsc`), the
E2E type check (`e2e:types`), the architecture guard (`guard`) and the dead-code check (`deadcode`), all of which must
pass to merge. The guard runs `scripts/check-layers.ts`, `scripts/check-agent-policy.ts`,
`scripts/check-data-boundary.ts`, `scripts/check-overlay-invariants.ts` and `scripts/check-generated.ts`; it reads the
production code only, so test fixtures may cross layers. The full E2E suite runs only on demand, not as a release gate;
the `Security` workflow runs `cargo deny` and `bun audit` weekly and on pull requests that change dependencies.

Coverage and mutation reports are optional local tools, not CI gates. Run `cargo llvm-cov` or `cargo mutants` when a
change needs the extra signal; the E2E suite and its isolation are described in
[`../docs/agent/quality.md`](../docs/agent/quality.md).

The `e2e` feature registers `tauri-plugin-wdio-webdriver` for the WebdriverIO suite. It is test-only and is never part
of a release or a normal debug build; `bun run e2e:build` enables it with `tauri.e2e.conf.json`.

Run the Linux-only paths too when you touch `platform/`, `deploy/overlay/linux.rs`, `launch/proton.rs`,
`launch/tools/prerequisites.rs`, or any item gated with `not(windows)`: CI builds them on Ubuntu, and a Windows
build does not compile them.
