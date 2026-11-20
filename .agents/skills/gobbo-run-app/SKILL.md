---
name: gobbo-run-app
description: Run the real Gobbo window with the MCP bridge and inspect it.
---

## Read

The routing row "seeing or measuring the real window" in the root `AGENTS.md`, then
[docs/agent/run-app.md](../../../docs/agent/run-app.md) and load `mcp__tauri__*` tools.

## Do

Start Vite on 1420 in the background. Build with `TAURI_CONFIG="$(cat tauri.mcp.conf.json)" cargo build --features mcp-bridge`. Start `target/debug/gobbo` with `GOBBO_DATA_DIR` pointing at a scratch copy when you must not touch the real library. Wait for the bridge line, connect on 9223, check `typeof window.__TAURI__ === "object"`. Look and measure; do not apply, import or launch unless the task asks.

## Verify

Take a screenshot or read the accessibility tree. For a perf claim, record a before and after on the same machine. Stop the session, `pkill -x gobbo`, and stop Vite by its port.
