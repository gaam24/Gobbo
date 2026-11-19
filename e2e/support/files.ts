import { strict as assert } from "node:assert";
import { chmodSync, existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { strToU8, unzipSync, zipSync } from "fflate";

/// A file the suite writes. Every helper creates the parent folder, so a spec names the path and not the mkdir calls.
export function writeFile(file: string, content: string | Uint8Array, mode?: number): void {
	mkdirSync(path.dirname(file), { recursive: true });
	writeFileSync(file, content);
	if (mode !== undefined) chmodSync(file, mode);
}

export function writeJson(file: string, value: unknown): void {
	writeFile(file, `${JSON.stringify(value, null, 2)}\n`);
}

export function readJson<T>(file: string): T {
	return JSON.parse(readFileSync(file, "utf8")) as T;
}

export function readText(file: string): string {
	return readFileSync(file, "utf8");
}

export function exists(file: string): boolean {
	return existsSync(file);
}

export function isFile(file: string): boolean {
	try {
		return statSync(file).isFile();
	} catch {
		return false;
	}
}

export function listDir(dir: string): string[] {
	try {
		return readdirSync(dir).toSorted();
	} catch {
		return [];
	}
}

export function removeTree(target: string): void {
	// A failed overlay apply can leave `state/<profile>/.work/work` at mode 000. Make the tree writable before the
	// recursive remove, so a test cleanup never fails on the app's own temporary folder.
	const unlock = (entry: string): void => {
		try {
			const stat = lstatSync(entry);
			if (stat.isSymbolicLink()) return;
			if (stat.isDirectory()) {
				try {
					chmodSync(entry, 0o700);
				} catch {
					// A folder on a read-only mount is left for rmSync to report.
				}
				for (const child of readdirSync(entry)) unlock(`${entry}/${child}`);
			} else {
				try {
					chmodSync(entry, 0o600);
				} catch {
					// The file may be owned elsewhere; rmSync reports the real failure.
				}
			}
		} catch {
			// A path that vanished during the walk is already gone.
		}
	};
	unlock(target);
	rmSync(target, { recursive: true, force: true });
}

export function makeDir(dir: string): void {
	mkdirSync(dir, { recursive: true });
}

/// A ZIP with text files. Paths use `/`, as an archive does.
export function writeZip(file: string, entries: Record<string, string | Uint8Array>): void {
	const files: Record<string, Uint8Array> = {};
	for (const [name, content] of Object.entries(entries)) {
		files[name] = typeof content === "string" ? strToU8(content) : content;
	}
	writeFile(file, zipSync(files, { level: 6 }));
}

export function zipNames(file: string): string[] {
	return Object.keys(unzipSync(new Uint8Array(readFileSync(file)))).toSorted();
}

export function expectFile(file: string, message?: string): void {
	assert.ok(isFile(file), message ?? `expected the file ${file} to exist`);
}

export function expectNoFile(file: string, message?: string): void {
	assert.ok(!existsSync(file), message ?? `expected the file ${file} to be gone`);
}

export function expectText(file: string, needle: string): void {
	const text = readText(file);
	assert.ok(text.includes(needle), `expected ${file} to contain ${JSON.stringify(needle)}; got ${JSON.stringify(text.slice(0, 400))}`);
}
