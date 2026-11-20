import { parse, stringify } from "smol-toml";

/// The shape of `game.toml` the editor builds. It mirrors the Rust schema on purpose: the form is a view of this
/// object, and the backend checks the serialized text with the real rules before anything is written.
export interface GameDraft {
	schema: number;
	id: string;
	name: string;
	verified?: VerifiedDraft;
	providers?: Record<string, { key?: string }>;
	loader?: string | { id: string; variant?: string };
	detect: DetectDraft;
	executables: ExecutableDraft[];
	deploy: DeployDraft;
	load_order?: LoadOrderDraft;
	profile_data?: string[];
	user_data?: UserDataDraft[];
	requires?: string[];
	requirements?: RequirementDraft[];
	tools?: ToolDraft[];
	steps?: StepDraft[];
	logs?: LogSourceDraft[];
}

export interface VerifiedDraft {
	date: string;
	os?: string[];
	game_version?: string;
	loader_version?: string;
}

export interface DetectDraft {
	steam_app_ids?: number[];
	steam_install_dirs?: string[];
	gog_ids?: string[];
	epic_ids?: string[];
	heroic_slugs?: string[];
	lutris_slugs?: string[];
	markers?: string[];
	any_marker?: boolean;
}

export interface ExecutableDraft {
	name: string;
	path: string;
	path_linux?: string;
	args?: string[];
}

export interface DeployDraft {
	root?: string;
	root_linux?: string;
	slots?: SlotDraft[];
	mirror?: MirrorDraft[];
	root_files?: string[];
	resource_names?: boolean;
	shared?: SharedDraft[];
	protected?: string[];
	aliases?: Record<string, string>;
	/// Every archive path is a game path. The whole deployment root is a mirror; a bare file needs a move rule.
	full_root?: boolean;
	move?: MoveDraft[];
}

export interface MoveDraft {
	extensions?: string[];
	to: string;
}

export interface SlotDraft {
	dir: string;
	prefix?: string;
	marker?: string;
	extensions?: string[];
	root_extensions?: string[];
	root_folder_extensions?: string[];
	flat?: boolean;
}

export interface MirrorDraft {
	dir: string;
	extensions?: string[];
	prefix?: string;
}

export interface SharedDraft {
	dir?: string;
	extensions?: string[];
}

export interface UserDataDraft {
	id: string;
	kind?: string;
	path: string;
	path_linux?: string;
	mode?: string;
	default?: string;
	inherit?: boolean;
}

export interface LoadOrderDraft {
	format: "mods_settings";
	file: string;
	priority_min?: number;
	priority_max?: number;
}

export interface RequirementDraft {
	name: string;
	path: string;
	url?: string;
}

export interface ToolDraft {
	id: string;
	name: string;
	detect?: string[];
	output: string;
	source?: { provider: string; mod_id: string };
	prerequisites?: PrerequisiteDraft[];
}

export interface PrerequisiteDraft {
	name: string;
	url: string;
	sha512: string;
	args?: string[];
	check: string;
	check_starts_with: string;
}

export interface StepDraft {
	id: string;
	phase: "before_start" | "after_end";
	program: string;
	args?: string[];
	when_dir?: string;
	clean?: string[];
	outputs?: string[];
	log?: string;
}

export interface LogSourceDraft {
	kind: string;
	path: string;
	pattern?: string;
	limit?: number;
	recursive?: boolean;
}

const ID_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

/// The `game.toml` schema version the editor writes.
const SCHEMA_VERSION = 1;

export const EMPTY_DRAFT: GameDraft = {
	schema: SCHEMA_VERSION,
	id: "",
	name: "",
	detect: {},
	executables: [],
	deploy: {},
};

/// A draft with the required tables present, so a file that omits a section still opens in the form.
export function normalizeDraft(value: unknown): GameDraft {
	// The value is a TOML document from the backend; the form only needs its shape and the backend validates the
	// whole file again before anything is written.
	// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the backend re-parses and validates the text.
	const source = (isRecord(value) ? value : {}) as Partial<GameDraft>;
	const draft: GameDraft = {
		...EMPTY_DRAFT,
		...source,
		schema: SCHEMA_VERSION,
		id: source.id ?? "",
		name: source.name ?? "",
		detect: source.detect ?? {},
		executables: Array.isArray(source.executables) ? source.executables : [],
		deploy: source.deploy ?? {},
	};
	return draft;
}

export function parseDraft(text: string): GameDraft {
	return normalizeDraft(parse(text));
}

/// Serializes a draft into clean TOML. Defaults are left out, so a definition written from the form looks like a
/// hand-written one. The backend still parses and validates the result before it is saved.
export function serializeDraft(draft: GameDraft): string {
	// smol-toml accepts a plain table; cleanDraft has already removed values TOML cannot express.
	return stringify(cleanDraft(draft));
}

/// The problems the form can see without the backend, as translation keys. The backend check remains the authority.
export type DraftIssue = "name" | "id" | "executable" | "layout";

export function localIssues(draft: GameDraft): DraftIssue[] {
	const issues: DraftIssue[] = [];

	if (draft.name.trim() === "") issues.push("name");

	if (!ID_PATTERN.test(draft.id.trim())) issues.push("id");

	if (!draft.executables.some((executable) => executable.path.trim() !== "")) issues.push("executable");

	if (
		!(draft.loader || draft.deploy.full_root) &&
		(draft.deploy.slots ?? []).every((slot) => slot.dir.trim() === "") &&
		(draft.deploy.mirror ?? []).every((mirror) => mirror.dir.trim() === "")
	) {
		issues.push("layout");
	}

	return issues;
}

export function slugify(text: string): string {
	const slug = text
		.normalize("NFKD")
		.replaceAll(/[\u0300-\u036f]/g, "")
		.replaceAll("ł", "l")
		.replaceAll("Ł", "L")
		.toLowerCase()
		.replaceAll(/[^a-z0-9]+/g, "-")
		.replaceAll(/^-+|-+$/g, "");
	return slug === "" ? "game" : slug;
}

export function loaderId(draft: GameDraft): string {
	if (!draft.loader) return "";
	return typeof draft.loader === "string" ? draft.loader : draft.loader.id;
}

export function loaderVariant(draft: GameDraft): string {
	if (!draft.loader || typeof draft.loader === "string") return "";
	return draft.loader.variant ?? "";
}

export function setLoader(draft: GameDraft, id: string, variant: string): GameDraft {
	if (id === "") return { ...draft, loader: undefined };
	return { ...draft, loader: variant === "" ? id : { id, variant } };
}

export function placeSummary(draft: GameDraft): string[] {
	const places = (draft.deploy.slots ?? []).map((slot) => slot.dir).filter(Boolean);
	places.push(...(draft.deploy.mirror ?? []).map((mirror) => mirror.dir).filter(Boolean));
	return places;
}

/// The quick card that matches an existing definition. Both kinds of place mean the user added something the two
/// simple cards cannot show, so the advanced form is the right starting point.
export function inferLayout(draft: GameDraft): "loader" | "slot" | "mirror" | "custom" {
	if (draft.deploy.full_root) return "custom";

	if (draft.loader) return "loader";
	const slots = (draft.deploy.slots ?? []).filter((slot) => slot.dir.trim() !== "").length;
	const mirrors = (draft.deploy.mirror ?? []).filter((mirror) => mirror.dir.trim() !== "").length;

	if (slots > 0 && mirrors > 0) return "custom";

	if (mirrors > 0) return "mirror";
	return "slot";
}

function cleanDraft(draft: GameDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {
		schema: SCHEMA_VERSION,
		id: draft.id.trim(),
		name: draft.name.trim(),
		detect: cleanDetect(draft.detect),
		deploy: cleanDeploy(draft.deploy),
	};

	if (draft.loader) {
		if (typeof draft.loader === "string") {
			out.loader = draft.loader;
		} else if (draft.loader.variant) {
			out.loader = { id: draft.loader.id, variant: draft.loader.variant };
		} else {
			out.loader = draft.loader.id;
		}
	}

	if (draft.verified?.date) out.verified = cleanVerified(draft.verified);
	const providers = entriesWith(draft.providers, (entry) => (entry.key ?? "").trim() !== "");

	if (providers) out.providers = providers;
	const executables = keepFilled(draft.executables, (executable) => executable.path.trim() !== "", cleanExecutable);

	if (executables.length > 0) out.executables = executables;
	const userData = keepFilled(draft.user_data, (set) => set.id.trim() !== "" && set.path.trim() !== "", cleanUserData);

	if (userData.length > 0) out.user_data = userData;
	const requires = cleanLines(draft.requires);

	if (requires) out.requires = requires;
	const requirements = keepFilled(draft.requirements, (entry) => entry.name.trim() !== "" && entry.path.trim() !== "", cleanRequirement);

	if (requirements.length > 0) out.requirements = requirements;
	const profileData = cleanLines(draft.profile_data);

	if (profileData) out.profile_data = profileData;

	if (draft.load_order?.file.trim()) {
		out.load_order = cleanLoadOrder(draft.load_order);
	}

	const tools = keepFilled(draft.tools, (tool) => tool.id.trim() !== "" && tool.output.trim() !== "", cleanTool);

	if (tools.length > 0) out.tools = tools;
	const steps = keepFilled(draft.steps, (step) => step.id.trim() !== "" && step.program.trim() !== "", cleanStep);

	if (steps.length > 0) out.steps = steps;
	const logs = keepFilled(draft.logs, (log) => log.path.trim() !== "", cleanLog);

	if (logs.length > 0) out.logs = logs;
	return out;
}

function cleanLog(log: LogSourceDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {
		kind: log.kind.trim() || "game",
		path: log.path.trim(),
	};
	putText(out, "pattern", log.pattern);

	if (log.limit != null && log.limit > 0) out.limit = Math.trunc(log.limit);

	if (log.recursive === false) out.recursive = false;
	return out;
}

function cleanDetect(detect: DetectDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {};
	const appIds = (detect.steam_app_ids ?? []).filter((id) => Number.isInteger(id) && id > 0);

	if (appIds.length > 0) out.steam_app_ids = appIds;

	for (const key of ["steam_install_dirs", "gog_ids", "epic_ids", "heroic_slugs", "lutris_slugs", "markers"] as const) {
		const values = cleanLines(detect[key]);

		if (values) out[key] = values;
	}

	if (detect.any_marker) out.any_marker = true;
	return out;
}

function cleanDeploy(deploy: DeployDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {};
	putText(out, "root", deploy.root);
	putText(out, "root_linux", deploy.root_linux);
	const slots = keepFilled(deploy.slots, (slot) => slot.dir.trim() !== "", cleanSlot);

	if (slots.length > 0) out.slots = slots;
	const mirror = keepFilled(deploy.mirror, (entry) => entry.dir.trim() !== "", cleanMirror);

	if (mirror.length > 0) out.mirror = mirror;
	const rootFiles = cleanLines(deploy.root_files);

	if (rootFiles) out.root_files = rootFiles;

	if (deploy.resource_names) out.resource_names = true;
	const shared = keepFilled(deploy.shared, (entry) => (entry.dir?.trim() ?? "") !== "" || (entry.extensions ?? []).length > 0, cleanShared);

	if (shared.length > 0) out.shared = shared;
	const protectedFolders = cleanLines(deploy.protected);

	if (protectedFolders) out.protected = protectedFolders;
	const aliases = entriesWith(deploy.aliases, (target, name) => name.trim() !== "" && target.trim() !== "");

	if (aliases) out.aliases = aliases;

	if (deploy.full_root) out.full_root = true;
	const moves = keepFilled(deploy.move, (entry) => entry.to.trim() !== "", cleanMove);

	if (moves.length > 0) out.move = moves;
	return out;
}

function cleanMove(move: MoveDraft): Record<string, unknown> {
	const out: Record<string, unknown> = { to: move.to.trim() };
	const extensions = cleanLines(move.extensions);

	if (extensions) out.extensions = extensions;
	return out;
}

function cleanSlot(slot: SlotDraft): Record<string, unknown> {
	const out: Record<string, unknown> = { dir: slot.dir.trim() };
	putText(out, "prefix", slot.prefix);
	putText(out, "marker", slot.marker);

	for (const key of ["extensions", "root_extensions", "root_folder_extensions"] as const) {
		const values = cleanLines(slot[key]);

		if (values) out[key] = values;
	}

	if (slot.flat) out.flat = true;
	return out;
}

function cleanMirror(mirror: MirrorDraft): Record<string, unknown> {
	const out: Record<string, unknown> = { dir: mirror.dir.trim() };
	const extensions = cleanLines(mirror.extensions);

	if (extensions) out.extensions = extensions;
	putText(out, "prefix", mirror.prefix);
	return out;
}

function cleanShared(shared: SharedDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {};
	putText(out, "dir", shared.dir);
	const extensions = cleanLines(shared.extensions);

	if (extensions) out.extensions = extensions;
	return out;
}

function cleanExecutable(executable: ExecutableDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {
		name: executable.name.trim() || executable.path.trim(),
		path: executable.path.trim(),
	};
	putText(out, "path_linux", executable.path_linux);
	const args = cleanLines(executable.args);

	if (args) out.args = args;
	return out;
}

function cleanUserData(set: UserDataDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {
		id: set.id.trim(),
		path: set.path.trim(),
	};

	if (set.kind && set.kind !== "user-data") out.kind = set.kind;
	putText(out, "path_linux", set.path_linux);

	if (set.mode && set.mode !== "redirect") out.mode = set.mode;

	if (set.default && set.default !== "shared") out.default = set.default;

	if (set.inherit === false) out.inherit = false;
	return out;
}

function cleanLoadOrder(order: LoadOrderDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {
		format: order.format,
		file: order.file.trim(),
	};

	if (order.priority_min !== undefined && order.priority_min !== 1) out.priority_min = order.priority_min;

	if (order.priority_max !== undefined && order.priority_max !== 9999) out.priority_max = order.priority_max;
	return out;
}

function cleanRequirement(requirement: RequirementDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {
		name: requirement.name.trim(),
		path: requirement.path.trim(),
	};
	putText(out, "url", requirement.url);
	return out;
}

function cleanTool(tool: ToolDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {
		id: tool.id.trim(),
		name: tool.name.trim() !== "" ? tool.name.trim() : tool.id.trim(),
		output: tool.output.trim(),
	};
	const detect = cleanLines(tool.detect);

	if (detect) out.detect = detect;

	if (tool.source?.provider.trim() && tool.source.mod_id.trim()) {
		out.source = {
			provider: tool.source.provider.trim(),
			mod_id: tool.source.mod_id.trim(),
		};
	}

	const prerequisites = keepFilled(tool.prerequisites, (entry) => entry.name.trim() !== "" && entry.url.trim() !== "", cleanPrerequisite);

	if (prerequisites.length > 0) out.prerequisites = prerequisites;
	return out;
}

function cleanPrerequisite(prerequisite: PrerequisiteDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {
		name: prerequisite.name.trim(),
		url: prerequisite.url.trim(),
		sha512: prerequisite.sha512.trim(),
		check: prerequisite.check.trim(),
		check_starts_with: prerequisite.check_starts_with.trim(),
	};
	const args = cleanLines(prerequisite.args);

	if (args) out.args = args;
	return out;
}

function cleanStep(step: StepDraft): Record<string, unknown> {
	const out: Record<string, unknown> = {
		id: step.id.trim(),
		phase: step.phase,
		program: step.program.trim(),
	};
	const args = cleanLines(step.args);

	if (args) out.args = args;
	putText(out, "when_dir", step.when_dir);

	for (const key of ["clean", "outputs"] as const) {
		const values = cleanLines(step[key]);

		if (values) out[key] = values;
	}

	putText(out, "log", step.log);
	return out;
}

function cleanVerified(verified: VerifiedDraft): Record<string, unknown> {
	const out: Record<string, unknown> = { date: verified.date.trim() };
	const os = cleanLines(verified.os);

	if (os) out.os = os;
	putText(out, "game_version", verified.game_version);
	putText(out, "loader_version", verified.loader_version);
	return out;
}

/// Writes the trimmed text under `key`. A missing or blank value leaves the key out of the table.
function putText(out: Record<string, unknown>, key: string, value: string | undefined): void {
	const trimmed = value?.trim();

	if (trimmed) out[key] = trimmed;
}

/// The entries that pass `filled` (their required fields are not blank), each cleaned. A missing list is empty.
function keepFilled<T>(
	entries: readonly T[] | undefined,
	filled: (entry: T) => boolean,
	clean: (entry: T) => Record<string, unknown>,
): Array<Record<string, unknown>> {
	return (entries ?? []).filter((entry) => filled(entry)).map((entry) => clean(entry));
}

function cleanLines(values: string[] | undefined): string[] | undefined {
	const cleaned = (values ?? []).map((value) => value.trim()).filter((value) => value !== "");
	return cleaned.length > 0 ? cleaned : undefined;
}

function entriesWith<T>(entries: Record<string, T> | undefined, keep: (value: T, key: string) => boolean): Record<string, T> | undefined {
	if (!entries) return undefined;
	const out: Record<string, T> = {};

	for (const [key, value] of Object.entries(entries)) {
		if (keep(value, key)) out[key] = value;
	}

	return Object.keys(out).length > 0 ? out : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}
