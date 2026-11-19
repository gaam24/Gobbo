# Run the app with the MCP bridge

Use this when a change must be seen or measured in the real window, not only in tests. On Linux the window is
WebKitGTK; on Windows it is WebView2. The Tauri MCP tools (`mcp__tauri__*`) drive the window through the
`tauri-plugin-mcp-bridge` plugin on port 9223. The plugin exists only behind the `mcp-bridge` feature in debug builds.

The steps below are the ones that worked. An earlier attempt failed on every `webview_*` call with
`Request timeout after 2000ms`; the cause was a build without `src-tauri/tauri.mcp.conf.json`, which the steps below
prevent.

### Rules

- The bridge is debug-only. Never add it to a release build or to `tauri.conf.json`. `tauri.mcp.conf.json` is tracked
  on purpose: it is the separate debug config, used only by an explicit `TAURI_CONFIG=$(cat ...)` build.
- The app opens the user's real library. Look, read and measure. Do not launch a game, deploy or apply mods, delete,
  import, check for updates or change settings unless the user asks for that action.
- Put ad-hoc screenshots and logs in the session's scratch folder, never in the repository. The README screenshots in
  `docs/assets/screenshots/` are the exception; regenerate them with the E2E screenshot spec described in
  [`e2e/README.md`](../../e2e/README.md).

### 1. Load the tools first

The `mcp__tauri__*` tools are deferred: calling one before loading it fails. Load them in one call:

```text
ToolSearch query: select:mcp__tauri__driver_session,mcp__tauri__webview_execute_js,mcp__tauri__webview_screenshot,mcp__tauri__webview_dom_snapshot,mcp__tauri__manage_window,mcp__tauri__read_logs
```

If the `tauri` server still shows as connecting, search again after a moment.

### 2. Start the frontend dev server (port 1420)

From the repository root. Check the port first; the app loads its UI from it:

```bash
ss -ltn | grep -q ':1420 ' && echo "port 1420 is busy" || bun run dev > "$LOGS/vite.log" 2>&1 &
```

Run `bun run dev` in the background and keep the process. Set the log folder once per session, outside the repository:

```bash
LOGS=$(mktemp -d)
```

### 3. Build the debug app with the bridge config

From `src-tauri`, always with the MCP config in the environment:

```bash
cd src-tauri && TAURI_CONFIG="$(cat tauri.mcp.conf.json)" cargo build --features mcp-bridge
```

`tauri.mcp.conf.json` turns on `withGlobalTauri` and adds the `mcp-bridge` capability. The bridge returns results
through `window.__TAURI__`, so without this config every query times out. A plain `cargo build` in the same
`target/debug` folder replaces the binary without the config; build with the variable again before you start the app.
After a Rust change, build again and restart the app.

### 4. Start the app and check the bridge

```bash
cd src-tauri && ./target/debug/gobbo > "$LOGS/app.log" 2>&1 &
```

Wait about 15 seconds, then confirm the bridge listens:

```bash
grep "WebSocket server listening" "$LOGS/app.log"
```

The expected line names `127.0.0.1:9223`. If it is missing, read `$LOGS/app.log`; another `gobbo` may be running
(`pgrep -x gobbo`).

### 5. Connect

```text
mcp__tauri__driver_session  action: "start"  port: 9223
```

Check the connection with a query that returns the window state:

```text
mcp__tauri__webview_execute_js  script: (() => ({ tauri: typeof window.__TAURI__, title: document.title, rows: document.querySelectorAll('[data-mod]').length }))()
```

`tauri` must be `"object"`. If you rebuilt or restarted the app, call `driver_session` with `action: "stop"` first and
start it again.

### 6. Look and measure

- Screenshot: `mcp__tauri__webview_screenshot` with `format: "png"` and a `filePath` in the scratch folder, then open
  the file with `Read`. Use `mcp__tauri__manage_window` with `action: "focus"` if the capture times out.
- Labels and roles: `mcp__tauri__webview_dom_snapshot` with `type: "accessibility"`.
- Frame timing: start a `requestAnimationFrame` loop that writes its result to `window.__perf`, then read
  `window.__perf` in a later `webview_execute_js` call. A script's promise does not come back in the same call, so
  read the global. Report the p50, p95, the worst frame and the number of frames above 33 ms.
- Scrolling by setting `scrollTop` is not real input. Say so when you report it.
- A change to the frontend is picked up by Vite; if the page does not show it, run `location.reload()` through
  `webview_execute_js`, wait about 3 seconds, and check the bridge again. If the bridge stops answering, restart the app.

### 7. Stop everything

```text
mcp__tauri__driver_session  action: "stop"
```

```bash
pkill -x gobbo
```

Use `pkill -x`, the exact name. `pkill -f gobbo` matches your own shell and ends it. Stop Vite by its port:

```bash
kill "$(ss -ltnp | grep ':1420 ' | grep -o 'pid=[0-9]*' | cut -d= -f2)"
```

### Troubleshooting

- Every `webview_*` call times out after 2000 ms: the app was built without `TAURI_CONFIG`, or it is not running.
  Rebuild with the config, restart the app, then `driver_session` stop and start.
- Screenshot fails with a native timeout: the window is hidden or unfocused. Focus it with `manage_window`.
- A blank window: read `$LOGS/vite.log` for a dev server error, and check that port 1420 serves the app.
- On KDE Wayland, `grim` does not work; `spectacle -b -n -f -o <file>` takes a fullscreen screenshot.

### How an agent uses it

1. Make the change. Run `bun run lint` and `bun run test`.
2. Load the tools (step 1), start Vite (step 2), build with the config (step 3) and start the app (step 4).
3. Connect and check `window.__TAURI__` (step 5).
4. Look at the screens the change touches. For a smoothness claim, measure before and after; if there is no
   "before", say that the numbers have no baseline.
5. Stop everything (step 7). In the report, name what you saw, what you measured, and what you could not check.
