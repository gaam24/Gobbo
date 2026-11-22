# Contributing to Gobbo Mods Organizer

Thank you for helping. This guide explains how to set up the project, what to check before you open a pull request, and
which rules the code follows. The map of the documentation is in [docs/README.md](../docs/README.md).

## Before you start

- For a bug, search the [existing issues](https://github.com/gaam24/Gobbo/issues) first, then open one with the bug
  report form.
- For a larger change, open an issue or a discussion first, so we can agree on the approach before you write the code.
- For a new game, read [games/README.md](../games/README.md). A game is added only after it has been tested on a real
  installation.
- For documentation, read [docs/README.md](../docs/README.md) and use the labels in
  [docs/ux/glossary.md](../docs/ux/glossary.md). The README is a landing page; task guides live in `docs/user/`.

## Setting up

You need [Bun](https://bun.sh) 1.4.4 and a stable Rust toolchain (1.97 or newer; the exact MSRV is in
`src-tauri/Cargo.toml`). The Tauri prerequisites for your
system are listed in the [README](../README.md#building-from-source).

```bash
bun install      # also installs the git hooks through lefthook
bun run tauri dev
```

## Checks

Run these before you open a pull request. CI runs the lint, unit, build and Rust gates on Linux and Windows; the
release waits for that CI run instead of repeating it. E2E runs only on demand (Actions → CI → Run workflow), not
nightly and not before a release.

| Command              | What it checks                                                                              |
| -------------------- | ------------------------------------------------------------------------------------------- |
| `bun run lint`       | Biome, oxlint (type-aware), TypeScript (app + E2E), architecture guard and Knip dead code   |
| `bun test`           | Frontend unit tests, including that `en.ts` and `pl.ts` have the same keys                  |
| `bun run build:web`  | Vite web bundle and the E2E-hook check                                                      |
| `bun run check:rust` | `cargo fmt --check`, `cargo clippy`, `cargo test` and the allocation budget                 |
| `bun run deadcode`   | Knip unused files, exports and dependencies (also part of `lint`)                           |
| `bun run docs:check` | Optional: relative links, anchors, image paths and alt text in Markdown                     |

The git hooks run part of this for you. The pre-commit hook formats staged files and runs oxlint on them. The pre-push
hook runs lint, the unit tests and the Rust checks.

## Code rules

- **English everywhere in code:** identifiers, comments, log messages and commit messages are in English.
- **Translations:** text the user sees goes into both `src/locales/en.ts` and `src/locales/pl.ts`. Add the key to both.
- **Commits:** use [Conventional Commits](https://www.conventionalcommits.org/), for example `feat(import): ...`,
  `fix(overlay-hook): ...` or `refactor: ...`.
- **Game definitions:** keep them as data in `games/<id>/game.toml`. A layout shared by several games belongs in
  `loaders/<id>.toml`; a game names it with `loader`. Do not add a branch for one game id or one loader id to the core.
  If a game needs a new pattern, add a reusable primitive with tests, and explain the decision in the pull request.
- **Rust:** format with `cargo fmt`, keep clippy clean, and add tests next to the code they test.
- **Platforms:** Gobbo runs on Windows and Linux. Say in the pull request which platform you tested on. Overlay, launch
  and deployment changes need a real installation test.

## Documentation

- The README is the landing page: what Gobbo is, why it is useful, how to start, and where to go next. Task guides
  live in `docs/user/`; the README links to them instead of repeating them.
- One source of truth: the checks table is in this file, the full game table is in [games/README.md](../games/README.md),
  and the repository layout is in [docs/agent/layout.md](../docs/agent/layout.md).
- Use the labels in [docs/ux/glossary.md](../docs/ux/glossary.md) in user-facing copy: **Apply**, not deploy;
  **Keep the game folder clean**; **Installer options**.
- Write in English. The interface strings are separate in `src/locales/en.ts` and `src/locales/pl.ts`.
- Use relative links for repository files. Put images in `docs/assets/` with alt text, and keep each screenshot under
  a few hundred kilobytes.
- Plans are local and git-ignored. Do not link to `plans/` from a tracked document.
- `bun run docs:check` checks relative links, anchors, image paths and alt text. Run it before a documentation change;
  the weekly Security workflow reports it but does not block a merge.

## Pull requests

1. Fork the repository and create a branch from `main`.
2. Make the change, with tests where the behavior changes.
3. Run the checks above.
4. Update `CHANGELOG.md` under `## Unreleased` for user-visible changes.
5. Update the document that owns the changed behavior. The README stays a landing page; task guides belong in
   `docs/user/`. Refresh a screenshot when the interface it shows changed.
6. Open the pull request and fill in the template.

Keep a pull request focused on one change. Small, separate fixes are easier to review and merge than one large branch.

## License

By contributing, you agree that your contribution is licensed under the
[GNU General Public License v3.0](../LICENSE), the license of this project.
