import { join } from "node:path";
import { productionSources } from "./rust-sources";

const ROOT = join(import.meta.dir, "..", "src-tauri", "src");
const GAME_BRANCH = /\b(game_id|game\.id|loader)\s*==\s*"/;
const PROVIDER_NAME = /"(nexus|thunderstore)"/;
const LOW_LAYERS: ReadonlySet<string> = new Set(["domain", "infra", "platform", "context"]);

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
		if (LOW_LAYERS.has(module) && PROVIDER_NAME.test(text)) {
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
