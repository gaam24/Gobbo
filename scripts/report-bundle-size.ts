import { readFileSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";

/// Reports the first paint of the Tauri window: `index.html` and every module it preloads. A lazy screen is not in this
/// list, which is the point of the lazy imports. This is a measurement, not a gate: a size change is reviewed, never
/// blocked by a number that has to be raised on every change. Run it after `bun run build:web`.
const ROOT = join(import.meta.dir, "..");
const html = readFileSync(join(ROOT, "dist/index.html"), "utf8");
const assets = [
	...new Set(
		[...html.matchAll(/(?:src|href)="(\/assets\/[^"]+)"/g)].map((match) => match[1]).filter((path): path is string => Boolean(path)),
	),
];

let raw = 0;
let gzip = 0;
const rows: Array<{ path: string; gzip: number }> = [];
for (const path of assets) {
	const file = readFileSync(join(ROOT, "dist", path.replace(/^\//, "")));
	const bytes = gzipSync(file, { level: 9 }).length;
	raw += file.length;
	gzip += bytes;
	rows.push({ path, gzip: bytes });
}
rows.sort((left, right) => right.gzip - left.gzip);
for (const row of rows.slice(0, 10)) {
	process.stdout.write(`  ${(row.gzip / 1024).toFixed(1).padStart(6)} kB gz  ${row.path}\n`);
}
process.stdout.write(`First paint: ${assets.length} assets, ${(raw / 1024).toFixed(1)} kB raw, ${(gzip / 1024).toFixed(1)} kB gzip.\n`);
