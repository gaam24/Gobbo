import type { Status_Serialize } from "../src/bindings";
import { listItems, matchesNeedle, statusItems, tableItems, toRows } from "../src/lib/mods";
import { statusFixture } from "../src/test/fixtures/status";

/// A synthetic mod with the fields the list pipeline reads. The status fixture supplies the rest of the game state.
function makeMod(index: number): Status_Serialize["mods"][number] {
	return {
		id: `local:mod-${index}`,
		folder: `Mod ${index}`,
		fomod_installer: false,
		version_count: 1,
		installed_at: 0,
		slot: "Mods",
		enabled: index % 2 === 0,
		priority: index + 1,
		load_priority: null,
		files: 10,
		size_kb: 128,
		wins: index % 11 === 0 ? 1 : 0,
		loses: index % 13 === 0 ? 1 : 0,
		archive: null,
		archive_file: null,
		skipped: 0,
		info: { name: `Mod ${index}`, version: "1.0", author: "Author", description: null },
		source: null,
		sources: [],
		page_id: null,
		page_name: null,
		part_id: null,
		part_name: null,
		part_category: null,
		update: null,
		tags: index % 5 === 0 ? ["core"] : [],
		instructions: "",
		unmet_requirements: index % 7 === 0 ? 1 : 0,
		requires_names: [],
		requirement_groups: [],
		profiles: ["Default"],
		collections: [],
	};
}

function percentile(sorted: number[], fraction: number): number {
	const at = Math.min(sorted.length - 1, Math.max(0, Math.round((sorted.length - 1) * fraction)));
	return sorted[at] ?? 0;
}

function measure(name: string, run: () => void): void {
	for (let warmup = 0; warmup < 5; warmup += 1) run();
	const times: number[] = [];

	for (let index = 0; index < 100; index += 1) {
		const start = performance.now();
		run();
		times.push(performance.now() - start);
	}

	times.sort((left, right) => left - right);
	const p50 = percentile(times, 0.5);
	const p95 = percentile(times, 0.95);
	const max = times.at(-1) ?? 0;
	process.stdout.write(`${name.padEnd(28)} p50=${p50.toFixed(3)} ms  p95=${p95.toFixed(3)} ms  max=${max.toFixed(3)} ms\n`);
}

const count = Number(process.argv[2] ?? 1000);
const mods = Array.from({ length: count }, (_, index) => makeMod(index));
const status = statusFixture({ kind: "ready", enabled: 1, total: 1 }, { mods, separators: [] });
const updates = {};

process.stdout.write(`mods=${count}\n`);
measure("filter all", () => mods.filter((mod) => matchesNeedle(mod, "all", "", null, updates)));
measure("filter enabled+query", () => mods.filter((mod) => matchesNeedle(mod, "enabled", "mod 4", null, updates)));
measure("statusItems", () => statusItems(status));
measure("listItems", () => listItems(mods, []));
measure("tableItems", () => tableItems(statusItems(status)));
measure("statusItems+toRows", () => toRows(statusItems(status)));
const selection = mods.filter((_, index) => index % 3 === 0).map((mod) => mod.id);
measure("batch selection", () => {
	const selected = new Set(selection);
	status.mods.filter((mod) => selected.has(mod.id));
});
