/// <reference types="bun" />
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

// The update manifest (`latest.json`) for the Tauri updater. Each OS signs its own archives, then:
// - `stage` (installers) and `stage-archive` (the portable archive) run on the build job. They record the archive's
//   name and signature in a small fragment; `stage` also copies the installer archive under a name without spaces,
//   because GitHub turns spaces in asset names into dots and a URL built from Tauri's name would not match.
// - `merge` runs once in the publish job. It reads every fragment and writes `latest.json` whose URLs point at the
//   release assets. A manifest without all four platforms is refused: an update must not reach a copy that has no
//   archive to install from.

/// The installer platforms: the updater runs the installer (NSIS on Windows, AppImage on Linux). With
/// `createUpdaterArtifacts: true`, Tauri v2 signs the installer itself (`<file>.sig`); there is no `.nsis.zip` or
/// `.AppImage.tar.gz` (those are the v1-compatible names).
export const INSTALLER_SUFFIX = {
	"windows-x86_64": "-setup.exe",
	"linux-x86_64": ".AppImage",
} as const;

/// The portable archive platforms: the updater replaces the program files of a portable copy.
export const PORTABLE_PLATFORMS = ["windows-x86_64-portable", "linux-x86_64-portable"] as const;

export type InstallerPlatform = keyof typeof INSTALLER_SUFFIX;
export type PortablePlatform = (typeof PORTABLE_PLATFORMS)[number];
export type Platform = InstallerPlatform | PortablePlatform;

/// Every key a manifest must have.
export const PLATFORMS: readonly Platform[] = ["windows-x86_64", "linux-x86_64", "windows-x86_64-portable", "linux-x86_64-portable"];

export interface Fragment {
	platform: Platform;
	file: string;
	signature: string;
}

export interface PlatformEntry {
	signature: string;
	url: string;
}

export interface Manifest {
	version: string;
	notes: string;
	pub_date: string;
	platforms: Record<Platform, PlatformEntry>;
}

const WHITESPACE = /\s/;
const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?$/;
const REPO = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/;

export function isPlatform(value: string): value is Platform {
	return PLATFORMS.some((platform) => platform === value);
}

function isInstallerPlatform(value: Platform): value is InstallerPlatform {
	return value in INSTALLER_SUFFIX;
}

function isFragment(value: unknown): value is Fragment {
	if (typeof value !== "object" || value === null) return false;
	if (!("platform" in value && "file" in value && "signature" in value)) return false;
	return (
		typeof value.platform === "string" &&
		isPlatform(value.platform) &&
		typeof value.file === "string" &&
		typeof value.signature === "string"
	);
}

function filesUnder(dir: string): string[] {
	return readdirSync(dir, { recursive: true })
		.map((entry) => path.join(dir, entry.toString()))
		.filter((file) => statSync(file).isFile());
}

function signatureOf(archive: string): string {
	if (!existsSync(`${archive}.sig`)) {
		throw new Error(`${archive} has no signature file; the build was not signed (set TAURI_SIGNING_PRIVATE_KEY)`);
	}
	return readFileSync(`${archive}.sig`, "utf8").trim();
}

/// The one installer archive of a platform. Zero or several archives is an error, not a guess.
export function findUpdaterArchive(bundleDir: string, platform: InstallerPlatform): string {
	const suffix = INSTALLER_SUFFIX[platform];
	const [archive, ...others] = filesUnder(bundleDir).filter((file) => file.endsWith(suffix));
	if (archive === undefined || others.length > 0) {
		throw new Error(`expected one ${suffix} archive for ${platform} under ${bundleDir}, found ${1 + others.length}`);
	}
	signatureOf(archive);
	return archive;
}

function writeFragment(outDir: string, fragment: Fragment): void {
	mkdirSync(outDir, { recursive: true });
	writeFileSync(path.join(outDir, `${fragment.platform}.fragment.json`), `${JSON.stringify(fragment, null, 2)}\n`);
}

/// Copies the installer archive under a URL-safe name and writes its fragment.
export function stage(bundleDir: string, outDir: string, platform: InstallerPlatform): Fragment {
	const archive = findUpdaterArchive(bundleDir, platform);
	const file = path.basename(archive).replace(/\s+/g, "-");
	mkdirSync(outDir, { recursive: true });
	copyFileSync(archive, path.join(outDir, file));
	const fragment: Fragment = { platform, file, signature: signatureOf(archive) };
	writeFragment(outDir, fragment);
	return fragment;
}

/// Writes the fragment of a portable archive. The archive is already a release asset (it is in the archive upload),
/// so only its name and signature are recorded. A name with a space would not match the uploaded asset.
export function stageArchive(archive: string, outDir: string, platform: PortablePlatform): Fragment {
	const file = path.basename(archive);
	if (WHITESPACE.test(file)) {
		throw new Error(`${file} has a space in its name; GitHub would rename the asset`);
	}
	const fragment: Fragment = { platform, file, signature: signatureOf(archive) };
	writeFragment(outDir, fragment);
	return fragment;
}

/// The manifest for one version. All four platforms must be present.
export function buildManifest(fragments: Fragment[], version: string, repo: string, notes: string, now: Date): Manifest {
	if (!SEMVER.test(version)) {
		throw new Error(`invalid version: ${version}`);
	}
	if (!REPO.test(repo)) {
		throw new Error(`invalid repository: ${repo}`);
	}
	const entry = (platform: Platform): PlatformEntry => {
		const fragment = fragments.find((item) => item.platform === platform);
		if (!fragment) {
			throw new Error(`no staged updater for ${platform}; every platform must be built before the manifest`);
		}
		return {
			signature: fragment.signature,
			url: `https://github.com/${repo}/releases/download/v${version}/${fragment.file}`,
		};
	};
	return {
		version,
		notes,
		pub_date: now.toISOString(),
		platforms: {
			"windows-x86_64": entry("windows-x86_64"),
			"linux-x86_64": entry("linux-x86_64"),
			"windows-x86_64-portable": entry("windows-x86_64-portable"),
			"linux-x86_64-portable": entry("linux-x86_64-portable"),
		},
	};
}

/// Reads every fragment in `inDir` and writes the manifest.
export function merge(inDir: string, outFile: string, version: string, repo: string, notes: string): Manifest {
	const fragments: Fragment[] = [];
	for (const name of readdirSync(inDir)) {
		if (!name.endsWith(".fragment.json")) continue;
		const parsed: unknown = JSON.parse(readFileSync(path.join(inDir, name), "utf8"));
		if (isFragment(parsed)) fragments.push(parsed);
	}
	const manifest = buildManifest(fragments, version, repo, notes, new Date());
	mkdirSync(path.dirname(outFile), { recursive: true });
	writeFileSync(outFile, `${JSON.stringify(manifest, null, 2)}\n`);
	return manifest;
}

function fail(message: string): never {
	console.error(`updater-manifest: ${message}`);
	process.exit(1);
}

function main(argv: string[]) {
	const [command, ...args] = argv;
	const [first, second, third, fourth, fifth] = args;
	try {
		if (command === "stage" && args.length === 3 && first !== undefined && second !== undefined && third !== undefined) {
			if (!(isPlatform(third) && isInstallerPlatform(third))) fail(`not an installer platform: ${third}`);
			const fragment = stage(first, second, third);
			process.stdout.write(`Staged ${fragment.file} for ${third}\n`);
		} else if (command === "stage-archive" && args.length === 3 && first !== undefined && second !== undefined && third !== undefined) {
			if (!isPortablePlatform(third)) fail(`not a portable platform: ${third}`);
			const fragment = stageArchive(first, second, third);
			process.stdout.write(`Staged ${fragment.file} for ${third}\n`);
		} else if (
			command === "merge" &&
			(args.length === 4 || args.length === 5) &&
			first !== undefined &&
			second !== undefined &&
			third !== undefined &&
			fourth !== undefined
		) {
			merge(first, second, third, fourth, fifth ?? `Gobbo ${third}`);
			process.stdout.write(`Wrote ${second}\n`);
		} else {
			fail(
				"usage: stage <bundleDir> <outDir> <installer-platform> | stage-archive <archive> <outDir> <portable-platform> | merge <inDir> <outFile> <version> <owner/repo> [notes]",
			);
		}
	} catch (error) {
		fail(error instanceof Error ? error.message : String(error));
	}
}

function isPortablePlatform(value: string): value is PortablePlatform {
	return (PORTABLE_PLATFORMS as readonly string[]).includes(value);
}

if (import.meta.main) {
	main(process.argv.slice(2));
}
