/// <reference types="bun" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { releaseVersion } from "./release-version";

/// One owner for the app version: `release.json` at the repository root. Every copy is asserted against it, so a bump
/// changes that one value and any file left behind fails `bun run guard` (part of `bun run lint`). The
/// updater tests read it through the shared `releaseVersion()` accessor instead of repeating the version. The README
/// status line is prose, not a version copy: it says the project is in early development, with no number or count to
/// keep in step. The supported-games tables in both READMEs are checked against `games/*/game.toml`: adding a
/// definition cannot update one list and forget the other.

const root = join(import.meta.dir, "..");
const CARGO_VERSION = /\[workspace\.package\]\s*\n\s*version\s*=\s*"(\d+\.\d+\.\d+)"/;
const JSON_VERSION = /"version"\s*:\s*"(\d+\.\d+\.\d+)"/;
const BUG_VERSION = /id: version[\s\S]*?placeholder:\s*(\S+)/;
const ICON_ID = /games\/([a-z0-9-]+)\/icon\.webp/g;
const TOML_NAME = /^name\s*=\s*"([^"]+)"/m;
const SEPARATOR = /^-+$/;

interface Game {
	id: string;
	name: string;
}

function read(relative: string): string {
	return readFileSync(join(root, relative), "utf8");
}

function expectVersion(text: string, pattern: RegExp, label: string, version: string, problems: string[]): void {
	const found = pattern.exec(text)?.[1] ?? null;
	if (found === null) problems.push(`${label}: no current-version reference to check`);
	else if (found !== version) problems.push(`${label}: ${found} but release.json says ${version}`);
}

function shippedGames(problems: string[]): Game[] {
	return readdirSync(join(root, "games"), { withFileTypes: true })
		.filter((entry) => entry.isDirectory() && !entry.name.startsWith("_"))
		.flatMap((entry) => {
			const name = TOML_NAME.exec(readFileSync(join(root, "games", entry.name, "game.toml"), "utf8"))?.[1];
			if (name === undefined) {
				problems.push(`games/${entry.name}/game.toml has no top-level name`);
				return [];
			}
			return [{ id: entry.name, name }];
		});
}

function compareSets(actual: Set<string>, expected: Set<string>, label: string, problems: string[]): void {
	for (const value of expected) {
		if (!actual.has(value)) problems.push(`${label}: missing ${value}`);
	}
	for (const value of actual) {
		if (!expected.has(value)) problems.push(`${label}: stale ${value}`);
	}
}

/// Asserts the version copies and the game tables. Callers pass their own problem list so this folds into the
/// generated-files guard without a second process.
export function checkReleaseMeta(problems: string[]): void {
	let version: string;
	try {
		version = releaseVersion();
	} catch {
		problems.push("release.json has no version string");
		return;
	}

	expectVersion(read("package.json"), JSON_VERSION, "package.json", version, problems);
	expectVersion(read("src-tauri/tauri.conf.json"), JSON_VERSION, "src-tauri/tauri.conf.json", version, problems);
	expectVersion(read("src-tauri/Cargo.toml"), CARGO_VERSION, "src-tauri/Cargo.toml", version, problems);

	expectVersion(read(".github/ISSUE_TEMPLATE/bug_report.yml"), BUG_VERSION, "bug_report.yml version placeholder", version, problems);

	// The updater test reads through the shared accessor; guard what the accessor points at instead of the test's text.
	if (!read("scripts/release-version.ts").includes("release.json")) {
		problems.push("releaseVersion() must read release.json");
	}

	const games = shippedGames(problems);
	const readme = read("README.md");
	const ids = new Set([...readme.matchAll(ICON_ID)].map((match) => match[1]).filter((id): id is string => id !== undefined));
	compareSets(ids, new Set(games.map((game) => game.id)), "README.md supported-games table", problems);

	const gamesReadme = read("games/README.md");
	const start = gamesReadme.indexOf("## Supported games");
	const next = gamesReadme.indexOf("\n## ", start + 3);
	const section = gamesReadme.slice(start, next === -1 ? undefined : next);
	const rows = section
		.split("\n")
		.filter((line) => line.startsWith("|"))
		.map((line) =>
			line
				.split("|")
				.slice(1, -1)
				.map((cell) => cell.trim()),
		)
		.filter((cells) => (cells[0] ?? "") !== "Game" && !SEPARATOR.test(cells[0] ?? ""));
	compareSets(
		new Set(rows.map((cells) => cells[0]).filter((name): name is string => name !== undefined)),
		new Set(games.map((game) => game.name)),
		"games/README.md supported-games table",
		problems,
	);
}
