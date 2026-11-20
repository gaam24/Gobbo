import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

/// The E2E build reads test-only globals; a production build must not contain them. Vite replaces `import.meta.env.MODE`
/// at build time, so this check catches a regression that keeps a test seam in the release bundle.
const root = join(import.meta.dir, "..");
const assets = join(root, "dist", "assets");
const markers = ["__GOBBO_E2E_DIALOGS__", "__GOBBO_E2E_THROW__", "gobbo.e2e.throw", "__GOBBO_E2E_TASKS__"];

if (!existsSync(assets)) {
	process.stdout.write("No dist build; E2E hook check skipped.\n");
	process.exit(0);
}

const hits: string[] = [];

for (const name of readdirSync(assets)) {
	const path = join(assets, name);

	if (!/\.(?:js|css|html)$/.test(name)) continue;
	const text = readFileSync(path, "utf8");

	for (const marker of markers) {
		if (text.includes(marker)) hits.push(`${name}: ${marker}`);
	}
}

if (hits.length > 0) {
	console.error("Production bundle contains E2E test hooks:");

	for (const hit of hits) console.error(`  ${hit}`);
	process.exit(1);
}

process.stdout.write("Production bundle has no E2E test hooks.\n");
