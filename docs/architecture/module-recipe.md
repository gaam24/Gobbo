# Module recipe

A feature starts as one `.rs` file. When it grows, split it into a folder with the same shape as every other module.
The backend rules are in [`../../src-tauri/AGENTS.md`](../../src-tauri/AGENTS.md); the shape is checked by
`bun scripts/check-module-shape.ts` in the `guard` chain.

## The shape

- `mod.rs` is a façade: a module doc comment, `mod` declarations, and explicit re-exports of the names callers use. Keep
  it thin — a folder with submodules should not hide logic in its façade — but there is no separate, smaller limit.
- **Every source file, `mod.rs` included, stays under 400 production lines**; the guard fails at 401. "Production lines"
  means the file up to an inline `#[cfg(test)] mod tests` (or `mod tests;`). Test-only files — `tests.rs`,
  `test_support.rs`, `bench_api.rs` and anything under a `tests/` folder — are not counted.
- Re-exports use **single names**, either one at a time or in an explicit brace list. There are no globs: no
  `pub use submodule::*;` in a façade and no `use x::*;` in production code. `use super::*;` belongs only in
  `#[cfg(test)] mod tests` and in a test-only `tests.rs`.
- Import a name from the module that defines it, not from a façade that happens to re-export it. A submodule names its
  siblings directly (`use super::model::Family`), never through the façade's re-exports.

## The vocabulary

| File            | Holds                                                                                                  |
| --------------- | ------------------------------------------------------------------------------------------------------ |
| `model.rs`      | The data types and pure rules of the module. No disk, no network, no Tauri.                             |
| `port.rs`       | A trait the module consumes or provides, with the request and result types crossing that boundary.      |
| `plan.rs`       | A computed plan: the steps to run, assembled from `model.rs` and the ports, ready to be applied.        |
| `io.rs`         | The only part that reads or writes files. It takes values from `model.rs` and returns plain data.       |
| `commands.rs`   | Thin Tauri command functions: read arguments, call one use case, return the result.                     |
| `error.rs`      | The module's error type and its conversion into `AppError`.                                            |
| `tests.rs`      | A large test module next to the code, paired with a `tests/` folder for fixtures.                       |
| `mod.rs`        | The façade: docs, module declarations and explicit single-name re-exports.                              |

Not every folder needs every file. Use the names that fit the part being split; a small module can keep a pure rule and
its I/O in one file until the 400-line limit says otherwise. When a file splits, the public entry points stay in
`mod.rs` so callers keep the same path.

## Dependency direction

The layers below a feature never call up into it: `error` ← `domain` ← `infra`, `platform`, `providers` ← `context` ←
feature modules ← IPC ← composition. A folder keeps that direction inside itself too:

- `model.rs` and `port.rs` depend only on lower layers and on each other.
- `plan.rs` composes those pure parts; it does not open files or call a command.
- `io.rs` is the only submodule that touches the disk or the network. The decision it makes belongs in `model.rs`, so
  a test can call it with plain data.
- `commands.rs` sits at the edge: it calls a use case and returns its result. It never owns a rule.
- A submodule does not reach back up through the façade. A rule shared by two submodules gets its own file and is
  declared in `mod.rs`; a rule shared by two modules moves down to `domain` or `infra`, and shared state to `context`.

A shared rule gets a named owner instead of piling up in a façade: a pure rule used by two modules goes into its own
file under `domain/` (taking `is_windows` as a parameter where the rule depends on the platform), while registration and
dispatch helpers live in the layer they serve so `mod.rs` stays imports plus its entry point. The
[module map](../../src-tauri/docs/module-map.md) names the current owners.

## Where new code belongs

1. A pure type or rule goes into `model.rs` of the module that owns it, or into `domain` if two modules need it.
2. An external system (a provider, a service, an OS facility) is a `port.rs` plus an adapter in `providers` or
   `platform`; callers receive values, not a concrete client.
3. A sequence of steps goes into `plan.rs` (or the module's use case), then `commands.rs` exposes it if the window
   needs it.
4. A file read or write goes into `io.rs`, which returns the domain type. Keep paths as data; `infra` owns the actual
   file operations.
5. Tests go into an inline `#[cfg(test)] mod tests` for a small file, or into `tests.rs` with fixtures under `tests/`
   for a large one. Shared helpers go into `test_support.rs`.

When a file crosses 400 lines, split along these seams rather than moving an arbitrary block: the limit is a signal that
two responsibilities have grown together. A thin façade is the goal, not a second number to enforce.
