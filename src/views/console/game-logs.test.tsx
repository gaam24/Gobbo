import { expect, test } from "bun:test";
import { renderToString } from "react-dom/server";

import type { RunReport_Serialize } from "@/bindings";
import { applyLanguage } from "@/i18n";
import { GameLogs } from "@/views/console/game-logs";

// The app's own setup touches the document, so the test starts a plain English instance.
await applyLanguage("en");

const report: RunReport_Serialize = {
	started_at: 1,
	finished_at: 2,
	game_started: true,
	game_exit_code: 1,
	stopped_by_user: false,
	launch_ms: null,
	launch_steps: null,
	steps: [],
	output: [],
	game_logs: [
		{
			kind: "script",
			path: "logs/script/runtime_CURRENT.log",
			lines: ["Script compilation has failed.", "- Example Mod"],
			modified_at: 1_794_660_000,
		},
	],
	mod_logs: [
		{
			kind: "mod",
			path: "logs/loader/loader-1.log",
			lines: ["plugin loaded"],
			modified_at: 1_794_660_001,
		},
	],
	app_log: ["launch: game=example-game overlay=true"],
	game_changes: [],
	game_changes_total: 0,
	decisions: [],
	decisions_total: 0,
	crash_dump: null,
	dlls: null,
};

test("the log tab lists every file and opens the selected log beside it", () => {
	const html = renderToString(<GameLogs report={report} />);
	expect(html).toContain("launch: game=example-game overlay=true");
	expect(html).toContain("runtime_CURRENT.log");
	expect(html).toContain("loader-1.log");
	expect(html).toContain("Mods and runtime");
	expect(html).toContain("gobbo.log");
});
