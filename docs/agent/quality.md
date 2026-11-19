# Quality reports and E2E

## Weekly security

The `Security` workflow runs `cargo deny check` and `bun audit --production --audit-level high` weekly and on demand, so
an advisory published between pushes is still reported. It also runs on a pull request that changes a lockfile, the
dependency policy or the workflow itself. `cargo deny` fails on every advisory that is not explicitly ignored in
`src-tauri/deny.toml`; each ignore carries a reason and a recheck date, so a moderate advisory with no safe upgrade is
visible without blocking a merge on noise. `cargo machete` and `bun run docs:check` stay report-only.

`ci.yml` first classifies the changed paths: formatting, biome, oxlint, types and the architecture guards run on every
change. A pull request runs the web gate and a single Linux `clippy --all-targets` gate – which type-checks the tests
without running them – so it pays for one Rust compile. The full Rust suite and the Windows gate (whose build script
needs the staged hook binaries) run on main, nightly and reusable calls; a pull request that touches Windows-only code
also runs the Windows gate. Nightly and manual runs run every gate. The release calls the same workflow with
`e2e: false`, so the long E2E suite never holds a release.

`sccache` is deliberately not used: it cannot cache `cargo check` or `cargo clippy`, and `rust-cache` already restores
all dependencies. Cargo deliberately drops workspace artifacts from its cache, so adding more jobs would recompile the
workspace more often. One test that cannot find its helper process on a hosted runner is quarantined with `#[ignore]`;
it runs in a non-blocking step on main and nightly.

## Optional local reports

These are measurements, not merge gates. A normal change does not need to run them; use them when the change touches
the area they measure.

- `bun run size:report` — the first paint of the web bundle. Run it after `bun run build:web`; it prints the assets and
  the gzip total, with no fixed budget to update.
- `bun run docs:check` — relative links, anchors, image paths and alt text in Markdown. The weekly Security workflow
  also runs it as a report.
- `bun run deadcode` — Knip: unused files, exports and dependencies.
- `bun run ui:contrast` — contrast of the design tokens.
- `bun run ui:check` — raw form controls and the generated `components/ui` rules.
- `bun run icons:check` — the game icons (size, format and dimensions).
- `bun run bench:list` — the pure mod-list pipeline baseline (filter, rows, table groups) with p50/p95.
- `bun run translations:check` — locale keys with no literal use, backend code or dynamic prefix. Every hit needs a
  human check before removal; codes built at run time are not always visible to the scanner.

Rust coverage and mutation testing are local tools too. From `src-tauri`:

```bash
cargo llvm-cov --locked --workspace --lcov --output-path target/coverage/lcov.info
cargo mutants -p gobbo-overlay-core -p gobbo-overlay-pe --output target/mutants
```

Install the subcommands once with `cargo install --locked cargo-llvm-cov cargo-mutants`.

## E2E

`bun run e2e:build` builds the web bundle in the `e2e` mode and a debug app with `--features e2e` and
`tauri.e2e.conf.json`; `bun run e2e` runs the WebdriverIO suite against the embedded WebDriver server. The binary embeds
the web bundle, so run `e2e:build` again after a change under `src/`. The suite uses a real Node.js, not Bun
(`expect-webdriverio` does not run under Bun); set `E2E_NODE` when `node` is a shim. `GOBBO_DATA_DIR`, `HOME` and the
Steam/Proton fixtures all point under `target/`, so the suite never opens the user's library. The `e2e` feature,
`tauri.e2e.conf.json` and the dialog/forced-failure seams are test-only: never enable them in a release build.

`bun run e2e:smoke` runs the seven core specs for a quick local check. CI runs the full suite nightly and on demand, not
on a pull request and not as a release gate. `E2E_SPECS`, `E2E_SMOKE` and `E2E_BAIL` select a subset, the smoke set and
fail-fast. The harness, isolation and the list of specs are in [`e2e/README.md`](../../e2e/README.md). Run it for a
change to the window, the IPC surface, the first-run flow, deployment, launch, profiles, packs, FOMOD or recovery, or
when you add a user-visible command.

A GitHub-hosted runner can refuse unprivileged user namespaces. CI tries to allow them, and the tests that really mount
an overlay then return with a clear message; the same specs take the documented `deploy.overlay_unavailable` path. A
machine that allows namespaces still runs every overlay test in full.
