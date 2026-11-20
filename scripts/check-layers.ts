/// <reference types="bun" />
import { join } from "node:path";
import { productionSources } from "./rust-sources";

const ROOT = join(import.meta.dir, "../src-tauri/src");

/// The layers below the feature modules, in the order of `src-tauri/AGENTS.md`. A file may use its own layer and every
/// layer below it; a feature module is above all of them and is never imported from here.
const LAYERS = ["error", "domain", "infra", "platform", "providers", "context"] as const;
type Layer = (typeof LAYERS)[number];

/// Modules a layer may use although they are not below it: the context reads the game definitions and runs the
/// file-layout migrations.
const EXTRA: Record<Layer, readonly string[]> = {
	error: [],
	domain: [],
	infra: [],
	platform: [],
	providers: [],
	context: ["games", "migrations"],
};

export const layerOf = (module: string): Layer | undefined => LAYERS.find((layer) => layer === module);

/// A reference to a top-level crate module, for example `crate::domain`.
const CRATE_REF = /\bcrate::([a-z_]+)/g;

export const allowed = (layer: Layer, target: string): boolean => {
	if (EXTRA[layer].includes(target)) return true;
	const targetLayer = layerOf(target);
	return targetLayer !== undefined && LAYERS.indexOf(targetLayer) <= LAYERS.indexOf(layer);
};

/// Every low-layer file that names a feature module or a higher layer.
export function layerProblems(root: string): string[] {
	const problems: string[] = [];

	for (const { name, module, text } of productionSources(root, (file) => file === "tests.rs")) {
		const layer = layerOf(module);

		if (layer === undefined) continue;

		for (const match of text.matchAll(CRATE_REF)) {
			const target = match[1];

			if (target !== undefined && !allowed(layer, target)) {
				problems.push(`${name}: ${layer} must not use crate::${target}`);
			}
		}
	}

	return problems;
}

if (import.meta.main) {
	const problems = layerProblems(ROOT);

	if (problems.length > 0) {
		process.stderr.write(`${problems.join("\n")}\n`);
		process.exit(1);
	}

	process.stdout.write("Layer check passed.\n");
}
