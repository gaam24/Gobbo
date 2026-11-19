import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { type Zippable, zipSync } from "fflate";

// Packs the release build of the current OS into two archives in `dist-release/`:
// - `gobbo-<version>-<os>-<arch>-portable.zip` with the program, its hook DLLs (Windows), `games/` and `loaders/`;
// - `gobbo-<version>-<os>-<arch>.zip` with what Tauri bundled (the installers).
// Tauri cannot cross-compile, so each OS builds and packs its own archives; run `bun run build` on each one.

const root = path.resolve(import.meta.dirname, "..");
const srcTauri = path.join(root, "src-tauri");
const targetDir = path.resolve(srcTauri, process.env.CARGO_TARGET_DIR ?? "target");
const release = path.join(targetDir, "release");
const distDir = path.join(root, "dist-release");

let osName: string | undefined;
if (process.platform === "win32") {
	osName = "windows";
} else if (process.platform === "linux") {
	osName = "linux";
}

if (osName === undefined) {
	throw new Error(`Packaging is not set up for ${process.platform} yet.`);
}

const config: unknown = JSON.parse(readFileSync(path.join(srcTauri, "tauri.conf.json"), "utf8"));
if (typeof config !== "object" || config === null || !("version" in config) || typeof config.version !== "string") {
	throw new Error("src-tauri/tauri.conf.json has no version.");
}
const version = config.version;
const folder = `gobbo-${version}-${osName}-${process.arch}`;
const portableFolder = `${folder}-portable`;
const exeName = process.platform === "win32" ? "gobbo.exe" : "gobbo";

// Every file under `dir`, keyed by its path below `prefix` in the archive (always with forward slashes).
function filesUnder(dir: string, prefix: string, entries: Zippable) {
	for (const entry of readdirSync(dir, { recursive: true })) {
		const rel = entry.toString();
		const abs = path.join(dir, rel);
		if (!statSync(abs).isFile() || abs.includes(".AppDir")) {
			continue;
		}
		entries[`${prefix}/${rel.split(path.sep).join("/")}`] = readFileSync(abs);
	}
}

function required(file: string) {
	if (!existsSync(file)) {
		throw new Error(`${file} is missing. Run \`bun run build\` on this OS first.`);
	}
	return file;
}

const portableEntries: Zippable = {};
function addPortableFile(file: string) {
	portableEntries[`${portableFolder}/${file}`] = readFileSync(required(path.join(release, file)));
}

addPortableFile(exeName);
// The marker tells the app this is the portable archive, so the first-start question recommends Portable. The
// decision file itself is deliberately not shipped: the user chooses on the first start.
portableEntries[`${portableFolder}/gobbo.portable`] = new TextEncoder().encode("Gobbo portable\n");
if (process.platform === "win32") {
	for (const file of [
		"gobbo_overlay_hook.dll",
		"gobbo_overlay_hook32.dll",
		"gobbo_overlay_hook.pdb",
		"gobbo_overlay_hook32.pdb",
		"gobbo_overlay_inject32.exe",
		"gobbo_overlay_probe.exe",
		"gobbo_overlay_probe32.exe",
		"gobbo_overlay_dump32.exe",
	]) {
		addPortableFile(file);
	}
}
// The game and loader definitions come from the repository, not from `target/release/games`: Tauri merges the resource
// folder into the target directory on every build, so a renamed game would otherwise ship twice, under its old and new
// id.
filesUnder(path.join(root, "games"), `${portableFolder}/games`, portableEntries);
filesUnder(path.join(root, "loaders"), `${portableFolder}/loaders`, portableEntries);

const installerEntries: Zippable = {};
filesUnder(required(path.join(release, "bundle")), folder, installerEntries);

mkdirSync(distDir, { recursive: true });
function writeArchive(name: string, entries: Zippable) {
	const zipPath = path.join(distDir, name);
	rmSync(zipPath, { force: true });
	writeFileSync(zipPath, zipSync(entries, { level: 9 }));
	process.stdout.write(`Wrote ${zipPath}\n`);
}

writeArchive(`${portableFolder}.zip`, portableEntries);
writeArchive(`${folder}.zip`, installerEntries);
