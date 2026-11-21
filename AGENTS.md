# AGENTS.md: Gobbo Mods Organizer

This is the **global contract**: the rules that hold for every change, in every area. Read this file first, then open
only the narrower guide your task needs. The guides form a pyramid — do not load the rest "just in case".

Gobbo is a mod manager for PC games. The Rust backend in `src-tauri/` imports mod archives into a library, keeps
profiles and a mod list, computes a deployment plan, deploys through an overlay layer and starts the game. The React
window in `src/` calls the Tauri commands in `src-tauri/src/commands/`. Layout: `games/` and `loaders/` are data and
`scripts/` holds the build and check tools; the full table is in [docs/agent/layout.md](docs/agent/layout.md).

## 1. Route before you read

| Task                                                  | Open next — only this                                                                                                         |
| ----------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| anything user-visible, docs or config                 | `CHANGELOG.md` → **Unreleased**; [docs/README.md](docs/README.md) for the documentation map                                   |
| React window, a screen, a shared component            | [src/AGENTS.md](src/AGENTS.md); for visual work also [DESIGN.md](DESIGN.md) and [docs/ux/glossary.md](docs/ux/glossary.md)    |
| Rust app, a command, an IPC type                      | [src-tauri/AGENTS.md](src-tauri/AGENTS.md)                                                                                    |
| anything under `src-tauri/crates/overlay-hook/`       | [ARCHITECTURE.md](src-tauri/crates/overlay-hook/ARCHITECTURE.md) — read it **first**; it is the binding contract for the hook |
| a game or a loader definition                         | [games/README.md](games/README.md) and [loaders/README.md](loaders/README.md)                                                 |
| game state, the primary action or a long-running task | [docs/state-machines.md](docs/state-machines.md)                                                                              |
| a Windows/Linux difference                            | [PLATFORMS.md](PLATFORMS.md) and the platform rules in the area guide                                                         |
| seeing or measuring the real window                   | load the `gobbo-run-app` skill; runbook in [docs/agent/run-app.md](docs/agent/run-app.md)                                     |

A narrower guide can point to further docs; follow only the part you touch. If two files disagree, the more specific
contract wins for its area (the hook's `ARCHITECTURE.md` wins for the hook) — fix the stale one in the same change.

## 2. Load a skill when one matches

Most tasks have no skill: the routed guide is the whole procedure. A few do — inspecting the real window, auditing
accessibility, measuring performance, auditing security or handling an incident. Their names and descriptions are
**already listed** as available skills, but a skill is never executed on its own: when a task matches a description,
load it after routing and before reading code (in OpenCode, call the `skill` tool with its id), then follow its
`## Read → ## Do → ## Verify`. Use one skill per task. Rules live in the AGENTS pyramid and the area guides, never in a
skill: if a skill disagrees with a guide, the guide wins and the skill is fixed in the same change.

## 3. Universal rules

- Keep the existing structure and naming. Prefer the smallest change that solves the problem.
- No new dependency without a reason, a license check and a maintenance check.
- A user-visible change gets a `CHANGELOG.md` entry under **Unreleased**.
- A changelog entry is short and written for users: one bullet per change, a bold one-line summary and at most one
  plain sentence after it (`- **Mods load again after an update.** The deploy no longer skips renamed files.`).
  Leave out internals — paths, file names, checksums, keys, how the code does it; those belong in the docs.
- Do not commit generated build output (`target/`, `dist/`, `src-tauri/overlay-hook/`). The staged hook DLL is ignored.
- Biome formats code, JSON, CSS and config only. Markdown — including `SKILL.md` and subagent frontmatter — is written by
  hand; do not add it back to `biome.jsonc`.
- Do not add `#[allow]` without a comment explaining why; use `reason = "..."`.
- Destructive actions (push, release, delete data, clean the worktree) need an explicit user request. Do not launch a
  game, deploy, import or change settings unless the user asked for that action.

### Universal code, both platforms

These rules apply to every change, whichever game or platform started it.

- **Universal, not per game.** Gobbo serves every game the same way. Code never has a special case for one game, one
  file name, one Proton build or one folder layout. A game's difference is data in `games/<id>/game.toml`, a setting the
  user chooses, or a reusable primitive with a test. A fix that works only for the game it was found in is not finished.
- **Tests are universal and in English.** A fixture uses generic names and ids, never the mod, file or game from the bug
  report that prompted the test. Test names, comments and data are written in English, so a test stays readable on any
  platform and for every game.
- **The user chooses what depends on the game or the computer.** Proton build, launch flags and folders are settings
  the user can change. A default ("Steam's choice") is a documented rule the user can override; it is never a value
  written into the code that cannot be changed.
- **Same behavior on Windows and Linux.** A feature gives the same results, error codes and data on both systems.
  Platform differences stay in the platform layer (`platform/`, `deploy/overlay/linux.rs`, `#[cfg]` at the edges). Both
  sides export the same names, so a function that exists on one platform only fails to compile on the other.
- **Hide what does not apply.** The interface does not show an option the current platform cannot use. Proton is not
  shown on Windows. The backend answers an empty or "not supported" result for such a feature there, so the interface
  never has to guess.
- **Portable data moves between systems.** Data a user exports on one system and imports on the other (packs, profile
  mod lists, library metadata, profile data) stores paths relative to a known root and with `/` separators. It never
  stores a drive letter, a Proton build or a path that only exists on one machine. Machine-specific values (game folder,
  Proton choice, mods folder, launch environment) are local settings: an export leaves them out, and an import asks the
  user for them again.
- **Case and names.** Linux is case-sensitive, Windows is not. Mod files, folders and profile data are matched with the
  same path-key rule on both platforms (`domain/pathkey`). A name valid on one system must import on the other.
- **Round trip is tested.** A change to export, import, paths or settings has a test that exports under one platform's
  rules, imports under the other's and compares the result. Tests that need one OS are `cfg`-gated and say so.
- **Both sides are checked.** A change in `platform/`, `deploy/`, `launch/`, `games/`, `packs/` or any data format runs
  the Linux checks and the Windows checks in "Verify before you finish". If one side cannot run here, the report says so.

### Checks and reports

`bun run lint` is the merge gate: Biome, oxlint, the TypeScript checks and the architecture guard (layers, data
boundary, production `unwrap`/`expect`/`panic`/`todo`/`dbg`, overlay invariants, generated bindings and permissions).
The full E2E suite runs only on demand, not as a release gate; the `Security` workflow runs `cargo deny` and
`bun audit` weekly and on pull requests that change dependencies. CI (`.github/workflows/ci.yml`) runs the web gate, Linux clippy and tests, and a Windows job (clippy and tests), each only
when the diff needs it.
Coverage, mutation, bundle-size and docs-link reports are local tools, not merge gates. Details:
[docs/agent/quality.md](docs/agent/quality.md).

## 4. Verify before you finish

From the repository root:

```bash
bun run lint
bun run test
bun run check:rust
```

`bun run check:rust` runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
and `cargo test --workspace --features bench --locked` (which includes the overlay allocation budget test) in
`src-tauri`; on Windows run `bun run build:overlay-hook` first. Area-specific commands are in
[src-tauri/AGENTS.md](src-tauri/AGENTS.md) and in the hook's `ARCHITECTURE.md`. A change in `platform/`, in `deploy/` or
behind `#[cfg]` is checked on Linux and on Windows; name any gate that cannot run on this machine instead of skipping it
quietly.

## 5. Commit messages

Use the existing conventional style, scoped by area when useful:

```text
feat(overlay-hook): ...
fix(overlay-hook): ...
refactor(overlay-hook): ...
docs(overlay-hook): ...
feat(games): ...
fix(ui): ...
```

When a change is only a refactor, say so. When it changes behavior, add a `CHANGELOG.md` entry.
