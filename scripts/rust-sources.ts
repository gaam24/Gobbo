import { readdirSync, readFileSync } from "node:fs";
import { join, relative, sep } from "node:path";

/// The first inline test module; production code ends there.
const TEST_MODULE = /#\[cfg\(test\)\]/;

/// A Rust file with the text a checker reads: the production part or the whole file.
export interface RustSource {
	path: string;
	/// The path relative to the scanned root, with `/` separators on every system.
	name: string;
	/// The top-level module below the scanned root.
	module: string;
	text: string;
}

/// A path relative to `root`, with `/` separators on every system, for messages and module names.
export function portableName(root: string, path: string): string {
	return relative(root, path).replaceAll(sep, "/");
}

/// The Rust files below `dir`, without the `tests` folders. `skip` filters files by name inside the walk.
export function rustFiles(dir: string, skip: (name: string) => boolean = () => false): string[] {
	return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
		if (skip(entry.name)) return [];
		const path = join(dir, entry.name);
		if (entry.isDirectory()) return entry.name === "tests" ? [] : rustFiles(path, skip);
		return entry.name.endsWith(".rs") ? [path] : [];
	});
}

/// The production Rust files below `root`: the text ends at the first inline test module.
export function productionSources(root: string, skip: (name: string) => boolean = () => false): RustSource[] {
	return rustFiles(root, skip).map((path) => {
		const name = portableName(root, path);
		const source = readFileSync(path, "utf8");
		const at = source.search(TEST_MODULE);
		return {
			path,
			name,
			module: name.split("/")[0] ?? "",
			text: at === -1 ? source : source.slice(0, at),
		};
	});
}
