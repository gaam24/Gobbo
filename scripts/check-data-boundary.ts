import { existsSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { productionSources } from "./rust-sources";

const ROOT = join(import.meta.dir, "..", "src-tauri", "src");
const PROVIDERS = join(ROOT, "providers");
const GAME_BRANCH = /\b(game_id|game\.id|loader)\s*==\s*"/;
const PROVIDER_ID = /pub const ID:\s*&str\s*=\s*"([^"]+)"/;
const LOW_LAYERS: ReadonlySet<string> = new Set(["domain", "infra", "platform", "context"]);

/// The ids are owned by the adapter modules. Reading them here means a new provider tightens this guard without a
/// second edit, just as the game folders own the ids in `check-generated.ts`.
const providerNames = readdirSync(PROVIDERS, { withFileTypes: true })
	.flatMap((entry) => {
		if (!entry.isDirectory()) return [];
		const module = join(PROVIDERS, entry.name, "mod.rs");
		if (!existsSync(module)) return [];
		const id = PROVIDER_ID.exec(readFileSync(module, "utf8"))?.[1];
		return id === undefined ? [] : [id];
	})
	.toSorted();
const PROVIDER_NAME = providerNames.length === 0 ? null : new RegExp(`"(${providerNames.join("|")})"`);

function isTestOnly(name: string): boolean {
	return name === "tests.rs" || name === "test_support.rs";
}

/// A game or loader id in a branch, or a provider name in a layer that must not know one. The data folders own those
/// names; a lower layer receives values, never a literal that selects one.
export function boundaryProblems(root: string): string[] {
	const problems: string[] = [];
	for (const { name, module, text } of productionSources(root, isTestOnly)) {
		if (module === "games" || module === "providers") continue;
		if (GAME_BRANCH.test(text)) {
			problems.push(`${name}: branches on a game or loader id`);
		}
		if (LOW_LAYERS.has(module) && PROVIDER_NAME?.test(text)) {
			problems.push(`${name}: names a provider in the ${module} layer`);
		}
	}
	return problems;
}

if (import.meta.main) {
	const problems = boundaryProblems(ROOT);
	if (problems.length > 0) {
		process.stderr.write(`${problems.join("\n")}\n`);
		process.exit(1);
	}
	process.stdout.write("Data boundary passed.\n");
}
