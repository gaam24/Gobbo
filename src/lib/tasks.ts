import type { TaskDedupe } from "@/bindings";
import type { TranslationKey } from "@/i18n";

/// One long-running operation the window can show in the activity centre. The task is a view model: the action that
/// started it owns the real state, and the activity hook turns that state into these rows.
export type TaskKind = "apply" | "launch" | "stop" | "import" | "download" | "install" | "update" | "scan" | "collection";

export type TaskState = "running" | "waiting" | "failed";

export interface TaskStages {
	names: string[];
	current: number;
}

export interface Task {
	id: string;
	kind: TaskKind;
	/// The line the user reads first, already translated.
	title: string;
	/// A second line with the file, the stage or the item the work is about.
	detail?: string;
	/// Why a failed task failed, already translated. Shown in full under the title.
	error?: string;
	/// The steps of the work, already translated, and the index of the current one. Past the last index means done.
	stages?: TaskStages;
	/// 0–100 when the operation can measure itself.
	percent?: number;
	state: TaskState;
	onCancel?: () => void;
	onRetry?: () => void;
	/// Removes a failed task from the activity centre without trying it again.
	onDismiss?: () => void;
	/// The label of the retry button when it is not a retry, such as **Install** for a finished download.
	actionLabel?: string;
	/// The game id this work belongs to. `null` means global work; absent means the current game.
	game?: string | null;
	/// The game name to prefix the detail line with, so work of another game is identifiable.
	gameName?: string;
	/// The label of the cancel button when **Cancel** would be ambiguous, such as **Stop collection**.
	cancelLabel?: string;
	/// Opens the dialog or window the user must act in, for work that is waiting on them.
	onOpen?: () => void;
	/// The label of the open button, defaulting to **Open window**.
	openLabel?: string;
	/// Identifies the same operation across sources, so a backend lifecycle row and the local row with real progress
	/// are drawn once. Falls back to `id`.
	dedupeKey?: string;
}

/// The key an operation is grouped by in the Working panel. A backend row carries the typed [`TaskDedupe`] its builder
/// set; the local row of the same work is built from the same variants. Switching on the typed value keeps the two in
/// step without parsing an id in the feature that draws them. `id`, `backend` and `backendKind` are only read for
/// [`TaskDedupe::Other`], which is how a row restored from an older journal is grouped.
export function dedupeKey(dedupe: TaskDedupe, id = "", backend = false, backendKind?: string): string {
	if (dedupe === "Other") return keyFromId(id, backend, backendKind);

	if ("Apply" in dedupe && dedupe.Apply) return `apply:${dedupe.Apply.game}`;

	if ("Import" in dedupe && dedupe.Import) return `import:${dedupe.Import.game}:${dedupe.Import.path}`;

	if ("Update" in dedupe && dedupe.Update) return `update:${dedupe.Update.game}`;

	if ("UpdateAll" in dedupe && dedupe.UpdateAll) return `update-all:${dedupe.UpdateAll.game}`;

	if ("Requirement" in dedupe && dedupe.Requirement) return `requirement:${dedupe.Requirement.game}`;

	if ("Collection" in dedupe && dedupe.Collection) {
		return `collection:${dedupe.Collection.game}:${dedupe.Collection.profile}:${dedupe.Collection.collection}`;
	}

	if ("Pack" in dedupe && dedupe.Pack) return `pack:${dedupe.Pack.game}:${dedupe.Pack.path}`;

	if ("PackLink" in dedupe && dedupe.PackLink) return `pack-link:${dedupe.PackLink.game}:${dedupe.PackLink.link}`;

	if ("Tool" in dedupe) return `tool:${dedupe.Tool.game}:${dedupe.Tool.tool}`;
	return id;
}

/// Reads a typed identity carried by a row that predates the field on the shared registry type. A missing or unknown
/// value means the row must fall back to its id, so a new backend field can never break an older feature row.
export function taskDedupeOf(value: unknown): TaskDedupe | undefined {
	if (value === undefined) return undefined;

	if (isTaskDedupe(value)) return value;
	return undefined;
}

function isTaskDedupe(value: unknown): value is TaskDedupe {
	if (value === "Other") return true;

	if (typeof value !== "object" || value === null) return false;
	const variants = ["Apply", "Import", "Update", "UpdateAll", "Requirement", "Collection", "Pack", "PackLink", "Tool"];
	return variants.some((variant) => variant in value);
}

/// Rows restored from an older journal carry no typed identity, and a local feature row may only have an id. This is
/// the single place that reads the encoding the backend builders write, so a local row and its backend lifecycle row
/// still collapse into one Working row.
function keyFromId(id: string, backend: boolean, backendKind?: string): string {
	const parts = id.split(backend ? "\n" : "|");
	const head = parts[0] ?? "";
	const separator = head.indexOf(":");
	const prefix = separator >= 0 ? head.slice(0, separator) : head;
	const game = separator >= 0 ? head.slice(separator + 1) : "";
	const detail = parts.slice(1).join(":");
	const withDetail = (name: string) => `${name}:${game}${detail ? `:${detail}` : ""}`;

	// A local row names only the workflows that register it; every other id is already its own identity.
	if (!backend) {
		if (prefix === "collection") return withDetail("collection");

		if (prefix === "pack-import" || prefix === "pack-download") return withDetail("pack");

		if (prefix === "pack-link") return withDetail("pack-link");
		return id;
	}

	switch (prefix) {
		case "apply":
			return `apply:${game}`;
		case "import":
			return withDetail("import");
		case "update-mod":
		case "update":
			return `update:${game}`;
		case "update-all":
			return `update-all:${game}`;
		case "requirement":
			return `requirement:${game}`;
		case "collection":
			return withDetail("collection");
		case "pack":
		case "pack-import":
		case "pack-download":
			return withDetail("pack");
		case "pack-link":
			return withDetail("pack-link");
		default:
			return `backend:${backendKind ?? ""}:${parts.join(":")}`;
	}
}

/// The order of the activity card: what needs a decision first, then what is in progress.
export const TASK_STATE_ORDER: readonly TaskState[] = ["failed", "waiting", "running"];

/// The one label per state, shared by the header badges, the group headers and the live region.
export const TASK_GROUP_LABEL = {
	failed: "tasks.group.failed",
	waiting: "tasks.group.waiting",
	running: "tasks.group.running",
} as const satisfies Record<TaskState, TranslationKey>;

export type TaskGroupLabelKey = (typeof TASK_GROUP_LABEL)[TaskState];

export interface TaskCounts {
	running: number;
	waiting: number;
	failed: number;
}

/// Counts by state, independent of insertion order. Progress ticks change `percent` but not these counts, which is why
/// the live region can key off them instead of announcing every update.
export function taskCounts(tasks: readonly Task[]): TaskCounts {
	const counts: TaskCounts = { running: 0, waiting: 0, failed: 0 };

	for (const task of tasks) counts[task.state] += 1;
	return counts;
}

export interface TaskGameGroup {
	/// `null` for work that belongs to no single game. A missing `game` is global work too.
	game: string | null;
	/// The display name, taken from the rows when the game is still in the catalogue.
	gameName?: string;
	count: number;
	tasks: Task[];
}

/// Code-point order, not `localeCompare`: the card must sort the same way on every machine and in every locale.
function compareText(a: string, b: string): -1 | 0 | 1 {
	if (a < b) return -1;

	if (a > b) return 1;
	return 0;
}

function compareTasks(a: Task, b: Task): -1 | 0 | 1 {
	return compareText(a.title, b.title) || compareText(a.id, b.id);
}

function compareGames(a: TaskGameGroup, b: TaskGameGroup): -1 | 0 | 1 {
	if (a.game === null) return b.game === null ? 0 : -1;

	if (b.game === null) return 1;
	return compareText(a.gameName ?? "", b.gameName ?? "") || compareText(a.game, b.game);
}

/// The order of rows inside one game: work in progress first, then work that waits for the user, then failures.
const ROW_STATE_ORDER: Record<TaskState, number> = {
	running: 0,
	waiting: 1,
	failed: 2,
};

function compareRows(a: Task, b: Task): number {
	return ROW_STATE_ORDER[a.state] - ROW_STATE_ORDER[b.state] || compareTasks(a, b);
}

/// The activity card's view model: one group per game, so every game's work is visible from any screen. Global work
/// comes first, then games by name; inside a game the rows go by state, then title, then id. The registry keeps
/// insertion order so a progress update does not jump a row, so the ordering here is explicit and deterministic.
export function groupedTasks(tasks: readonly Task[]): TaskGameGroup[] {
	const byGame = new Map<string | null, Task[]>();

	for (const task of tasks) {
		const game = task.game ?? null;
		const rows = byGame.get(game);

		if (rows) rows.push(task);
		else byGame.set(game, [task]);
	}

	const games: TaskGameGroup[] = [];

	for (const [game, rows] of byGame) {
		const sorted = rows.toSorted(compareRows);
		games.push({
			game,
			gameName: sorted.find((row) => Boolean(row.gameName))?.gameName,
			count: sorted.length,
			tasks: sorted,
		});
	}

	return games.toSorted(compareGames);
}

/// A short status for the live region. It names only the state whose count changed, so a percent tick does not make a
/// screen reader repeat the card. The caller supplies the translated label; a failure is left to the failure toast, so
/// the two announcements do not say the same thing twice.
export function taskAnnouncement(previous: TaskCounts, current: TaskCounts, label: (key: TaskGroupLabelKey) => string): string {
	if (current.waiting !== previous.waiting) return `${label(TASK_GROUP_LABEL.waiting)}: ${current.waiting}`;

	if (current.failed !== previous.failed) return `${label(TASK_GROUP_LABEL.failed)}: ${current.failed}`;

	if (current.running !== previous.running) return `${label(TASK_GROUP_LABEL.running)}: ${current.running}`;
	return "";
}
