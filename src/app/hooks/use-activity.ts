import { useEffect, useMemo } from "react";

import { useDownloadsContext } from "@/app/contexts/downloads-context";
import { useGames } from "@/app/contexts/games-context";
import { useStorage } from "@/app/contexts/storage-context";
import { useWorkspace, useWorkspaceActions } from "@/app/contexts/workspace-context";
import type { GameRuntime } from "@/app/selectors/game-presentation";
import type { DeployProgress, UpdateAllProgress, UpdateAllStage, UpdateProgress } from "@/bindings";
import { type TranslationKey, useTranslation } from "@/i18n";
import { clearFailedTask, type FailedTask, useFailedTasks } from "@/lib/failed-tasks-store";
import { errorText } from "@/lib/ipc";
import { formatSize } from "@/lib/mods";
import { type ImportState, useProgress } from "@/lib/progress-store";
import { dedupeKey, type Task, type TaskStages, taskDedupeOf } from "@/lib/tasks";
import {
	type RegisteredTask,
	registerBackendActions,
	unregisterBackendActions,
	useBackendTasks,
	useRegisteredTasks,
} from "@/lib/tasks-store";
import type { Download } from "@/nexus/use-downloads";

/// The steps each kind of work goes through, in order, as the stage codes the backend reports.
const IMPORT_STAGES = ["downloading", "hashing", "extracting", "copying"] as const;
const IMPORT_STAGE_NAME = {
	downloading: "tasks.stage.download",
	hashing: "tasks.stage.check",
	extracting: "tasks.stage.unpack",
	copying: "tasks.stage.copy",
} as const satisfies Record<ImportState["stage"], TranslationKey>;
const UPDATE_STAGES = ["downloading", "importing", "selecting", "switching"] as const;
const UPDATE_STAGE_NAME = {
	downloading: "tasks.stage.download",
	importing: "tasks.stage.install",
	selecting: "tasks.stage.select",
	switching: "tasks.stage.switch",
} as const satisfies Record<UpdateAllStage, TranslationKey>;
const DEPLOY_STAGES = ["preparing", "writing", "settings", "done"] as const;
const DEPLOY_STAGE_NAME = {
	preparing: "tasks.stage.prepare",
	writing: "tasks.stage.write",
	settings: "tasks.stage.settings",
} as const satisfies Record<Exclude<DeployProgress["stage"], "done">, TranslationKey>;

/// The track of one task: the names of its steps and the current one. `steps` lists the steps the track draws; a stage
/// after them (such as `done`) marks every step complete.
function stagesOf<S extends string>(steps: readonly S[], names: Partial<Record<S, TranslationKey>>, current: S, t: Translate): TaskStages {
	return {
		names: steps.flatMap((step) => {
			const name = names[step];
			return name ? [t(name)] : [];
		}),
		current: steps.indexOf(current),
	};
}

type Translate = ReturnType<typeof useTranslation>["t"];

/// The tasks of the whole window: the busy game action, the update check, the import queue, the provider downloads and
/// work registered by other games or dialogs. It is a hook rather than a provider, so progress re-renders the activity
/// card only, not the screens around it. Each group is a separate function, so its rule stays readable and testable.
export function useActivity(): Task[] {
	const { t, i18n } = useTranslation();
	const { busy, updateKind, updateModName, gameId, gameName, modUpdates } = useWorkspace();
	const { selectGame } = useWorkspaceActions();
	const { games } = useGames();
	const progress = useProgress();
	const failed = useFailedTasks();
	const registered = useRegisteredTasks();
	const { downloads, installDownload, removeDownload, retryDownload } = useDownloadsContext();
	const { loading: storageScanning } = useStorage();
	// Open fits every restored backend row: it brings the row's game back into view so the user can judge the failure.
	// Registered before the snapshot hydration effect, so the first listTasks answer already carries the action.
	useEffect(() => {
		registerBackendActions("*", {
			// Global work has no game to open, so it gets no dead button.
			open: (payload) => {
				const game = payload.game;
				return game ? { label: t("tasks.openWindow"), run: () => selectGame(game) } : undefined;
			},
		});
		return () => unregisterBackendActions("*");
	}, [selectGame, t]);
	useBackendTasks();

	return useMemo(() => {
		const local = [
			...busyTasks(
				busy,
				gameId,
				gameName,
				progress.deploy,
				progress.update,
				progress.updateAll,
				progress.updateAllCancel,
				updateKind,
				updateModName,
				t,
			),
			...localFailedTasks(failed, games),
			...updateTasks(modUpdates.checkingGames, games, t),
			...importTasks(progress.importing, gameId, gameName, t),
			...downloadTasks(downloads, i18n.language, { installDownload, removeDownload, retryDownload }, t),
			...storageTasks(storageScanning, t),
		];
		// Feature-owned registry rows (collections, packs, waiting dialogs) own their presentation. Backend lifecycle
		// rows are added only when no local row already describes the same operation: one apply must not appear twice.
		const byOperation = new Map<string, Task>();

		for (const task of local) byOperation.set(task.dedupeKey ?? task.id, task);

		for (const task of registryTasks(
			registered.filter((item) => item.source === "local"),
			games,
		)) {
			byOperation.set(task.dedupeKey ?? task.id, task);
		}

		for (const task of registryTasks(
			registered.filter((item) => item.source === "backend"),
			games,
		)) {
			const key = task.dedupeKey ?? task.id;

			if (!byOperation.has(key)) byOperation.set(key, task);
		}

		return [...byOperation.values()];
	}, [
		busy,
		failed,
		registered,
		gameId,
		games,
		progress,
		updateModName,
		updateKind,
		downloads,
		gameName,
		i18n.language,
		installDownload,
		modUpdates.checkingGames,
		removeDownload,
		retryDownload,
		storageScanning,
		t,
	]);
}

function busyTasks(
	busy: GameRuntime["busy"],
	gameId: string | undefined,
	gameName: string,
	deployProgress: DeployProgress | null,
	updateProgress: UpdateProgress | null,
	updateAllProgress: UpdateAllProgress | null,
	updateAllCancel: (() => void) | null,
	updateKind: "update" | "requirement",
	updateModName: string | null,
	t: Translate,
): Task[] {
	switch (busy) {
		case "applying":
			return [
				{
					id: "apply",
					kind: "apply",
					title: t("tasks.kind.apply"),
					detail: deployDetail(deployProgress, t),
					percent: deployProgress?.percent,
					stages: deployProgress ? stagesOf(DEPLOY_STAGES, DEPLOY_STAGE_NAME, deployProgress.stage, t) : undefined,
					state: "running",
					game: gameId,
					gameName,
					dedupeKey: dedupeKey({ Apply: { game: gameId ?? "" } }),
				},
			];
		case "launching":
			return [
				{
					id: "launch",
					kind: "launch",
					title: t("tasks.kind.launch"),
					state: "running",
					game: gameId,
					gameName,
					dedupeKey: `launch:${gameId ?? ""}`,
				},
			];
		case "stopping":
			return [
				{
					id: "stop",
					kind: "stop",
					title: t("tasks.kind.stop"),
					state: "running",
					game: gameId,
					gameName,
					dedupeKey: `stop:${gameId ?? ""}`,
				},
			];
		case "updating":
			if (updateAllProgress) {
				return [
					{
						id: "update-all",
						kind: "update",
						title: t("tasks.updatingAll", {
							done: Math.min(updateAllProgress.done + 1, updateAllProgress.total),
							total: updateAllProgress.total,
						}),
						detail: updateAllProgress.name,
						percent: updateAllProgress.percent,
						stages: stagesOf(UPDATE_STAGES, UPDATE_STAGE_NAME, updateAllProgress.stage, t),
						state: "running",
						game: gameId,
						gameName,
						onCancel: updateAllCancel ?? undefined,
						dedupeKey: dedupeKey({ UpdateAll: { game: gameId ?? "" } }),
					},
				];
			}

			return [
				{
					id: "update",
					kind: "update",
					title: t(updateKind === "requirement" ? "tasks.installingRequirement" : "tasks.updating", {
						name: updateModName ?? gameName,
					}),
					percent: updateProgress?.percent,
					stages: updateProgress ? stagesOf(UPDATE_STAGES.slice(0, 3), UPDATE_STAGE_NAME, updateProgress.stage, t) : undefined,
					state: "running",
					game: gameId,
					gameName,
					dedupeKey:
						updateKind === "requirement"
							? dedupeKey({ Requirement: { game: gameId ?? "" } })
							: dedupeKey({ Update: { game: gameId ?? "" } }),
				},
			];
		case null:
			return [];
		default:
			return [];
	}
}

/// A short description of the apply phase. The file count is the useful number while files are written; the other
/// phases say what is happening instead of showing a count that does not move.
function deployDetail(progress: DeployProgress | null, t: Translate): string | undefined {
	if (!progress) return undefined;

	switch (progress.stage) {
		case "preparing":
			return t("tasks.deployPreparing");
		case "writing":
			return progress.total > 0 ? t("tasks.deployFiles", { done: progress.done, total: progress.total }) : undefined;
		case "settings":
			return t("tasks.deploySettings");
		case "done":
			return t("tasks.deployDone");
		default:
			return undefined;
	}
}

/// The name of a game id from the catalogue, falling back to the id for a game just removed from it.
function gameNameOf(gameId: string | undefined | null, games: Array<{ id: string; name: string }>): string | undefined {
	if (!gameId) return undefined;
	return games.find((game) => game.id === gameId)?.name ?? gameId;
}

/// Every failure stays until it is retried or dismissed, regardless of the active game. The game name travels with the
/// row so the user can tell which game a retry will affect. A row with an identity shares its group with the backend
/// lifecycle row of the same work, so one failed import is not drawn twice.
export function localFailedTasks(tasks: readonly FailedTask[], games: Array<{ id: string; name: string }>): Task[] {
	return tasks.map((task) => ({
		id: task.id,
		kind: task.kind,
		title: task.title,
		detail: task.detail,
		game: task.game,
		gameName: gameNameOf(task.game, games),
		state: "failed" as const,
		onRetry: task.onRetry,
		onDismiss: () => clearFailedTask(task.id),
		dedupeKey: task.dedupeKey,
	}));
}

/// One operation can be described twice: a backend lifecycle row (stable code, no progress) and a feature-owned row
/// with the real Channel progress. Both are normalised to the same key so the richer local row wins. A backend row
/// carries the typed identity from its task builder; a row without one is normalised from the id by the shared helper.
export function registryDedupeKey(task: RegisteredTask): string {
	const dedupe = "dedupe" in task ? taskDedupeOf(task.dedupe) : undefined;
	return dedupeKey(dedupe ?? "Other", task.id, task.source === "backend", task.sourceKind ?? task.kind);
}

/// Work from the shared registry: a collection install, a waiting dialog or a backend task of any game. The callbacks
/// are copied as data, not wrapped, so their identity and the caller's `t` stay unchanged across a status answer.
function registryTasks(tasks: readonly RegisteredTask[], games: Array<{ id: string; name: string }>): Task[] {
	return tasks.map((task) => ({
		id: task.id,
		kind: task.kind,
		title: task.title,
		detail: task.detail,
		error: task.error,
		percent: task.percent,
		state: task.state,
		game: task.game,
		gameName: gameNameOf(task.game, games),
		onCancel: task.cancel?.run,
		cancelLabel: task.cancel?.label,
		onRetry: task.retry?.run,
		actionLabel: task.retry?.label,
		onOpen: task.open?.run,
		openLabel: task.open?.label,
		onDismiss: task.dismiss,
		dedupeKey: registryDedupeKey(task),
	}));
}

/// One row per game whose update check is running. Background checks of other games stay visible, and the game id in
/// the key keeps two concurrent checks from collapsing into one row.
export function updateTasks(checkingGames: readonly string[], games: Array<{ id: string; name: string }>, t: Translate): Task[] {
	return checkingGames.map((gameId) => {
		const name = gameNameOf(gameId, games) ?? gameId;
		return {
			id: `checking:${gameId}`,
			kind: "update" as const,
			title: t("tasks.checkingUpdates"),
			detail: name,
			game: gameId,
			gameName: name,
			state: "running" as const,
			dedupeKey: `checking:${gameId}`,
		};
	});
}

function storageTasks(scanning: boolean, t: Translate): Task[] {
	return scanning ? [{ id: "storage", kind: "scan", title: t("tasks.scanningStorage"), state: "running" }] : [];
}

function importTasks(importing: ReadonlyMap<string, ImportState>, gameId: string | undefined, gameName: string, t: Translate): Task[] {
	return [...importing].map(([path, item]) => ({
		id: `import:${path}`,
		kind: "import" as const,
		title: t("tasks.importTitle", { name: item.name }),
		detail: item.total > 1 ? t("tasks.importCount", { index: item.index, total: item.total }) : undefined,
		percent: item.percent,
		stages: stagesOf(IMPORT_STAGES, IMPORT_STAGE_NAME, item.stage, t),
		state: "running" as const,
		game: gameId,
		gameName,
		onCancel: item.onCancel,
		dedupeKey: dedupeKey({ Import: { game: gameId ?? "", path } }),
	}));
}

function downloadTasks(
	downloads: Download[],
	locale: string,
	actions: {
		installDownload: (download: Download) => void | Promise<void>;
		removeDownload: (id: number) => void | Promise<void>;
		retryDownload: (id: number) => void | Promise<void>;
	},
	t: Translate,
): Task[] {
	const tasks: Task[] = [];

	for (const download of downloads) {
		if (download.state === "downloading") {
			tasks.push({
				id: `download:${download.id}`,
				kind: "download",
				title: t("tasks.downloadTitle", { name: download.name }),
				detail:
					download.total_kb > 0
						? t("tasks.downloadDetail", {
								received: formatSize(download.received_kb, locale),
								total: formatSize(download.total_kb, locale),
							})
						: undefined,
				percent: download.total_kb > 0 ? Math.min(100, Math.floor((download.received_kb * 100) / download.total_kb)) : undefined,
				state: "running",
				onCancel: () => void actions.removeDownload(download.id),
			});
		} else if (download.state === "installing") {
			tasks.push({
				id: `install:${download.id}`,
				kind: "install",
				title: t("tasks.installing", { name: download.name }),
				state: "running",
			});
		} else if (download.state === "failed") {
			tasks.push({
				id: `download:${download.id}`,
				kind: "download",
				title: t("tasks.downloadFailed", { name: download.name }),
				detail: download.error ? errorText(download.error) : undefined,
				state: "failed",
				onRetry: () => void actions.retryDownload(download.id),
			});
		} else if (download.state === "ready" && !download.auto_install) {
			tasks.push({
				id: `install:${download.id}`,
				kind: "install",
				title: t("tasks.waitingInstall", { name: download.name }),
				state: "waiting",
				onRetry: () => void actions.installDownload(download),
				actionLabel: t("downloads.install"),
			});
		}
	}

	return tasks;
}
