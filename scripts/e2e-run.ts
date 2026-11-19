import { spawnSync } from "node:child_process";
import process from "node:process";

const run = (node: string, args: string[]) => spawnSync(node, args, { encoding: "utf8", stdio: "inherit" });

// `bun run e2e:smoke` selects the short suite without shell-specific environment syntax, so it works on Windows too.
if (process.argv.includes("--smoke")) process.env.E2E_SMOKE = "1";

// WebdriverIO's expect-webdriverio does not run under Bun: the worker dies on
// `The superclass is not a constructor`. Use a real Node.js for the suite. The
// `E2E_NODE` override is for a machine whose `node` is a Bun shim.
const candidates = [process.env.E2E_NODE, "node", "/usr/bin/node"].filter(
	(candidate): candidate is string => candidate !== undefined && candidate.length > 0,
);

for (const candidate of candidates) {
	const version = spawnSync(candidate, ["--version"], { encoding: "utf8" });
	if (version.status === 0 && /^v\d+\./.test(version.stdout.trim())) {
		const result = run(candidate, ["node_modules/@wdio/cli/bin/wdio.js", "run", "e2e/wdio.conf.ts"]);
		process.exit(result.status ?? 1);
	}
}

console.error("E2E needs a real Node.js (not Bun). Install Node 20+ or set E2E_NODE to its executable.");
process.exit(1);
