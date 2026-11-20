import { expect, test } from "bun:test";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { buildManifest, type Fragment, findUpdaterArchive, merge, stage, stageArchive } from "./updater-manifest";

/// The version is owned by release.json; the updater fixtures read it once so a release bump does not rewrite a dozen
/// names. `scripts/check-release-meta.ts` asserts the other copies match it.
function releaseVersion(): string {
	const meta: unknown = JSON.parse(readFileSync(join(import.meta.dir, "..", "release.json"), "utf8"));

	if (typeof meta === "object" && meta !== null && "version" in meta && typeof meta.version === "string") {
		return meta.version;
	}

	throw new Error("release.json has no version");
}

const VERSION = releaseVersion();

function workspace() {
	const root = mkdtempSync(join(tmpdir(), "updater-manifest-"));
	return {
		root,
		cleanup: () => rmSync(root, { recursive: true, force: true }),
	};
}

/// A bundle folder shaped like Tauri's output: the archive with a space in its name, and its signature beside it.
function bundleWith(root: string, folder: string, name: string, signature: string | null) {
	const dir = join(root, "bundle", folder);
	mkdirSync(dir, { recursive: true });
	writeFileSync(join(dir, name), "archive");

	if (signature !== null) {
		writeFileSync(join(dir, `${name}.sig`), `${signature}\n`);
	}

	return join(root, "bundle");
}

function fragments(): Fragment[] {
	return [
		{
			platform: "windows-x86_64",
			file: `Gobbo-${VERSION}-setup.exe`,
			signature: "SIG-WIN",
		},
		{
			platform: "linux-x86_64",
			file: `Gobbo_${VERSION}_amd64.AppImage`,
			signature: "SIG-LIN",
		},
		{
			platform: "windows-x86_64-portable",
			file: `gobbo-${VERSION}-windows-x64-portable.zip`,
			signature: "SIG-WIN-P",
		},
		{
			platform: "linux-x86_64-portable",
			file: `gobbo-${VERSION}-linux-x64-portable.zip`,
			signature: "SIG-LIN-P",
		},
	];
}

test("stage copies the installer under a name without spaces and records its signature", () => {
	const ws = workspace();

	try {
		const bundle = bundleWith(ws.root, "nsis", `Gobbo Mods Organizer_${VERSION}_x64-setup.exe`, "SIG-WIN");
		const out = join(ws.root, "out");
		const fragment = stage(bundle, out, "windows-x86_64");
		expect(fragment).toEqual({
			platform: "windows-x86_64",
			file: `Gobbo-Mods-Organizer_${VERSION}_x64-setup.exe`,
			signature: "SIG-WIN",
		});
		expect(existsSync(join(out, fragment.file))).toBe(true);
		expect(existsSync(join(out, "windows-x86_64.fragment.json"))).toBe(true);
	} finally {
		ws.cleanup();
	}
});

test("a missing signature or an ambiguous installer is refused, not guessed", () => {
	const ws = workspace();

	try {
		const unsigned = bundleWith(ws.root, "appimage", `Gobbo_${VERSION}_amd64.AppImage`, null);
		expect(() => findUpdaterArchive(unsigned, "linux-x86_64")).toThrow("was not signed");

		const twice = bundleWith(ws.root, "nsis", "a-setup.exe", "S1");
		writeFileSync(join(twice, "nsis", "b-setup.exe"), "archive");
		writeFileSync(join(twice, "nsis", "b-setup.exe.sig"), "S2");
		expect(() => findUpdaterArchive(twice, "windows-x86_64")).toThrow("found 2");
	} finally {
		ws.cleanup();
	}
});

test("stageArchive records the portable archive that is already a release asset", () => {
	const ws = workspace();

	try {
		const archive = join(ws.root, "dist-release", `gobbo-${VERSION}-linux-x64-portable.zip`);
		mkdirSync(join(ws.root, "dist-release"), { recursive: true });
		writeFileSync(archive, "zip");
		writeFileSync(`${archive}.sig`, "SIG-PORTABLE\n");
		const out = join(ws.root, "updater");
		const fragment = stageArchive(archive, out, "linux-x86_64-portable");
		expect(fragment).toEqual({
			platform: "linux-x86_64-portable",
			file: `gobbo-${VERSION}-linux-x64-portable.zip`,
			signature: "SIG-PORTABLE",
		});
		// The archive is not copied: the release already carries it under this name.
		expect(existsSync(join(out, `gobbo-${VERSION}-linux-x64-portable.zip`))).toBe(false);
	} finally {
		ws.cleanup();
	}
});

test("a portable archive whose name has a space is refused before it reaches a URL", () => {
	const ws = workspace();

	try {
		const archive = join(ws.root, `gobbo ${VERSION} portable.zip`);
		writeFileSync(archive, "zip");
		writeFileSync(`${archive}.sig`, "SIG");
		expect(() => stageArchive(archive, join(ws.root, "out"), "windows-x86_64-portable")).toThrow("has a space");
	} finally {
		ws.cleanup();
	}
});

test("the manifest points each platform at its release asset and needs all four platforms", () => {
	const manifest = buildManifest(fragments(), VERSION, "gaam24/Gobbo", "Notes", new Date("2026-11-16T00:00:00Z"));
	expect(manifest.version).toBe(VERSION);
	expect(manifest.notes).toBe("Notes");
	expect(manifest.pub_date).toBe("2026-11-16T00:00:00.000Z");
	expect(manifest.platforms["windows-x86_64"]).toEqual({
		signature: "SIG-WIN",
		url: `https://github.com/gaam24/Gobbo/releases/download/v${VERSION}/Gobbo-${VERSION}-setup.exe`,
	});
	expect(manifest.platforms["linux-x86_64-portable"]).toEqual({
		signature: "SIG-LIN-P",
		url: `https://github.com/gaam24/Gobbo/releases/download/v${VERSION}/gobbo-${VERSION}-linux-x64-portable.zip`,
	});

	expect(() => buildManifest(fragments().slice(0, 3), VERSION, "gaam24/Gobbo", "", new Date())).toThrow(
		"no staged updater for linux-x86_64-portable",
	);
});

test("bad versions and repository names are refused before a URL is written", () => {
	expect(() => buildManifest(fragments(), `v${VERSION}`, "gaam24/Gobbo", "", new Date())).toThrow("invalid version");
	expect(() => buildManifest(fragments(), VERSION, "gaam24/Gobbo/extra", "", new Date())).toThrow("invalid repository");
});

test("merge reads the staged fragments and writes latest.json", () => {
	const ws = workspace();

	try {
		const staged = join(ws.root, "staged");
		mkdirSync(staged, { recursive: true });

		for (const fragment of fragments()) {
			writeFileSync(join(staged, `${fragment.platform}.fragment.json`), JSON.stringify(fragment));
		}

		writeFileSync(join(staged, "notes.txt"), "ignored");
		const outFile = join(ws.root, "dist", "latest.json");
		merge(staged, outFile, VERSION, "gaam24/Gobbo", `Gobbo ${VERSION}`);
		const written = JSON.parse(readFileSync(outFile, "utf8"));
		expect(written.version).toBe(VERSION);
		expect(written.notes).toBe(`Gobbo ${VERSION}`);
		expect(written.platforms["windows-x86_64"].signature).toBe("SIG-WIN");
		expect(written.platforms["windows-x86_64-portable"].url).toBe(
			`https://github.com/gaam24/Gobbo/releases/download/v${VERSION}/gobbo-${VERSION}-windows-x64-portable.zip`,
		);
	} finally {
		ws.cleanup();
	}
});
