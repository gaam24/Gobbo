import { commands, type TASK_KINDS, type TaskDedupe, type TaskError, type TaskView_Serialize } from "@/bindings";
import { type TranslationKey, useTranslation } from "@/i18n";
import { createStore, useStore } from "@/lib/external-store";
import { errorText, unwrap } from "@/lib/ipc";
import { fileName } from "@/lib/paths";
import type { TaskKind } from "@/lib/tasks";
import { useTauriListener } from "@/lib/use-tauri-listener";

/// Work registered by a feature that lives outside the active game's status: a collection install, a dialog that is
/// waiting for the user, or a long backend operation. The registry is a singleton so the row survives a game switch
/// and an open dialog, and the activity card is the only subscriber.
export type RegisteredTaskState = "running" | "waiting" | "failed";

export interface TaskAction {
	label: string;
	run: () => void;
}

export interface RegisteredTask {
	id: string;
	/// The game id this task belongs to. `null` means global work.
	game: string | null;
	kind: TaskKind;
	/// Already translated by the feature that registered the task.
	title: string;
	detail?: string;
	/// Why the work failed, already translated.
	error?: string;
	percent?: number;
	state: RegisteredTaskState;
	cancel?: TaskAction;
	retry?: TaskAction;
	open?: TaskAction;
	dismiss?: () => void;
	source: "local" | "backend";
	/// The backend's stable `kind` code, kept so a backend row can be matched with its local counterpart.
	sourceKind?: string;
	/// The typed identity, when the producer knows it. Local rows without one keep the id fallback.
	dedupe?: TaskDedupe;
}

const store = createStore<RegisteredTask[]>([]);

/// Adds a task or replaces the one with the same id, in place, so a percent update does not jump the row to the top.
export function registerTask(task: RegisteredTask): void {
	const current = store.get();
	const at = current.findIndex((item) => item.id === task.id);

	if (at < 0) {
		store.set([...current, task]);
		return;
	}

	const next = [...current];
	next[at] = task;
	store.set(next);
}

/// Patches one field group of a task without needing its other callbacks again.
export function updateTask(id: string, patch: Partial<Omit<RegisteredTask, "id">>): void {
	const current = store.get();

	if (!current.some((task) => task.id === id)) return;
	store.set(current.map((task) => (task.id === id ? { ...task, ...patch } : task)));
}

/// Removes a task. An unknown id changes nothing.
export function removeTask(id: string): void {
	const current = store.get();
	const next = current.filter((task) => task.id !== id);

	if (next.length === current.length) return;
	store.set(next);
}

/// Drops every registered task. Tests use it; the window keeps the rows across a game switch.
export function resetRegisteredTasks(): void {
	store.set([]);
}

/// The current snapshot, for the activity hook and for a test reader without a component.
export function registeredTasks(): RegisteredTask[] {
	return store.get();
}

export function useRegisteredTasks(): RegisteredTask[] {
	return useStore(store);
}

/// The payload of the backend task events. `kind` is a stable code from the backend, not display text; `percent` is
/// the last progress the work reported, when it has sent any.
export interface BackendTaskPayload {
	id: string;
	game: string | null;
	kind: string;
	state: string;
	percent?: number;
	/// Optional because a journal row written before the typed identity existed has no discriminator.
	dedupe?: TaskDedupe;
	/// Why a failed task failed. Absent on rows of other states and on journals written before the field existed.
	error?: TaskError | null;
}

/// The codes the backend can emit, from the generated binding. The Rust `TASK_KINDS` list is the single source, so a
/// new backend code widens this type and the map below stops compiling until it is handled here.
type BackendTaskCode = (typeof TASK_KINDS)[number];
interface KindEntry {
	kind: TaskKind;
	label: TranslationKey;
}

/// Every emitted code must have an entry here. Non-null `Record` typing is the drift guard: a missing or misspelled
/// code is a compile error, not a silent `update` fallback in the activity centre.
const REQUIRED_BACKEND_KINDS: Record<BackendTaskCode, KindEntry> = {
	import: { kind: "import", label: "tasks.kind.import" },
	update: { kind: "update", label: "tasks.kind.update" },
	"update-all": { kind: "update", label: "tasks.kind.update-all" },
	requirement: { kind: "install", label: "tasks.kind.requirement" },
	apply: { kind: "apply", label: "tasks.kind.apply" },
	pack: { kind: "import", label: "tasks.kind.pack" },
	"pack-link": { kind: "download", label: "tasks.kind.pack-link" },
	collection: { kind: "collection", label: "tasks.kind.collection" },
	launch: { kind: "launch", label: "tasks.kind.tool" },
	install: { kind: "install", label: "tasks.kind.toolInstall" },
};

/// The lookup also keeps a few frontend-only codes that older callers pass in; every backend code comes from the typed
/// map above.
const BACKEND_KINDS: Record<string, KindEntry> = {
	...REQUIRED_BACKEND_KINDS,
	deploy: { kind: "apply", label: "tasks.kind.deploy" },
	stop: { kind: "stop", label: "tasks.kind.stop" },
	download: { kind: "download", label: "tasks.kind.download" },
	scan: { kind: "scan", label: "tasks.kind.scan" },
};

/// An unknown code still gets a row, using the update icon and a generic title.
export function backendTaskKind(code: string): TaskKind {
	return BACKEND_KINDS[code]?.kind ?? "update";
}

/// The actions a restored backend row can offer. Feature hooks register them by the backend's stable `kind` code;
/// `"*"` holds actions that fit every row, such as Open switching to the row's game. A row with no matching action
/// simply shows no button, so an operation no feature can restart is not given a dead Retry.
export interface BackendTaskActions {
	retry?: (payload: BackendTaskPayload) => TaskAction | undefined;
	open?: (payload: BackendTaskPayload) => TaskAction | undefined;
}

const backendActions = new Map<string, BackendTaskActions>();

/// Attaches the actions of one backend code. The latest registration wins, so a hook that owns a kind can replace the
/// generic actions with a more specific Retry.
export function registerBackendActions(code: string, actions: BackendTaskActions): void {
	backendActions.set(code, actions);
}

/// Drops the actions of one code, called by the hook that registered them when it unmounts.
export function unregisterBackendActions(code: string): void {
	backendActions.delete(code);
}

/// The actions that fit one row: the code's own handler first, then the generic one for the fields it left out.
export function backendActionsOf(kind: string): BackendTaskActions {
	const generic = backendActions.get("*");
	const own = backendActions.get(kind);
	return { retry: own?.retry ?? generic?.retry, open: own?.open ?? generic?.open };
}

type Translate = ReturnType<typeof useTranslation>["t"];

/// The title of a failed row says what failed ("Import failed"), not what was running ("Importing files…").
const FAILED_TITLE: Record<BackendTaskCode, TranslationKey> = {
	import: "tasks.failedKind.import",
	update: "tasks.failedKind.update",
	"update-all": "tasks.failedKind.update-all",
	requirement: "tasks.failedKind.requirement",
	apply: "tasks.failedKind.apply",
	pack: "tasks.failedKind.pack",
	"pack-link": "tasks.failedKind.pack-link",
	collection: "tasks.failedKind.collection",
	launch: "tasks.failedKind.launch",
	install: "tasks.failedKind.install",
};

function isBackendCode(code: string): code is BackendTaskCode {
	return code in FAILED_TITLE;
}

export function backendTaskTitle(code: string, state: string, t: Translate): string {
	if (state === "failed") return t(isBackendCode(code) ? FAILED_TITLE[code] : "tasks.failedKind.generic");
	return t(BACKEND_KINDS[code]?.label ?? "tasks.kind.generic");
}

/// The item a backend row is about, read from its typed identity: the archive, the pack, the link, the collection or the
/// tool. The game is not repeated here; the Working panel already groups the rows by game.
export function backendTaskDetail(dedupe: TaskDedupe | undefined): string | undefined {
	if (dedupe === undefined || dedupe === "Other") return undefined;

	if ("Import" in dedupe && dedupe.Import) return fileName(dedupe.Import.path);

	if ("Pack" in dedupe && dedupe.Pack) return fileName(dedupe.Pack.path);

	if ("PackLink" in dedupe && dedupe.PackLink) return dedupe.PackLink.link;

	if ("Collection" in dedupe && dedupe.Collection) return dedupe.Collection.collection;

	if ("Tool" in dedupe && dedupe.Tool) return dedupe.Tool.tool;
	return undefined;
}

/// The translated reason of a failed backend row.
export function backendTaskError(error: TaskError | null | undefined): string | undefined {
	return error ? errorText({ code: error.code, params: error.params ?? {}, kind: "user_action" }) : undefined;
}

function backendState(state: string): RegisteredTaskState {
	if (state === "failed") return "failed";

	if (state === "waiting") return "waiting";
	return "running";
}

/// Pure mapping of one backend payload into a registry row. The caller supplies the translated title and the actions
/// that fit the row, so this stays free of i18n and Tauri calls and can be tested directly. Only a failed or waiting
/// row gets Retry/Open: a running row is stopped with Cancel instead.
export function backendTaskFromPayload(payload: BackendTaskPayload, title: string, actions: BackendTaskActions = {}): RegisteredTask {
	const task: RegisteredTask = {
		id: payload.id,
		game: payload.game,
		kind: backendTaskKind(payload.kind),
		title,
		detail: backendTaskDetail(payload.dedupe),
		error: backendTaskError(payload.error),
		state: backendState(payload.state),
		percent: payload.percent ?? undefined,
		source: "backend",
		sourceKind: payload.kind,
		dedupe: payload.dedupe,
	};

	if (task.state === "failed" || task.state === "waiting") {
		const retry = actions.retry?.(payload);
		const open = actions.open?.(payload);

		if (retry) task.retry = retry;

		if (open) task.open = open;
	}

	return task;
}

/// Turns a serialized row from the snapshot command into the payload shape the task events share. A snapshot row
/// without progress carries `null`, while the events omit the field, so both become the same optional value.
export function normalizeBackendTask(task: TaskView_Serialize): BackendTaskPayload {
	return {
		id: task.id,
		game: task.game,
		kind: task.kind,
		state: task.state,
		percent: task.percent ?? undefined,
		dedupe: task.dedupe,
		error: task.error,
	};
}

/// Reads the snapshot through the generated binding. A browser preview has no backend, so a failed invoke simply
/// leaves the registry empty rather than breaking the activity card.
async function listBackendTasks(): Promise<readonly BackendTaskPayload[]> {
	try {
		const tasks = await unwrap(commands.listTasks());
		return tasks.map((task) => normalizeBackendTask(task));
	} catch {
		return [];
	}
}

/// Subscribes the activity card to backend work on every game. Called once by `useActivity`, which is mounted for the
/// lifetime of the window, so a task started on another game is visible even while a dialog is open.
export function useBackendTasks(): void {
	const { t } = useTranslation();
	// A browser preview has no events and no backend; the registry simply stays empty.
	useTauriListener(async (scope) => {
		/// A running row is stopped through the generic cancel command, which dispatches to the task's own flag. A failed
		/// or waiting row has no flag: it offers the Retry/Open actions a feature registered for its kind. Rows hydrated
		/// from the `listTasks` snapshot go through the same mapping as live events, so a restart keeps their actions.
		const fromBackend = (payload: BackendTaskPayload): RegisteredTask => {
			const task = backendTaskFromPayload(payload, backendTaskTitle(payload.kind, payload.state, t), backendActionsOf(payload.kind));

			if (task.state === "running") {
				task.cancel = { label: t("common.cancel"), run: () => void commands.cancelTask(payload.id) };
			}

			// A failure stays in the backend journal until the user has read it; dismissing forgets it there too.
			if (task.state === "failed") {
				task.dismiss = () => {
					removeTask(payload.id);
					void commands.dismissTask(payload.id);
				};
			}

			return task;
		};
		const unlisten = await Promise.all([
			scope.listen<BackendTaskPayload>("task-started", (event) => {
				registerTask(fromBackend(event.payload));
			}),
			scope.listen<BackendTaskPayload>("task-finished", (event) => {
				if (event.payload.state === "failed") registerTask(fromBackend(event.payload));
				else removeTask(event.payload.id);
			}),
			scope.listen<BackendTaskPayload>("task-cancelled", (event) => removeTask(event.payload.id)),
			scope.listen<BackendTaskPayload>("task-progress", (event) => {
				updateTask(event.payload.id, { percent: event.payload.percent ?? undefined });
			}),
		]);
		scope.add(...unlisten);
		const listed = await listBackendTasks();

		if (!scope.active) return;

		for (const item of listed) registerTask(fromBackend(item));
	}, t);
}
