import { existsSync, readdirSync, readFileSync, renameSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, extname, join, relative, resolve } from "node:path";
import process from "node:process";

import sharp from "sharp";

// Normalizes and checks the game icons in `games/<id>/icon.webp`.
//
// The standard is one 128x128 WebP per game: lossy quality 85, alpha quality 100, method 6, sharp YUV, sRGB, no
// metadata, at most 8 KB (12 KB is the hard limit). The interface shows a game icon at 32 CSS px at most, so 128 px
// covers a 4x display; a larger file only adds installer size and decoded memory.
//
// Usage:
//   bun scripts/icons.ts                              Normalize every `games/*/icon.webp` in place; files that already
//                                                    meet the standard are left alone.
//   bun scripts/icons.ts --game the-witcher-3-wild-hunt   Normalize one game icon in place.
//   bun scripts/icons.ts --source <dir>               Read `<dir>/<game-id>.{png,webp,jpg,jpeg,ico}` for every game.
//   bun scripts/icons.ts --game valheim --source <file>    Convert one source file.
//   bun scripts/icons.ts --map <file.json>            Map game ids to source files, for downloads with hashed names.
//   bun scripts/icons.ts --check                      Verify the whole set; writes nothing.
//
// `--map` is a JSON object of game id to source path, relative to the map file or absolute. A source may be an ICO;
// the largest frame is used. The script needs `sharp` from `bun install`.

const ROOT = resolve(import.meta.dirname, "..");
const GAMES = join(ROOT, "games");
const ICON = "icon.webp";
const ICON_SIZE = 128;
const QUALITY = 85;
const QUALITIES = [QUALITY, 80, 75, 70];
const TARGET_BYTES = 8 * 1024;
const HARD_MAX_BYTES = 12 * 1024;
const SOURCE_EXTENSIONS: readonly string[] = [".png", ".webp", ".jpg", ".jpeg", ".ico"];

interface Options {
	check: boolean;
	dryRun: boolean;
	game?: string;
	source?: string;
	map?: string;
}

interface GameFolder {
	id: string;
	dir: string;
}

interface IcoEntry {
	width: number;
	height: number;
	bytes: number;
	offset: number;
	bpp: number;
	kind: "png" | "bmp";
}

interface RawImage {
	data: Buffer;
	width: number;
	height: number;
}

interface Conversion {
	game: string;
	source: string;
	bytes: number;
	quality: number;
}

interface CheckRow {
	game: string;
	bytes: number;
	width: number;
	height: number;
	problems: string[];
	warnings: string[];
}

function usage(): string {
	return [
		"Usage: bun scripts/icons.ts [--check] [--dry-run] [--game <id>] [--source <dir|file>] [--map <file.json>]",
		"",
		"  (no options)        normalize every games/<id>/icon.webp in place",
		"  --game <id>         only this game",
		"  --source <dir>      read <dir>/<game-id>.{png,webp,jpg,jpeg,ico}",
		"  --source <file>     with --game, convert this one file",
		"  --map <file.json>   game id to source path; relative paths use the map folder",
		"  --check             verify the set and write nothing",
		"  --dry-run           encode but do not write",
		"",
	].join("\n");
}

function valueAfter(args: string[], index: number, flag: string): { value: string; next: number } {
	const value = args[index + 1];

	if (value === undefined || value.startsWith("--")) throw new Error(`${flag} needs a value`);
	return { value, next: index + 1 };
}

function parseArgs(args: string[]): Options {
	const options: Options = { check: false, dryRun: false };

	for (let i = 0; i < args.length; i++) {
		const arg = args[i] ?? "";

		if (arg === "--check") {
			options.check = true;
		} else if (arg === "--dry-run") {
			options.dryRun = true;
		} else if (arg === "--game") {
			const parsed = valueAfter(args, i, "--game");
			options.game = parsed.value;
			i = parsed.next;
		} else if (arg === "--source") {
			const parsed = valueAfter(args, i, "--source");
			options.source = parsed.value;
			i = parsed.next;
		} else if (arg === "--map") {
			const parsed = valueAfter(args, i, "--map");
			options.map = parsed.value;
			i = parsed.next;
		} else if (arg === "--help" || arg === "-h") {
			process.stdout.write(usage());
			process.exit(0);
		} else {
			throw new Error(`unknown option ${arg}`);
		}
	}

	if (options.check && (options.source !== undefined || options.map !== undefined)) {
		throw new Error("--check does not take --source or --map");
	}

	if (options.check && options.dryRun) throw new Error("--check already writes nothing");

	if (options.source !== undefined && options.map !== undefined) {
		throw new Error("use either --source or --map, not both");
	}

	return options;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function formatBytes(bytes: number): string {
	return `${(bytes / 1024).toFixed(1)} KB`;
}

function gameFolders(): GameFolder[] {
	const folders: GameFolder[] = [];

	for (const entry of readdirSync(GAMES, { withFileTypes: true })) {
		if (!entry.isDirectory() || entry.name.startsWith("_")) continue;
		const dir = join(GAMES, entry.name);

		if (!existsSync(join(dir, "game.toml"))) continue;
		folders.push({ id: entry.name, dir });
	}

	folders.sort((a, b) => a.id.localeCompare(b.id));
	return folders;
}

function readMap(path: string): Map<string, string> {
	const absolute = resolve(path);
	const parsed: unknown = JSON.parse(readFileSync(absolute, "utf8"));

	if (!isRecord(parsed)) throw new Error(`${path}: the map must be a JSON object`);
	const map = new Map<string, string>();

	for (const [game, value] of Object.entries(parsed)) {
		if (typeof value !== "string") throw new Error(`${path}: ${game} must be a string`);
		map.set(game, resolve(dirname(absolute), value));
	}

	return map;
}

function withoutExtension(name: string): string {
	const ext = extname(name);
	return ext.length === 0 ? name : name.slice(0, -ext.length);
}

function sourceIn(dir: string, id: string): string | undefined {
	const matches: string[] = [];

	for (const entry of readdirSync(dir, { withFileTypes: true })) {
		if (!entry.isFile()) continue;
		const ext = extname(entry.name).toLowerCase();

		if (!SOURCE_EXTENSIONS.includes(ext)) continue;

		if (withoutExtension(entry.name).toLowerCase() !== id.toLowerCase()) continue;
		matches.push(entry.name);
	}

	matches.sort((a, b) => SOURCE_EXTENSIONS.indexOf(extname(a).toLowerCase()) - SOURCE_EXTENSIONS.indexOf(extname(b).toLowerCase()));
	const first = matches[0];
	return first === undefined ? undefined : join(dir, first);
}

function sourceFor(game: GameFolder, options: Options, map: Map<string, string> | undefined): string | undefined {
	if (map !== undefined) return map.get(game.id);

	if (options.source !== undefined) {
		if (options.game !== undefined && statSync(options.source).isFile()) return options.source;
		return sourceIn(options.source, game.id);
	}

	const icon = join(game.dir, ICON);
	return existsSync(icon) ? icon : undefined;
}

// ICO and its DIB frames. Sharp does not read ICO, so the largest frame is extracted and handed to sharp as a PNG.
function pngSize(data: Buffer): { width: number; height: number } | null {
	if (data.length < 24) return null;

	if (data.readUInt32BE(0) !== 0x89504e47 || data.readUInt32BE(4) !== 0x0d0a1a0a) return null;

	if (data.toString("ascii", 12, 16) !== "IHDR") return null;
	return { width: data.readUInt32BE(16), height: data.readUInt32BE(20) };
}

function dibSize(data: Buffer): { width: number; height: number; bpp: number } | null {
	if (data.length < 16) return null;
	const header = data.readUInt32LE(0);

	if (header < 40 || data.length < header) return null;
	const width = data.readInt32LE(4);
	const stored = data.readInt32LE(8);
	const height = Math.abs(stored) / 2;

	if (width <= 0 || height <= 0 || !Number.isInteger(height)) return null;
	return { width, height, bpp: data.readUInt16LE(14) };
}

function icoEntries(buffer: Buffer): IcoEntry[] {
	if (buffer.length < 6 || buffer.readUInt16LE(0) !== 0 || buffer.readUInt16LE(2) !== 1) {
		throw new Error("not an ICO file");
	}

	const count = buffer.readUInt16LE(4);
	const entries: IcoEntry[] = [];

	for (let i = 0; i < count; i++) {
		const at = 6 + i * 16;

		if (at + 16 > buffer.length) break;
		const bytes = buffer.readUInt32LE(at + 8);
		const offset = buffer.readUInt32LE(at + 12);

		if (offset + bytes > buffer.length) continue;
		const data = buffer.subarray(offset, offset + bytes);
		const png = pngSize(data);

		if (png !== null) {
			entries.push({ ...png, bytes, offset, bpp: 32, kind: "png" });
			continue;
		}

		const dib = dibSize(data);

		if (dib !== null) entries.push({ ...dib, bytes, offset, kind: "bmp" });
	}

	return entries;
}

function bestEntry(entries: IcoEntry[]): IcoEntry {
	let best = entries[0];

	if (best === undefined) throw new Error("the ICO has no readable frame");

	for (const entry of entries) {
		const area = entry.width * entry.height;
		const bestArea = best.width * best.height;

		if (area > bestArea || (area === bestArea && entry.bytes > best.bytes)) best = entry;
	}

	return best;
}

interface DibLayout {
	width: number;
	height: number;
	bpp: number;
	topDown: boolean;
	pixels: number;
	xorStride: number;
	maskAt: number;
	maskStride: number;
	hasMask: boolean;
}

function dibLayout(dib: Buffer): DibLayout {
	const header = dib.readUInt32LE(0);
	const width = dib.readInt32LE(4);
	const stored = dib.readInt32LE(8);
	const height = Math.abs(stored) / 2;
	const bpp = dib.readUInt16LE(14);
	const compression = dib.readUInt32LE(16);

	if (!Number.isInteger(height) || width <= 0 || height <= 0) throw new Error("invalid DIB size");

	if (compression !== 0) throw new Error(`unsupported DIB compression ${compression}`);

	if (bpp !== 32 && bpp !== 24) throw new Error(`unsupported DIB bit count ${bpp}`);
	const xorStride = (((width * bpp + 31) >> 5) << 2) >>> 0;
	const maskStride = (((width + 31) >> 5) << 2) >>> 0;
	const maskAt = header + xorStride * height;
	return {
		width,
		height,
		bpp,
		topDown: stored < 0,
		pixels: header,
		xorStride,
		maskAt,
		maskStride,
		hasMask: maskAt + maskStride * height <= dib.length,
	};
}

function isMasked(dib: Buffer, layout: DibLayout, row: number, x: number): boolean {
	if (!layout.hasMask) return false;
	const at = layout.maskAt + row * layout.maskStride + (x >> 3);
	return (((dib[at] ?? 0) >> (7 - (x & 7))) & 1) === 1;
}

function decodeDib(dib: Buffer): RawImage {
	const layout = dibLayout(dib);
	const { width, height, bpp } = layout;
	const out = Buffer.alloc(width * height * 4);
	let anyAlpha = false;

	for (let y = 0; y < height; y++) {
		const row = layout.topDown ? y : height - 1 - y;
		const pixelRow = layout.pixels + row * layout.xorStride;

		for (let x = 0; x < width; x++) {
			const at = (y * width + x) * 4;
			const p = pixelRow + x * (bpp / 8);
			out[at] = dib[p + 2] ?? 0;
			out[at + 1] = dib[p + 1] ?? 0;
			out[at + 2] = dib[p] ?? 0;
			const masked = isMasked(dib, layout, row, x);

			if (bpp === 32) {
				const alpha = dib[p + 3] ?? 0;

				if (alpha !== 0) anyAlpha = true;
				out[at + 3] = masked ? 0 : alpha;
			} else {
				out[at + 3] = masked ? 0 : 255;
			}
		}
	}

	if (bpp === 32 && !anyAlpha && !layout.hasMask) {
		for (let at = 3; at < out.length; at += 4) out[at] = 255;
	}

	return { data: out, width, height };
}

async function readSource(path: string): Promise<Buffer> {
	const bytes = readFileSync(path);

	if (extname(path).toLowerCase() !== ".ico") return bytes;
	const best = bestEntry(icoEntries(bytes));
	const data = bytes.subarray(best.offset, best.offset + best.bytes);

	if (best.kind === "png") return data;
	const raw = decodeDib(data);
	return await sharp(raw.data, { raw: { width: raw.width, height: raw.height, channels: 4 } })
		.png()
		.toBuffer();
}

async function convertOne(game: GameFolder, source: string, dryRun: boolean): Promise<Conversion> {
	const input = await readSource(source);
	const base = sharp(input, { failOn: "error" })
		.resize(ICON_SIZE, ICON_SIZE, { fit: "cover", position: "centre", kernel: "lanczos3" })
		.toColourspace("srgb");

	let output: Buffer | undefined;
	let quality = QUALITY;

	for (const candidate of QUALITIES) {
		output = await base.clone().webp({ quality: candidate, alphaQuality: 100, effort: 6, smartSubsample: true }).toBuffer();
		quality = candidate;

		if (output.length <= TARGET_BYTES) break;
	}

	if (output === undefined) throw new Error(`${game.id}: the encoder produced nothing`);

	if (output.length > HARD_MAX_BYTES) {
		throw new Error(
			`${game.id}: ${formatBytes(output.length)} is above the ${formatBytes(HARD_MAX_BYTES)} hard limit even at quality ${quality}`,
		);
	}

	const meta = await sharp(output).metadata();

	if (meta.format !== "webp" || meta.width !== ICON_SIZE || meta.height !== ICON_SIZE) {
		throw new Error(`${game.id}: the encoder produced ${meta.format} ${meta.width}x${meta.height}`);
	}

	if (output.length > TARGET_BYTES) {
		console.warn(`${game.id}: ${formatBytes(output.length)} is above the ${formatBytes(TARGET_BYTES)} target`);
	}

	if (!dryRun) {
		const dest = join(game.dir, ICON);
		const temp = `${dest}.tmp`;
		writeFileSync(temp, output);
		renameSync(temp, dest);
	}

	return { game: game.id, source, bytes: output.length, quality };
}

function webpSize(bytes: Buffer): { width: number; height: number } | null {
	if (bytes.length < 30 || bytes.toString("ascii", 0, 4) !== "RIFF" || bytes.toString("ascii", 8, 12) !== "WEBP") return null;
	const chunk = bytes.toString("ascii", 12, 16);

	if (chunk === "VP8X") {
		return { width: 1 + bytes.readUIntLE(24, 3), height: 1 + bytes.readUIntLE(27, 3) };
	}

	if (chunk === "VP8L") {
		if (bytes.length < 25) return null;
		const bits = bytes.readUInt32LE(21);
		return { width: (bits & 0x3fff) + 1, height: ((bits >> 14) & 0x3fff) + 1 };
	}

	if (chunk === "VP8 ") {
		return { width: bytes.readUInt16LE(26) & 0x3fff, height: bytes.readUInt16LE(28) & 0x3fff };
	}

	return null;
}

function normalizedIcon(path: string): number | undefined {
	if (!existsSync(path)) return undefined;
	const bytes = readFileSync(path);
	const size = webpSize(bytes);

	if (size === null || size.width !== ICON_SIZE || size.height !== ICON_SIZE || bytes.length > TARGET_BYTES) {
		return undefined;
	}

	return bytes.length;
}

function checkOne(game: GameFolder): CheckRow | undefined {
	const problems: string[] = [];
	const warnings: string[] = [];
	const png = join(game.dir, "icon.png");

	if (existsSync(png)) problems.push("icon.png is legacy; use icon.webp");
	const icon = join(game.dir, ICON);

	if (!existsSync(icon)) {
		return problems.length === 0 ? undefined : { game: game.id, bytes: 0, width: 0, height: 0, problems, warnings };
	}

	const bytes = readFileSync(icon);
	const size = webpSize(bytes);

	if (size === null) {
		problems.push("not a WebP file");
	} else if (size.width !== ICON_SIZE || size.height !== ICON_SIZE) {
		problems.push(`${size.width}x${size.height}, expected ${ICON_SIZE}x${ICON_SIZE}`);
	}

	if (bytes.length > HARD_MAX_BYTES) {
		problems.push(`${formatBytes(bytes.length)} is above the ${formatBytes(HARD_MAX_BYTES)} hard limit`);
	} else if (bytes.length > TARGET_BYTES) {
		warnings.push(`${formatBytes(bytes.length)} is above the ${formatBytes(TARGET_BYTES)} target`);
	}

	return { game: game.id, bytes: bytes.length, width: size?.width ?? 0, height: size?.height ?? 0, problems, warnings };
}

function statusOf(row: CheckRow): "FAIL" | "warn" | "ok" {
	if (row.problems.length > 0) return "FAIL";

	if (row.warnings.length > 0) return "warn";
	return "ok";
}

function checkGames(selected: GameFolder[]): 0 | 1 {
	let failures = 0;
	let checked = 0;

	for (const game of selected) {
		const row = checkOne(game);

		if (row === undefined) continue;
		checked++;
		const status = statusOf(row);
		process.stdout.write(
			`${row.game.padEnd(24)} ${formatBytes(row.bytes).padStart(8)} ${`${row.width}x${row.height}`.padStart(9)} ${status}\n`,
		);

		for (const problem of row.problems) console.error(`  ${problem}`);

		for (const warning of row.warnings) console.warn(`  ${warning}`);

		if (row.problems.length > 0) failures++;
	}

	if (failures > 0) {
		console.error(`${failures} icon(s) failed.`);
		return 1;
	}

	process.stdout.write(`${checked} icon(s) checked.\n`);
	return 0;
}

function printConversion(result: Conversion, dryRun: boolean): void {
	const path = relative(ROOT, result.source);
	const source = path.startsWith("..") ? basename(result.source) : path;
	process.stdout.write(
		`${result.game.padEnd(24)} ${source.padEnd(44)} ${formatBytes(result.bytes).padStart(8)} q${result.quality} ${
			dryRun ? "dry-run" : "written"
		}\n`,
	);
}

async function main(): Promise<0 | 1> {
	const options = parseArgs(process.argv.slice(2));
	const all = gameFolders();
	const selected = options.game === undefined ? all : all.filter((game) => game.id === options.game);

	if (selected.length === 0) throw new Error(`unknown game ${options.game ?? ""}`);

	if (options.source !== undefined && !existsSync(options.source)) throw new Error(`no source at ${options.source}`);

	if (options.source !== undefined && statSync(options.source).isFile() && options.game === undefined) {
		throw new Error("--source <file> needs --game");
	}

	const map = options.map === undefined ? undefined : readMap(options.map);

	if (map !== undefined) {
		const known = new Set(all.map((game) => game.id));

		for (const id of map.keys()) if (!known.has(id)) throw new Error(`the map names an unknown game: ${id}`);
	}

	if (options.check) return checkGames(selected);

	const results: Conversion[] = [];
	const skipped: string[] = [];
	let kept = 0;

	for (const game of selected) {
		const source = sourceFor(game, options, map);

		if (source === undefined || !existsSync(source)) {
			if (options.game !== undefined) throw new Error(`${game.id}: no source icon`);
			skipped.push(game.id);
			continue;
		}

		const dest = join(game.dir, ICON);

		if (options.source === undefined && options.map === undefined && source === dest) {
			const bytes = normalizedIcon(dest);

			if (bytes !== undefined) {
				kept++;
				process.stdout.write(
					`${game.id.padEnd(24)} ${relative(ROOT, dest).padEnd(44)} ${formatBytes(bytes).padStart(8)} already normalized\n`,
				);
				continue;
			}
		}

		const result = await convertOne(game, source, options.dryRun);
		results.push(result);
		printConversion(result, options.dryRun);
	}

	if (results.length === 0 && kept === 0) throw new Error("no icon was converted");

	if (skipped.length > 0) console.warn(`No source icon for: ${skipped.join(", ")}`);

	if (results.length > 0) {
		process.stdout.write(`${results.length} icon(s) ${options.dryRun ? "encoded" : "written"}.\n`);
	}

	if (kept > 0) process.stdout.write(`${kept} icon(s) already normalized.\n`);
	return 0;
}

try {
	process.exitCode = await main();
} catch (error) {
	console.error(error instanceof Error ? error.message : String(error));
	process.exitCode = 1;
}
