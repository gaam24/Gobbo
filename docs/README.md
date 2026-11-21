# Gobbo documentation

The [README](../README.md) is the front door: what Gobbo is, what it does and how to install it. This page is the map for everything else — pick the guide for whatever you're trying to do.

## Start here

| Guide                               | What it covers                                                       |
| ----------------------------------- | -------------------------------------------------------------------- |
| [Quick start](user/quickstart.md)   | Add a game, import a mod, apply it and undo it in about five minutes |
| [Install and data](user/install.md) | Downloads, checksums, requirements, first start and storage modes    |
| [Supported games](user/games.md)    | What's tested today and how to add your own game                     |

## For players

| Guide                                            | What it covers                                                                             |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------ |
| [Install and data](user/install.md)              | Download, checksums, requirements, first start, portable or global data, backups and logs. |
| [Quick start](user/quickstart.md)                | Add a game, import a mod, apply and undo it in five minutes.                               |
| [Supported games](user/games.md)                 | What is tested, what is ready for testing, how to add a game, and how mods reach the game. |
| [Linux and Steam](user/linux.md)                 | The Steam launch option, Proton, overlayfs and the limits of each method.                  |
| [Nexus Mods and Thunderstore](user/providers.md) | API keys, `nxm://` and `ror2mm://` links, downloads and update checks.                     |
| [Mod packs](user/packs.md)                       | Export, import, pack links, missing archives and what a pack records.                      |
| [Collections and modpacks](user/collections.md)  | Nexus collections and Thunderstore modpacks: preview, downloads, several in one profile.   |
| [System tray](user/tray.md)                      | The tray's status, quick actions and what closing the window does.                         |
| [Troubleshooting](user/troubleshooting.md)       | Common errors, the log, the support bundle and how to report a bug.                        |

## For contributors

| Guide                                               | What it covers                                                            |
| --------------------------------------------------- | ------------------------------------------------------------------------- |
| [Contributing](../.github/CONTRIBUTING.md)          | Setup, checks, code rules and pull requests.                              |
| [Open work](https://github.com/gaam24/Gobbo/issues) | Reported bugs, planned work and deliberately deferred ideas.              |
| [Repository layout](agent/layout.md)                | What lives where.                                                         |
| [Definition of done](agent/definition-of-done.md)   | What a change must include.                                               |
| [Quality reports](agent/quality.md)                 | Security, local reports and E2E.                                          |
| [Run the app](agent/run-app.md)                     | The MCP bridge, screenshots and performance checks.                       |
| [State machines](state-machines.md)                 | Game states, the primary action and long-running tasks.                   |
| [Collections architecture](agent/collections.md)    | The collection plan, the provider port, the cache and the security rules. |
| [Glossary](ux/glossary.md)                          | One label per concept, and the words the interface uses.                  |

## Architecture and data

| Guide                                                                         | What it covers                                                                                   |
| ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| [Platforms](../PLATFORMS.md)                                                  | The Windows and Linux differences, and what each platform supports.                              |
| [Design system](../DESIGN.md)                                                 | Tokens, typography, spacing, shape, motion and the component rules.                              |
| [Game definitions](../games/README.md)                                        | `game.toml`, slots, mirrors, full-root layouts, user data, steps, tools, the definition catalog and the supported games. |
| [Loader presets](../loaders/README.md)                                        | Shared layouts such as BepInEx.                                                                  |
| [Overlay hook architecture](../src-tauri/crates/overlay-hook/ARCHITECTURE.md) | The invariants, hook table, path model and diagnostics contract.                                 |
| [Backend module map](../src-tauri/docs/module-map.md)                         | Modules, layers and where a change belongs.                                                      |
| [Nexus client](../src-tauri/src/providers/nexus/README.md)                    | The API client, downloads and errors.                                                            |
| [UI components](../src/components/ui/README.md)                               | The generated shadcn layer and its allowed extensions.                                           |

## For agents

| Guide                                   | What it covers                                                |
| --------------------------------------- | ------------------------------------------------------------- |
| [Agent workspace](../.agents/README.md) | The skills and their guardrails.                              |
| [E2E](../e2e/README.md)                 | The real-window suite, its isolation and the test-only seams. |
| [Root AGENTS](../AGENTS.md)             | The project-wide rules.                                       |

## Documentation rules

- The README is the landing page. Task guides live in `docs/user/`; the README links to them instead of repeating them.
- One source of truth: the checks table is in [CONTRIBUTING](../.github/CONTRIBUTING.md), the full game table is in [games/README.md](../games/README.md), and the layout is in [agent/layout.md](agent/layout.md).
- User-facing copy uses the labels in the [glossary](ux/glossary.md): **Apply**, not deploy; **Keep the game folder clean**, with the overlay explained once; **Installer options**, with FOMOD in the technical details.
- English is the source language for the repository. The interface locales are separate.
- Use relative links for repository files, and put images in `docs/assets/` with alt text.
- Run `bun run docs:check` before a documentation change. It checks relative links, anchors, image paths and alt text; the weekly Security workflow reports it without blocking a merge.
- Plans are local and git-ignored. Do not link to `plans/` from a tracked document.
