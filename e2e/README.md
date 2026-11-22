# E2E

The suite drives the real Gobbo window through WebdriverIO's Tauri service. It uses the embedded WebDriver server
(`tauri-plugin-wdio-webdriver`), so Linux and Windows need no external driver; Linux CI runs it under `xvfb-run`.

The suite is hybrid: setup and disk checks call the same Tauri commands the window calls through `window.__TAURI__`
(`e2e/support/bridge.ts`), and the flows a user performs are clicked in the real UI (`e2e/support/ui.ts`). Every fixture
is an isolated game definition and a game folder under `target/`, so the suite never opens or changes the real library.

```bash
bun run e2e:build   # build dist/ and target/debug/gobbo with the e2e feature
bun run e2e         # run every spec
bun run e2e:smoke   # a quick local subset
bun run e2e:types   # type-check the config, support files and specs
```

`e2e:build` uses `src-tauri/tauri.e2e.conf.json` and the `e2e` Cargo feature. Both are test-only and are never part of a
release build. The Tauri binary embeds `dist/`, so run `e2e:build` again after every change under `src/`; rebuilding only
the web bundle is not enough. The e2e build replaces `src-tauri/target/debug/gobbo`; rebuild with
`--features mcp-bridge` before using the MCP bridge again.

The runner needs a real Node.js: `expect-webdriverio` does not run under Bun. The wrapper in `scripts/e2e-run.ts`
prefers `E2E_NODE`, then `node`, then `/usr/bin/node`; set `E2E_NODE` when `node` is a shim.

## Selecting specs

```bash
E2E_SPECS='./specs/20-import.e2e.ts,./specs/30-mods-list.e2e.ts' E2E_BAIL=0 bun run e2e
```

- `E2E_SPECS` runs only the named files, in the order given.
- `E2E_BAIL=1` stops at the first failure; the default `0` collects every failure.
- `E2E_SMOKE=1` (or `bun run e2e:smoke`) runs first run, games, command contracts, import, mod list, deploy and launch.
  Use it for a quick local check; CI runs the full suite only when started by hand.

The runner deletes `target/e2e-data`, `target/e2e-home` and `target/e2e-artifacts` before the app starts, then writes
English app settings. A full run is independent of the previous one.

## Isolation

| Folder                 | Contents                                                                                    |
| ---------------------- | ------------------------------------------------------------------------------------------- |
| `target/e2e-data`      | `GOBBO_DATA_DIR`: definitions, games, library, archives and profiles of the suite           |
| `target/e2e-home`      | `HOME`/`USERPROFILE`: Steam, Proton, user-data and per-profile files                        |
| `target/e2e-artifacts` | screenshot, DOM, local storage, browser log, `gobbo.log` and the error of every failed test |

The fake Steam library, the Proton builds and the game folders are built by `e2e/support/fixtures.ts`. Each spec gets a
unique game id (`seedGame` / `uniqueId`), so specs do not share state and can run alone. `cleanupFixture` removes the game,
its definition and its fixture roots, including the user-data and per-profile folders under the fake home; it also
restores the write bit on folders an overlay apply may leave at mode `000` before deleting them.

## Specs

| File                  | Covers                                                                                                                     |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `00-first-run`        | empty app, first-run Games page, shared settings, custom data folder                                                       |
| `10-games`            | locate/add, Steam scan, remove/restore/delete, broken definitions, verified badges, deployed-game guard                    |
| `15-commands`         | recovery action for a missing game/settings folder, optional reads; command codes are in the Rust command-contract tests   |
| `20-import`           | UI import, several archives, wrapper/multi-mod shape, invalid/traversal archives, source, versions                         |
| `30-mods-list`        | search/filter, selection and Space, table/grid, columns and reload, tags/note, delete                                      |
| `35-scroll-stability` | the virtual list keeps its scroll step when the window changes                                                             |
| `36-drag-virtual-list` | dragging table rows and grid cards onto rows inside the window, including auto-scroll                                      |
| `40-profiles`         | create/duplicate, rename/switch/delete, undo/redo, bisect, compare, reload                                                 |
| `50-deploy`           | overlay apply and undo, conflict winner, load-order file, a game file the overlay covers                                   |
| `60-launch`           | launch and report, arguments/environment, stop while running, crash prompt, console                                        |
| `70-packs`            | export with archives, missing archives, import profile, pack-link refusal                                                  |
| `75-deep-links`       | a provider link that arrives while the app is running reaches the download queue                                           |
| `80-fomod`            | wizard from the Import button, validation, skip confirmation, unknown choice, change installer                             |
| `85-background-tasks` | a task stays in Working across a game switch; Stop and Open act on its own game (events through the real registry adapter) |
| `90-settings`         | app settings, language switch and reload, window setting, support bundle, game settings, per-profile files                 |
| `95-ui`               | command palette, shortcut list, details/console shortcuts, narrow window                                                   |
| `99-recovery`         | page and whole-window error boundaries, damaged settings/profiles, interrupted apply, moved game folder                    |

## Test-only seams

A native dialog cannot be driven by WebDriver, so the `e2e` build reads test-only globals that a production build
removes (`scripts/check-e2e-hooks.ts` proves the release bundle has none):

- `window.__GOBBO_E2E_DIALOGS__` answers the file dialogs (`files`, `file`).
  A spec sets it directly; `setThrow` in `e2e/support/ui.ts` arms the failure seam.
- `window.__GOBBO_E2E_THROW__` and `sessionStorage["gobbo.e2e.throw"]` force a render failure in one view or in the
  shell, so the error boundaries are checked with a real window. The session copy survives a reload.
- `window.__GOBBO_E2E_TASKS__(event, payload)` emits a real Tauri task event through the window's own lazy event module.
  `85-background-tasks` uses it for `task-started`, `task-cancelled` and `task-finished`: a real collection install needs
  a provider, a key and network, so the lifecycle is driven by the seam instead. The events reach the same registry
  adapter as backend work — the spec does not inject events with WebDriver's `plugin:event|emit`, and the payload is the
  backend `TaskView` shape (`id`, `game`, `kind`, `state`, optional `percent`). The Start/Stop/Open row and the game
  switch are real UI interactions.

The FOMOD, Steam, Proton, user-data and pack fixtures are generated in `e2e/support/fixtures.ts`; no binary fixture is
committed. The suite asserts the same labels a user reads, not test-only markup.

## Adding a spec

1. Prefix the file with its place in the run (`05-`, `85-`, ...). Add it to `defaultSpecs` in `e2e/wdio.conf.ts` and,
   only if it is a smoke flow, to `smokeSpecs`.
2. Start with `seedGame({ id: uniqueId("prefix"), name })`; it adds the game, saves the fixture paths and selects the
   game in the rail. For command-only tests, `prepareGame` + `add_game` + `save_settings` is faster.
3. Use `importMods`, `enableMods`, `apply`, `createProfile`, `switchProfile` and `waitForStatus` for setup. Read the
   answer of a command with `invoke`; assert a failure with `invokeError`.
4. Click the real controls with `clickByLabel`, `clickByText`, `clickMenuItem`, `chooseSelect` and `clickTab`. Wait for
   text or a selector instead of sleeping. A controlled input needs `typeInto`; a WebDriver key that the list handles
   can use `pressKeyOn` or `pressKeyOnMod`.
5. Assert the disk effects with `expectFile`, `expectNoFile`, `isSymlink`, `linkCount`, `readText` and `zipNames`.
6. Always end with `cleanupFixture(id, fixture.root)`; it removes the game, its definition and its fixture roots. A
   failing assertion in the middle may leave its game for the artifact, but a passing test must not leak one.
7. Keep the wait timeout at the helpers' defaults. A test that needs more than the 180 s Mocha timeout is usually
   waiting on the wrong condition.

A failing test writes `target/e2e-artifacts/<time>-<title>/` with a screenshot, the DOM, local storage, the browser log
when the driver exposes one, the app log when it exists, the data paths and `error.txt`.

## README screenshots

`98-readme-screenshots.e2e.ts` builds a small demo game under `target/`, imports readable mod names and saves the images
the README embeds. The intentionally realistic game, icon and mod names live in `e2e/support/screenshot-data.ts`, so no
other spec depends on them. It is not part of the normal suite and never opens the real library:

```bash
E2E_SPECS=./specs/98-readme-screenshots.e2e.ts E2E_NODE=/usr/bin/node bun run e2e
```

The PNG files land in `target/readme-screenshots/`. Convert them to WebP before copying them to `docs/assets/screenshots/`:

```bash
ffmpeg -y -i target/readme-screenshots/mods.png -c:v libwebp -lossless 1 docs/assets/screenshots/mods.webp
```

Do the same for `apply-preview.png` and `conflicts.png`.

## Limits

- Native file/folder dialogs are answered by the seam above; the system picker itself is not exercised.
- Provider success paths (downloads, update checks, key validation) need network access and a key, so the suite covers
  their offline and missing-key errors only.
- The Linux overlay is applied and launched when the kernel allows unprivileged user namespaces; the test asserts the
  documented error codes when the container does not. The Windows hook and launcher are covered by their own Rust gates
  and the Windows E2E suite is still to be added.
- The suite uses one window and one app process. A test that needs a restart should reload the webview; the backend state
  and the Tauri session stay alive. The e2e build has no single-instance plugin, so `75-deep-links` emits the
  `deep-link://new-url` event that the plugin would forward; the OS-level hand-off from a second process needs a real
  installed build.
