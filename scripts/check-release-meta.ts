/// <reference types="bun" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

/// One owner for the app version: `release.json` at the repository root. Every copy is asserted against it, so a bump
/// changes that one value and any file left behind fails `bun run generated:check` (part of `bun run lint`). The
/// updater tests read `release.json` directly instead of repeating the version. `docs/release.md` uses a later version
/// in its manifest example; only references to the *current* release are matched below. The supported-games tables in
/// both READMEs are checked against `games/*/game.toml` for the same reason: adding a definition cannot update one
/// list and forget the other.

const root = join(import.meta.dir, "..");
const CARGO_VERSION = /\[workspace\.package\]\s*\n\s*version\s*=\s*"(\d+\.\d+\.\d+)"/;
const JSON_VERSION = /"version"\s*:\s*"(\d+\.\d+\.\d+)"/;
const README_STATUS = /\*\*Status:\*\*\s+(\S+),/;
const RELEASE_OUT = /release `v(\d+\.\d+\.\d+)`/;
const RELEASE_UNPACK = /unpack `v(\d+\.\d+\.\d+)`/;
const RELEASE_ATTEST = /gh attestation verify dist-release\/gobbo-(\d+\.\d+\.\d+)-linux-x64-portable\.zip/;
const BUG_VERSION = /id: version[\s\S]*?placeholder:\s*(\S+)/;
const TEST_LITERAL = /\b\d+\.\d+\.\d+\b/;
const ICON_ID = /games\/([a-z0-9-]+)\/icon\.webp/g;
const TOML_NAME = /^name\s*=\s*"([^"]+)"/m;
const SEPARATOR = /^-+$/;
const SUMMARY = /(\w+) games? are tested on real installations; (\w+) more definitions? are ready/i;

const NUMBER_WORDS: Record<string, number> = {
	one: 1,
	two: 2,
	three: 3,
	four: 4,
	five: 5,
	six: 6,
	seven: 7,
	eight: 8,
	nine: 9,
	ten: 10,
	eleven: 11,
	twelve: 12,
	thirteen: 13,
	fourteen: 14,
	fifteen: 15,
	sixteen: 16,
	seventeen: 17,
	eighteen: 18,
	nineteen: 19,
	twenty: 20,
};

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

/// The release gate must really skip the long E2E suite and compare the tag against full history. Inside a reusable
/// workflow `github.event_name` is the caller's event, so only the input can turn E2E off; a shallow checkout makes the
/// tag-on-main ancestry check fail whenever main moved after the tag.
function checkReleaseGates(problems: string[]): void {
	const ci = read(".github/workflows/ci.yml");
	const release = read(".github/workflows/release.yml");
	if (!(ci.includes("skip_e2e:") && ci.includes("!inputs.skip_e2e"))) {
		problems.push("ci.yml: the E2E job must gate on !inputs.skip_e2e");
	}
	if (!release.includes("skip_e2e: true")) {
		problems.push("release.yml: the checks call must pass skip_e2e: true");
	}
	const buildJob = release.split("  build:")[1] ?? "";
	if (!buildJob.includes("fetch-depth: 0")) {
		problems.push("release.yml: the build checkout needs fetch-depth: 0 for the tag-on-main check");
	}
	// rust-cache keys include the job id unless shared-key is set, and the environment hash includes CARGO_INCREMENTAL.
	if (!release.includes('CARGO_INCREMENTAL: "0"')) {
		problems.push("release.yml: set CARGO_INCREMENTAL=0 so the Rust cache key matches ci.yml");
	}
	if (!(buildJob.includes("shared-key: ") && buildJob.includes("matrix.cache-key"))) {
		problems.push("release.yml: the Rust cache must use shared-key to reuse the CI cache");
	}
}

/// Asserts the version copies and the game tables. Callers pass their own problem list so this folds into the
/// generated-files guard without a second process.
export function checkReleaseMeta(problems: string[]): void {
	const meta: unknown = JSON.parse(read("release.json"));
	const version = typeof meta === "object" && meta !== null && "version" in meta && typeof meta.version === "string" ? meta.version : "";
	if (version === "") {
		problems.push("release.json has no version string");
		return;
	}

	expectVersion(read("package.json"), JSON_VERSION, "package.json", version, problems);
	expectVersion(read("src-tauri/tauri.conf.json"), JSON_VERSION, "src-tauri/tauri.conf.json", version, problems);
	expectVersion(read("src-tauri/Cargo.toml"), CARGO_VERSION, "src-tauri/Cargo.toml", version, problems);
	expectVersion(read("README.md"), README_STATUS, "README.md status", version, problems);

	const release = read("docs/release.md");
	expectVersion(release, RELEASE_OUT, "docs/release.md release step", version, problems);
	expectVersion(release, RELEASE_UNPACK, "docs/release.md unpack step", version, problems);
	expectVersion(release, RELEASE_ATTEST, "docs/release.md attestation command", version, problems);

	expectVersion(read(".github/ISSUE_TEMPLATE/bug_report.yml"), BUG_VERSION, "bug_report.yml version placeholder", version, problems);

	const updaterTest = read("scripts/updater-manifest.test.ts");
	if (!updaterTest.includes("release.json")) problems.push("scripts/updater-manifest.test.ts must take its version from release.json");
	if (TEST_LITERAL.test(updaterTest)) problems.push("scripts/updater-manifest.test.ts has a hard-coded version; read release.json");

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

	const summary = SUMMARY.exec(readme);
	const tested = rows.filter((cells) => (cells[2] ?? "").toLowerCase() === "yes").length;
	const ready = rows.filter((cells) => (cells[2] ?? "").toLowerCase() === "no").length;
	if (summary === null) {
		problems.push("README.md: keep the 'N games are tested ...; M more definitions are ready' status summary");
	} else {
		const words = [summary[1], summary[2]].map((word) => NUMBER_WORDS[(word ?? "").toLowerCase()]);
		if (words[0] !== tested || words[1] !== ready) {
			problems.push(
				`README.md status summary says ${String(words[0] ?? "?")}/${String(words[1] ?? "?")} but the game table says ${tested}/${ready}`,
			);
		}
	}
	checkReleaseGates(problems);
}

if (import.meta.main) {
	const problems: string[] = [];
	checkReleaseMeta(problems);
	if (problems.length > 0) {
		process.stderr.write(`${problems.join("\n")}\n`);
		process.exit(1);
	}
	process.stdout.write("Release metadata passed.\n");
}
