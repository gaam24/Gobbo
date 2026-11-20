import { copyFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { browser } from "@wdio/globals";

import { appBinary, appEnvironment, applyProcessEnvironment, artifacts, dataDir, homeDir } from "./support/env";
import { makeDir, removeTree, writeJson } from "./support/files";
import { prepareFixtureFolders } from "./support/fixtures";

/// Every run starts from an empty library and an empty fake home. `target/` is ignored, so the fixtures never touch the
/// user's data. The first-run spec sees exactly the state a new install sees.
removeTree(dataDir);
removeTree(homeDir);
removeTree(artifacts);
makeDir(dataDir);
makeDir(homeDir);
makeDir(artifacts);
prepareFixtureFolders();
writeJson(path.join(dataDir, "settings.json"), {
	language: "en",
	inspector_collapsed: false,
	source_links: [],
	remember_window: true,
	efficiency_mode: "auto",
});
applyProcessEnvironment();

const defaultSpecs = [
	"./specs/00-first-run.e2e.ts",
	"./specs/10-games.e2e.ts",
	"./specs/15-commands.e2e.ts",
	"./specs/20-import.e2e.ts",
	"./specs/30-mods-list.e2e.ts",
	"./specs/35-scroll-stability.e2e.ts",
	"./specs/36-drag-virtual-list.e2e.ts",
	"./specs/40-profiles.e2e.ts",
	"./specs/50-deploy.e2e.ts",
	"./specs/60-launch.e2e.ts",
	"./specs/70-packs.e2e.ts",
	"./specs/75-deep-links.e2e.ts",
	"./specs/80-fomod.e2e.ts",
	"./specs/85-background-tasks.e2e.ts",
	"./specs/90-settings.e2e.ts",
	"./specs/95-ui.e2e.ts",
	"./specs/99-recovery.e2e.ts",
];

/// The pull-request suite: the flows that break most visibly, in an order that leaves the later specs a clean state.
const smokeSpecs = [
	"./specs/00-first-run.e2e.ts",
	"./specs/10-games.e2e.ts",
	"./specs/15-commands.e2e.ts",
	"./specs/20-import.e2e.ts",
	"./specs/30-mods-list.e2e.ts",
	"./specs/50-deploy.e2e.ts",
	"./specs/60-launch.e2e.ts",
];

/// `E2E_SPECS=./specs/20-import.e2e.ts,./specs/30-mods-list.e2e.ts` runs a subset while a new spec is being written.
/// `E2E_SMOKE=1` (or `bun run e2e:smoke`) runs the pull-request subset. The nightly job runs every spec.
const selected = process.env.E2E_SPECS?.split(",")
	.map((entry) => entry.trim())
	.filter((entry) => entry.length > 0);
const smoke = process.env.E2E_SMOKE === "1";

let specs = defaultSpecs;

if (selected && selected.length > 0) specs = selected;
else if (smoke) specs = smokeSpecs;

export const config = {
	runner: "local",
	specs,
	maxInstances: 1,
	services: [
		[
			"tauri",
			{
				appBinaryPath: appBinary,
				driverProvider: "embedded",
				startTimeout: 120_000,
				commandTimeout: 60_000,
				logLevel: "info",
				env: appEnvironment(),
			},
		],
	],
	capabilities: [
		{
			browserName: "tauri",
			"tauri:options": {
				application: appBinary,
			},
		},
	],
	logLevel: "info",
	// The full run collects every failure; the smoke job sets `E2E_BAIL=1` to stop at the first broken flow.
	bail: process.env.E2E_BAIL === "1" ? 1 : 0,
	waitforTimeout: 10_000,
	connectionRetryTimeout: 120_000,
	connectionRetryCount: 1,
	framework: "mocha",
	reporters: ["spec"],
	mochaOpts: {
		ui: "bdd",
		timeout: 180_000,
	},
	afterTest: async (test: { title: string }, _context: unknown, result: { error?: Error }) => {
		if (!result.error) return;
		const folder = path.join(artifacts, `${Date.now()}-${test.title.replaceAll(/[^a-z0-9]+/gi, "-").slice(0, 60)}`);
		mkdirSync(folder, { recursive: true });

		try {
			await browser.saveScreenshot(path.join(folder, "screen.png"));
		} catch (error) {
			writeFileSync(path.join(folder, "screenshot-error.txt"), String(error));
		}

		try {
			const logs = await browser.getLogs("browser");
			writeFileSync(path.join(folder, "browser.log"), JSON.stringify(logs, null, 2));
		} catch {
			// The Tauri driver does not always expose browser logs; the screenshot is still useful.
		}

		try {
			writeFileSync(path.join(folder, "dom.html"), await browser.execute(() => document.documentElement.outerHTML));
			writeFileSync(
				path.join(folder, "local-storage.json"),
				JSON.stringify(
					await browser.execute(() => {
						const values: Record<string, string | null> = {};

						for (const key of Object.keys(localStorage)) values[key] = localStorage.getItem(key);
						return values;
					}),
					null,
					2,
				),
			);
		} catch {
			// The window may be gone after a crash; the error file below says what failed.
		}

		writeFileSync(path.join(folder, "error.txt"), result.error.stack ?? result.error.message);
		writeFileSync(path.join(folder, "data.txt"), `GOBBO_DATA_DIR=${dataDir}\nHOME=${homeDir}\n`);

		if (existsSync(path.join(dataDir, "gobbo.log"))) {
			copyFileSync(path.join(dataDir, "gobbo.log"), path.join(folder, "gobbo.log"));
		}
	},
};
