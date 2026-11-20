import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { type Zippable, zipSync } from "fflate";

// Packs the release build of the current OS into `dist-release/gobbo-<version>-<os>-<arch>-portable.zip`: the program,
// its overlay binaries (Windows), `games/` and `loaders/`. The installers Tauri bundled are published as they are.
// Tauri cannot cross-compile, so each OS builds and packs its own archive; run `bun run build` on each one.

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
const portableFolder = `gobbo-${version}-${osName}-${process.arch}-portable`;
const exeName = process.platform === "win32" ? "gobbo.exe" : "gobbo";

// Every file under `dir`, keyed by its path below `prefix` in the archive (always with forward slashes).
function filesUnder(dir: string, prefix: string, entries: Zippable) {
	for (const entry of readdirSync(dir, { recursive: true })) {
		const rel = entry.toString();
		const abs = path.join(dir, rel);

		if (!statSync(abs).isFile()) {
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

mkdirSync(distDir, { recursive: true });
const zipPath = path.join(distDir, `${portableFolder}.zip`);
rmSync(zipPath, { force: true });
writeFileSync(zipPath, zipSync(portableEntries, { level: 9 }));
process.stdout.write(`Wrote ${zipPath}\n`);
