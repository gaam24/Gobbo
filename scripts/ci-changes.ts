import { appendFileSync, readFileSync } from "node:fs";

/// Which CI jobs a change needs. The file list comes from the GitHub API, not from a shallow checkout, so a pull
/// request and a push classify the same way. A schedule, a manual run and a reusable call run every job.

interface ChangedFile {
	filename: string;
}
interface EventPayload {
	pull_request?: { number: number };
}

const event = process.env.GITHUB_EVENT_NAME ?? "";
const payloadPath = process.env.GITHUB_EVENT_PATH;
const payload: EventPayload = payloadPath ? JSON.parse(readFileSync(payloadPath, "utf8")) : {};
const token = process.env.GITHUB_TOKEN ?? "";
const api = process.env.GITHUB_API_URL ?? "https://api.github.com";
const repository = process.env.GITHUB_REPOSITORY ?? "";
const output = process.env.GITHUB_OUTPUT;

const headers = {
	Accept: "application/vnd.github+json",
	Authorization: `Bearer ${token}`,
	"X-GitHub-Api-Version": "2022-11-28",
};

function isItems<T>(value: unknown): value is T[] {
	return Array.isArray(value);
}

async function pages<T>(path: string): Promise<T[]> {
	const collected: T[] = [];
	for (let page = 1; ; page += 1) {
		const separator = path.includes("?") ? "&" : "?";
		const response = await fetch(`${api}${path}${separator}per_page=100&page=${page}`, { headers });
		if (!response.ok) throw new Error(`GitHub API ${path}: ${response.status} ${await response.text()}`);
		const data: unknown = await response.json();
		if (!isItems<T>(data)) throw new Error(`GitHub API ${path}: expected a list of items`);
		collected.push(...data);
		if (data.length < 100) return collected;
	}
}

async function changedFiles(): Promise<string[] | null> {
	// A push to main is already merged: run the full gate instead of paying for path classification. Pull requests keep
	// the selective treatment, because they are the ones waiting for feedback.
	if (event === "push") return null;
	if (event === "pull_request" && payload.pull_request) {
		const files = await pages<ChangedFile>(`/repos/${repository}/pulls/${payload.pull_request.number}/files`);
		return files.map((file) => file.filename);
	}
	// No diff to inspect: nightly, manual and reusable calls run the full set.
	return null;
}

// The web lint and build read the scripts and the generated bindings; the Rust crates read the game definitions and
// the same scripts. A workflow edit belongs to both so a change to a gate is checked by all of its jobs.
const groups: Record<string, RegExp[]> = {
	// Windows compiles code the Linux job never sees, and its build script needs staged hook resources. Only a change
	// to that surface (or a nightly/manual run) pays for the Windows job on a pull request.
	windows: [
		/^src-tauri\/src\/platform\/windows\.rs$/,
		/^src-tauri\/src\/deploy\/overlay\/windows\//,
		/^src-tauri\/src\/deploy\/overlay\/report\.rs$/,
		/^src-tauri\/crates\/overlay-(hook|launch|pe|probe|dump|core|decisions)\//,
		/^src-tauri\/tauri\.windows\.conf\.json$/,
		/^src-tauri\/build\.rs$/,
		/^src-tauri\/Cargo\.(toml|lock)$/,
	],
	web: [
		/^src\//,
		/^e2e\//,
		/^public\//,
		/^scripts\//,
		/^package\.json$/,
		/^bun\.lock$/,
		/^tsconfig/,
		/^biome\.jsonc$/,
		/^\.oxlintrc/,
		/^vite\.config\./,
		/^index\.html$/,
		/^\.github\/workflows\//,
	],
	rust: [/^src-tauri\//, /^games\//, /^loaders\//, /^scripts\//, /^package\.json$/, /^bun\.lock$/, /^\.github\/workflows\//],
};

const files = await changedFiles();
if (!output) throw new Error("GITHUB_OUTPUT is not set");
appendFileSync(output, `all=${files === null}\n`);
for (const [name, patterns] of Object.entries(groups)) {
	const needed = files === null || files.some((file) => patterns.some((pattern) => pattern.test(file)));
	appendFileSync(output, `${name}=${needed}\n`);
}
