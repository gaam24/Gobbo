import { copyFileSync, existsSync } from "node:fs";
import path from "node:path";

import { definitionsDir, fixturesDir, homeDir, loadersDir, outDir, rootsDir } from "./env";
import { makeDir, removeTree, writeFile, writeZip } from "./files";

export interface GameFixture {
	id: string;
	name: string;
	root: string;
	out: string;
	definitionPath: string;
	definition: string;
}

export interface GameOptions {
	id: string;
	name: string;
	/// The deployment layout. A slot is the mirror of `games/e2e`; the others exercise one rule each.
	layout?: "slot" | "mirror" | "flat" | "protected" | "loader";
	slotDir?: string;
	slotMarker?: string;
	slotPrefix?: string;
	shared?: boolean;
	rootFiles?: boolean;
	aliases?: boolean;
	protected?: boolean;
	loadOrder?: boolean;
	userData?: boolean;
	userDataMode?: "copy" | "redirect";
	userDataInherit?: boolean;
	profileData?: boolean;
	requires?: boolean;
	steps?: boolean;
	tools?: boolean;
	verified?: boolean;
	provider?: "nexus" | "thunderstore" | "both";
	steamAppId?: number;
	steamInstallDir?: string;
	markers?: string[];
	anyMarker?: boolean;
	/// The executable the fixture exposes. The default game script exits 0; the others sleep or crash for the launch tests.
	executable?: "game" | "sleep" | "crash" | "sleep-binary";
	/// An existing game folder, for a fake Steam library. Without one the fixture uses `target/e2e-data/roots/<id>`.
	root?: string;
	/// Appended at the end of the file; use it for a top-level array of tables such as `[[user_data]]` or `[[steps]]`.
	extraToml?: string;
}

function tomlString(value: string): string {
	return JSON.stringify(value);
}

function array(values: readonly string[]): string {
	return `[${values.map((value) => tomlString(value)).join(", ")}]`;
}

/// A game definition in the shape the app reads. The same template serves the slot, mirror, flat, loader and protected
/// rules; a test only names the difference.
// biome-ignore lint/complexity/noExcessiveCognitiveComplexity: one flat builder of optional TOML sections is clearer than five partial builders.
export function gameToml(options: GameOptions): string {
	const layout = options.layout ?? "slot";
	const slotDir = options.slotDir ?? (layout === "loader" ? "BepInEx/plugins" : "Mods");
	const slotMarker = options.slotMarker ?? (layout === "loader" ? undefined : "content");
	const slotPrefix = options.slotPrefix ?? "mod";
	let providers = "providers = {}";
	if (options.provider === "nexus") providers = `providers = { nexus = { key = ${tomlString(options.id)} } }`;
	if (options.provider === "thunderstore") providers = `providers = { thunderstore = { key = ${tomlString(options.id)} } }`;
	if (options.provider === "both") {
		providers = `providers = { nexus = { key = ${tomlString(options.id)} }, thunderstore = { key = ${tomlString(options.id)} } }`;
	}
	const markers = options.markers ?? ["bin/game.exe"];
	const lines: string[] = [
		"schema = 1",
		`id = ${tomlString(options.id)}`,
		`name = ${tomlString(options.name)}`,
		...(layout === "loader" ? ['loader = "e2e-loader"'] : []),
		providers,
	];
	if (options.loadOrder) {
		lines.push("[load_order]", 'format = "mods_settings"', 'file = "{settings}/mods.settings"');
	}
	if (options.profileData) {
		lines.push(`profile_data = ${array([`{home}/e2e-profile/${options.id}/settings.ini`])}`);
	}
	if (options.requires) {
		lines.push(`requires = ${array(["loader.dll"])}`);
	}
	if (options.verified) {
		lines.push("[verified]", 'date = "2026-11-12"', 'os = ["linux"]');
	}
	lines.push(
		"[detect]",
		`markers = ${array(markers)}`,
		...(options.anyMarker ? ["any_marker = true"] : []),
		...(options.steamAppId !== undefined ? [`steam_app_ids = [${options.steamAppId}]`] : []),
		...(options.steamInstallDir ? [`steam_install_dirs = ${array([options.steamInstallDir])}`] : []),
		"",
		"[[executables]]",
		'name = "Game"',
		'path = "bin/game.exe"',
		`path_linux = ${tomlString(options.executable === "sleep-binary" ? "bin/sleep" : `bin/${options.executable ?? "game"}.sh`)}`,
		...(options.executable === "sleep-binary" ? ['args = ["30"]'] : []),
		"",
	);
	if (options.requires) {
		lines.push("[[requirements]]", 'name = "E2E Loader"', 'path = "loader.dll"', 'url = "https://example.invalid/loader"');
	}
	if (layout === "slot" || layout === "loader" || layout === "protected") {
		lines.push("[deploy]");
		if (options.rootFiles) lines.push(`root_files = ${array(["winhttp.dll"])}`);
		if (options.protected || layout === "protected") lines.push(`protected = ${array(["bin/config"])}`);
		if (options.aliases || layout === "loader") {
			lines.push("[deploy.aliases]", 'plugins = "BepInEx/plugins"');
		}
		lines.push(
			"[[deploy.slots]]",
			`dir = ${tomlString(slotDir)}`,
			`prefix = ${tomlString(slotPrefix)}`,
			...(slotMarker ? [`marker = ${tomlString(slotMarker)}`] : []),
			...(layout === "loader" ? [`root_extensions = ${array([".dll"])}`, `root_folder_extensions = ${array([".dll"])}`] : []),
		);
		if (options.shared ?? true) {
			lines.push("[[deploy.shared]]", 'dir = "content"', `extensions = ${array([".ws"])}`);
		}
	}
	if (layout === "mirror") {
		lines.push(
			"[deploy]",
			"[[deploy.mirror]]",
			'dir = "DLC"',
			`extensions = ${array([".bin"])}`,
			"[[deploy.mirror]]",
			'dir = "bin/config"',
			`extensions = ${array([".ini"])}`,
			"[[deploy.mirror]]",
			'dir = "bin/x64_dx12"',
			`extensions = ${array([".dll"])}`,
		);
	}
	if (layout === "flat") {
		lines.push(
			"[deploy]",
			"[[deploy.slots]]",
			'dir = "Content/Paks/~mods"',
			"flat = true",
			`extensions = ${array([".pak"])}`,
			`root_extensions = ${array([".pak"])}`,
		);
	}
	if (options.userData) {
		lines.push(
			"[[user_data]]",
			`id = "saves"`,
			'kind = "saves"',
			`path = "{home}/e2e-user/${options.id}/saves"`,
			`mode = ${tomlString(options.userDataMode ?? "copy")}`,
			'default = "isolated"',
			`inherit = ${options.userDataInherit ?? true}`,
		);
	}
	if (options.steps) {
		lines.push(
			"[[steps]]",
			'id = "prepare"',
			'phase = "before_start"',
			'program = "bin/step.sh"',
			`outputs = ${array(["content/step.txt"])}`,
			'log = "step.log"',
		);
	}
	if (options.tools) {
		lines.push("[[tools]]", 'id = "hello"', 'name = "Hello Tool"', `detect = ${array(["bin/tool.sh"])}`, 'output = "content/tool.txt"');
	}
	if (options.extraToml) lines.push(options.extraToml);
	return `${lines.join("\n").trim()}\n`;
}

const GAME_SCRIPT = `#!/bin/sh
set -eu
out="\${GOBBO_E2E_OUT:-}"
if [ -z "$out" ]; then exit 0; fi
mkdir -p "$out"
printf '%s\\n' "$@" > "$out/args.txt"
pwd > "$out/cwd.txt"
env > "$out/env.txt"
for file in Mods/*/content/x.ws Mods/*/content/shared.ws ../Mods/*/content/x.ws ../Mods/*/content/shared.ws content/x.ws content/shared.ws ../content/x.ws ../content/shared.ws; do
  if [ -f "$file" ]; then cp "$file" "$out/mod.ws"; break; fi
done
printf 'gobbo e2e game\\n'
`;

const SLEEP_SCRIPT = `#!/bin/sh
out="\${GOBBO_E2E_OUT:-}"
if [ -n "$out" ]; then printf 'sleeping\\n' > "$out/sleep.txt"; fi
sleep 30
`;

const CRASH_SCRIPT = `#!/bin/sh
out="\${GOBBO_E2E_OUT:-}"
if [ -n "$out" ]; then printf 'crash\\n' > "$out/crash.txt"; fi
printf 'crash on purpose\\n' >&2
exit 3
`;

const SECOND_SCRIPT = `#!/bin/sh
out="\${GOBBO_E2E_OUT:-}"
if [ -n "$out" ]; then printf 'second\\n' > "$out/second.txt"; fi
`;

const STEP_SCRIPT = `#!/bin/sh
out="\${GOBBO_E2E_OUT:-}"
if [ -z "$out" ]; then exit 0; fi
mkdir -p "$out"
printf 'step %s\\n' "$*" >> "$out/steps.log"
printf 'step output\\n' > content/step.txt
`;

const TOOL_SCRIPT = `#!/bin/sh
mkdir -p ../content
printf 'tool output\\n' > ../content/tool.txt
`;

/// The files a game folder needs before `add_game`: the marker, the launch scripts, and the folders the launch reads.
export function writeGameRoot(options: GameOptions, root = path.join(rootsDir, options.id)): string {
	makeDir(path.join(root, "bin"));
	makeDir(path.join(root, "content/base"));
	writeFile(path.join(root, "bin/game.exe"), "");
	writeFile(path.join(root, "bin/game.sh"), GAME_SCRIPT, 0o755);
	writeFile(path.join(root, "bin/game.cmd"), "@echo off\r\nexit /b 0\r\n");
	writeFile(path.join(root, "bin/sleep.sh"), SLEEP_SCRIPT, 0o755);
	writeFile(path.join(root, "bin/crash.sh"), CRASH_SCRIPT, 0o755);
	writeFile(path.join(root, "bin/second.sh"), SECOND_SCRIPT, 0o755);
	writeFile(path.join(root, "bin/step.sh"), STEP_SCRIPT, 0o755);
	writeFile(path.join(root, "bin/tool.sh"), TOOL_SCRIPT, 0o755);
	if (options.executable === "sleep-binary") {
		const sleep = existsSync("/bin/sleep") ? "/bin/sleep" : "/usr/bin/sleep";
		copyFileSync(sleep, path.join(root, "bin/sleep"));
	}
	for (const marker of options.markers ?? ["bin/game.exe"]) {
		writeFile(path.join(root, marker), "");
	}
	if (options.loadOrder) makeDir(path.join(root, "settings"));
	if (options.layout === "loader") {
		writeFile(path.join(root, "BepInEx/core/E2E2.dll"), "loader");
		writeFile(path.join(root, "winhttp.dll"), "proxy");
	}
	if (options.userData) makeDir(path.join(homeDir, "e2e-user", options.id, "saves"));
	if (options.profileData) makeDir(path.join(homeDir, "e2e-profile", options.id));
	writeFile(path.join(homeDir, "e2e-user", options.id, "saves", "real.dat"), "real");
	writeFile(path.join(homeDir, "e2e-profile", options.id, "settings.ini"), "real=1\n");
	makeDir(path.join(outDir, options.id));
	return root;
}

/// Writes the definition and the game folder. The app reloads user definitions when the Games page opens, so a later
/// `add_game` and a window reload see it.
export function prepareGame(options: GameOptions): GameFixture {
	const definition = gameToml(options);
	const definitionPath = path.join(definitionsDir, options.id, "game.toml");
	writeFile(definitionPath, definition);
	const root = writeGameRoot(options, options.root);
	return { id: options.id, name: options.name, root, out: path.join(outDir, options.id), definitionPath, definition };
}

export function modZip(name: string, folder: string, files?: Record<string, string>): string {
	const entries: Record<string, string> = {};
	for (const [relative, content] of Object.entries(files ?? { "content/x.ws": "x" })) entries[`Mods/${folder}/${relative}`] = content;
	const file = path.join(fixturesDir, name);
	writeZip(file, entries);
	return file;
}

export function wrappedModZip(name: string, wrapper: string, folder: string): string {
	const file = path.join(fixturesDir, name);
	writeZip(file, { [`${wrapper}/Mods/${folder}/content/x.ws`]: "wrapped" });
	return file;
}

export function multiModZip(name: string, folders: string[]): string {
	const entries: Record<string, string> = {};
	for (const folder of folders) entries[`Mods/${folder}/content/${folder}.ws`] = folder;
	const file = path.join(fixturesDir, name);
	writeZip(file, entries);
	return file;
}

export function invalidZip(name: string): string {
	const file = path.join(fixturesDir, name);
	writeZip(file, { "readme.txt": "nothing the game reads" });
	return file;
}

export function traversalZip(name: string): string {
	const file = path.join(fixturesDir, name);
	writeZip(file, { "../escaped.txt": "escape", "Mods/Escape/content/x.ws": "x" });
	return file;
}

export function versionModZip(name: string, modName: string, version: string): string {
	const file = path.join(fixturesDir, name);
	writeZip(file, {
		[`Mods/${modName}/info.json`]: JSON.stringify({ modName, version }),
		[`Mods/${modName}/content/version.ws`]: version,
	});
	return file;
}

export function fomodZip(name: string, required = false): string {
	const file = path.join(fixturesDir, name);
	writeZip(file, {
		"fomod/ModuleConfig.xml": `<?xml version="1.0" encoding="UTF-8"?>
<config xmlns="http://qconsulting.ca/fo3/ModConfig5.0">
  <moduleName>E2E FOMOD</moduleName>
  <installSteps order="Explicit">
    <installStep name="Textures">
      <optionalFileGroups order="Explicit">
        <group name="Quality" type="SelectExactlyOne">
          <plugins>
            <plugin name="Low">
              <description>Low quality</description>
              <files><folder source="Mods/Fomod/textures/low" destination="content/textures"/></files>
              <typeDescriptor><type name="${required ? "Required" : "Optional"}"/></typeDescriptor>
            </plugin>
            <plugin name="High">
              <files><folder source="Mods/Fomod/textures/high" destination="content/textures"/></files>
              <typeDescriptor><type name="Optional"/></typeDescriptor>
            </plugin>
          </plugins>
        </group>
      </optionalFileGroups>
    </installStep>
  </installSteps>
</config>`,
		"Mods/Fomod/textures/low/low.ws": "low",
		"Mods/Fomod/textures/high/high.ws": "high",
		"Mods/Fomod/content/base.ws": "base",
	});
	return file;
}

export interface SteamFixture {
	appId: number;
	steamRoot: string;
	libraryRoot: string;
	gameRoot: string;
}

/// A fake Steam installation under the E2E home. `scan_games` reads the same files a real installation has.
export function seedSteamGame(appId: number, installDir: string): SteamFixture {
	const steamRoot = path.join(homeDir, ".local/share/Steam");
	const libraryRoot = path.join(homeDir, "SteamLibrary");
	makeDir(path.join(steamRoot, "steamapps"));
	makeDir(path.join(libraryRoot, "steamapps"));
	writeFile(
		path.join(steamRoot, "steamapps/libraryfolders.vdf"),
		`"libraryfolders"\n{\n\t"0"\n\t{\n\t\t"path"\t\t"${steamRoot}"\n\t}\n\t"1"\n\t{\n\t\t"path"\t\t"${libraryRoot}"\n\t}\n}\n`,
	);
	writeFile(
		path.join(libraryRoot, `steamapps/appmanifest_${appId}.acf`),
		`"AppState"\n{\n\t"appid"\t\t"${appId}"\n\t"installdir"\t\t"${installDir}"\n}\n`,
	);
	const gameRoot = path.join(libraryRoot, "steamapps/common", installDir);
	makeDir(path.join(gameRoot, "bin"));
	writeFile(path.join(gameRoot, "bin/game.exe"), "");
	return { appId, steamRoot, libraryRoot, gameRoot };
}

/// A pack is created by the app; this only records the path the suite asks the app to write to.
export function packPath(name: string): string {
	return path.join(fixturesDir, name.endsWith(".gobbopack") ? name : `${name}.gobbopack`);
}

/// The fixture folder is shared by specs; a clean run starts without one.
export function prepareFixtureFolders(): void {
	removeTree(fixturesDir);
	removeTree(rootsDir);
	removeTree(outDir);
	makeDir(fixturesDir);
	makeDir(rootsDir);
	makeDir(outDir);
	makeDir(definitionsDir);
	makeDir(loadersDir);
}
