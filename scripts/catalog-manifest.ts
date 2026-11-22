/// <reference types="bun" />
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";
import { parse, type TomlTable, type TomlValue } from "smol-toml";

// The definition catalog is the single file an app fetches before downloading individual game and loader
// definitions. It is generated, committed and guarded like `src/bindings.ts`. Every path is relative to the
// repository root with `/` separators, and every array is sorted by id so the output is byte-for-byte stable.

export interface CatalogGame {
	id: string;
	name: string;
	schema: number;
	loader?: string;
	verified: boolean;
	file: string;
	sha256: string;
	icon?: string;
	iconSha256?: string;
	minGobbo?: string;
}

export interface CatalogLoader {
	id: string;
	name: string;
	schema: number;
	file: string;
	sha256: string;
	minGobbo?: string;
}

export interface Catalog {
	apiVersion: number;
	games: CatalogGame[];
	loaders: CatalogLoader[];
}

/// A TOML table parses to a plain object; dates and arrays parse to objects too but are not tables here.
function isTable(value: TomlValue | undefined): value is TomlTable {
	return typeof value === "object" && !Array.isArray(value);
}

function requiredString(table: TomlTable, key: string, file: string): string {
	const value = table[key];

	if (typeof value !== "string") {
		throw new Error(`${file}: ${key} must be a string`);
	}

	return value;
}

function requiredNumber(table: TomlTable, key: string, file: string): number {
	const value = table[key];

	if (typeof value !== "number") {
		throw new Error(`${file}: ${key} must be a number`);
	}

	return value;
}

/// `min_gobbo` in a definition becomes `minGobbo` in its entry; an entry without a floor leaves the key out.
function minGobbo(table: TomlTable, file: string): { minGobbo?: string } {
	const value = table.min_gobbo;

	if (value === undefined) return {};
	if (typeof value !== "string") throw new Error(`${file}: min_gobbo must be a string`);
	return { minGobbo: value };
}

/// A game names a loader with either `"id"` or `{ id = "id", variant = "..." }`; only the id reaches the catalog.
function loaderId(table: TomlTable, file: string): string | undefined {
	const value = table.loader;

	if (value === undefined) return undefined;
	if (typeof value === "string") return value;
	if (isTable(value)) return requiredString(value, "id", file);
	throw new Error(`${file}: loader must be a string or a table with an id`);
}

function sha256(file: string): string {
	const hasher = new Bun.CryptoHasher("sha256");
	hasher.update(readFileSync(file));
	return hasher.digest("hex");
}

function byId(a: { id: string }, b: { id: string }): -1 | 0 | 1 {
	if (a.id === b.id) return 0;
	return a.id < b.id ? -1 : 1;
}

/// Reads every definition and builds the catalog. A mismatch with the folder/file name is a repository bug: fail with
/// the file in the message instead of publishing a broken catalog. The output depends only on the files, so rerunning
/// the generator produces no diff.
export function buildCatalog(root: string): Catalog {
	const gamesDir = join(root, "games");
	const loadersDir = join(root, "loaders");

	const games: CatalogGame[] = [];
	for (const entry of readdirSync(gamesDir, { withFileTypes: true })) {
		// `_template` is the folder a new definition is copied from; the Rust loader skips it too.
		if (!entry.isDirectory() || entry.name.startsWith("_")) continue;

		const id = entry.name;
		const file = join(gamesDir, id, "game.toml");
		const rel = `games/${id}/game.toml`;
		if (!existsSync(file)) throw new Error(`${rel}: missing game.toml`);

		const parsed = parse(readFileSync(file, "utf8"));
		const parsedId = requiredString(parsed, "id", rel);
		if (parsedId !== id) throw new Error(`${rel}: id ${parsedId} does not match folder ${id}`);

		const loader = loaderId(parsed, rel);
		const icon = join(gamesDir, id, "icon.webp");

		games.push({
			id,
			name: requiredString(parsed, "name", rel),
			schema: requiredNumber(parsed, "schema", rel),
			...(loader === undefined ? {} : { loader }),
			verified: isTable(parsed.verified),
			file: rel,
			sha256: sha256(file),
			...(existsSync(icon) ? { icon: `games/${id}/icon.webp`, iconSha256: sha256(icon) } : {}),
			...minGobbo(parsed, rel),
		});
	}
	games.sort(byId);

	const loaders: CatalogLoader[] = [];
	for (const entry of readdirSync(loadersDir, { withFileTypes: true })) {
		// README.md and anything else that is not a preset live beside the TOML files.
		if (!(entry.isFile() && entry.name.endsWith(".toml"))) continue;

		const id = entry.name.slice(0, -".toml".length);
		const rel = `loaders/${entry.name}`;
		const parsed = parse(readFileSync(join(loadersDir, entry.name), "utf8"));
		const parsedId = requiredString(parsed, "id", rel);
		if (parsedId !== id) throw new Error(`${rel}: id ${parsedId} does not match file name ${id}`);

		loaders.push({
			id,
			name: requiredString(parsed, "name", rel),
			schema: requiredNumber(parsed, "schema", rel),
			file: rel,
			sha256: sha256(join(loadersDir, entry.name)),
			...minGobbo(parsed, rel),
		});
	}
	loaders.sort(byId);

	return { apiVersion: 1, games, loaders };
}

export function serializeCatalog(catalog: Catalog): string {
	return `${JSON.stringify(catalog, null, 2)}\n`;
}

function fail(message: string): never {
	console.error(`catalog-manifest: ${message}`);
	process.exit(1);
}

function main(argv: string[]): void {
	const [flag, ...rest] = argv;
	const root = join(import.meta.dir, "..");

	try {
		if (flag === undefined && rest.length === 0) {
			const catalog = buildCatalog(root);
			writeFileSync(join(root, "catalog.json"), serializeCatalog(catalog));
			process.stdout.write(`Wrote catalog.json (${catalog.games.length} games, ${catalog.loaders.length} loaders)\n`);
		} else {
			fail("usage: catalog-manifest.ts");
		}
	} catch (error) {
		fail(error instanceof Error ? error.message : String(error));
	}
}

if (import.meta.main) {
	main(process.argv.slice(2));
}
