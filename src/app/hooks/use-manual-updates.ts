import { useCallback, useEffect, useRef, useState } from "react";
import { useNoticeActions } from "@/app/contexts/notices-context";
import type { AppDialog } from "@/app/contexts/ui-context";
import { useWorkspace, useWorkspaceActions } from "@/app/contexts/workspace-context";
import {
	type AppError,
	commands,
	type ManualStep,
	type ManualTarget_Deserialize,
	type ManualUpdateView_Serialize,
	type UpdateChange,
} from "@/bindings";
import { type TranslationKey, useTranslation } from "@/i18n";
import { translateCode, unwrap } from "@/lib/ipc";
import { needsGameSwitch } from "@/lib/task-open";
import { registerTask, removeTask, updateTask } from "@/lib/tasks-store";
import { useTauriListener } from "@/lib/use-tauri-listener";
import type { Download } from "@/sources/downloads";
import type { UpdateTarget } from "@/sources/update-targets";

/// The current-line text for the states without a live percentage or an error.
const STEP_STATUS: Record<Exclude<ManualStep, "downloading" | "failed">, TranslationKey> = {
	waiting: "updates.manual.waiting",
	installing: "updates.manual.installing",
	selecting: "updates.manual.selecting",
	selected: "updates.manual.selected",
	skipped: "updates.manual.skipped",
};

/// The actions the view still needs. They are captured when the run starts, while the owning game is still active, so
/// page opening and the finish handoff keep using that game after the user switches away. The sequence itself is owned
/// by the backend, not by these callbacks.
export interface ManualActions {
	openPage: (target: UpdateTarget) => void;
	selectLatest: (target: UpdateTarget) => void | Promise<void>;
	describeDownloadError: (error: AppError) => string;
	describeSelectionError: (error: unknown) => string;
	matchesDownload: (target: UpdateTarget, download: Download) => boolean;
	finish: (gameId: string, changes: UpdateChange[], failures: string[]) => void;
}

export interface ManualUpdatesView {
	active: boolean;
	targets: UpdateTarget[];
	steps: Record<string, ManualStep>;
	errors: Record<string, string>;
	index: number;
	current: UpdateTarget | undefined;
	percent: number | null;
	status: string;
}

const taskId = (gameId: string) => `manual-update:${gameId}`;
const PAGE_EVENT = "manual-update-open-page";

/// Turns a backend view into the shape the dialog draws. Backend error codes are translated here so no new i18n key is
/// needed; the live download percentage is overlaid from the queue the window already follows.
function toView(
	backend: ManualUpdateView_Serialize,
	matchesDownload: (target: UpdateTarget, download: Download) => boolean,
	downloads: Download[],
	t: ReturnType<typeof useTranslation>["t"],
): ManualUpdatesView {
	const targets: UpdateTarget[] = backend.targets.map((target) => ({
		id: target.id,
		name: target.name,
		folder: target.folder,
		provider: target.provider,
		modId: target.mod_id,
		current: target.current,
		latest: target.latest,
		latestFileId: null,
		installed: target.installed,
	}));
	const steps: Record<string, ManualStep> = {};

	for (const [id, step] of Object.entries(backend.steps)) steps[id] = step;
	const errors: Record<string, string> = {};

	for (const [id, code] of Object.entries(backend.errors)) errors[id] = translateCode(code);
	const index = backend.index;
	const current = targets[index];
	let livePercent: number | null = null;

	if (current) {
		const match = downloads
			.filter((download) => matchesDownload(current, download))
			.reduce<Download | null>((left, right) => (!left || right.id > left.id ? right : left), null);
		if (match && match.total_kb > 0) livePercent = Math.min(100, Math.floor((match.received_kb * 100) / match.total_kb));
	}

	const state = current ? (steps[current.id] ?? "waiting") : "waiting";
	const status = (() => {
		if (state === "downloading") {
			return livePercent === null ? t("updates.manual.downloading") : t("updates.manual.downloadingPercent", { percent: livePercent });
		}

		if (state === "failed") return errors[current?.id ?? ""] ?? t("updates.manual.failedShort");
		return t(STEP_STATUS[state]);
	})();
	return {
		active: backend.active,
		targets,
		steps,
		errors,
		index,
		current,
		percent: livePercent ?? backend.percent,
		status,
	};
}

function toBackendTarget(target: UpdateTarget): ManualTarget_Deserialize {
	return {
		id: target.id,
		name: target.name,
		provider: target.provider,
		mod_id: target.modId,
		folder: target.folder,
		current: target.current,
		latest: target.latest,
		installed: target.installed,
	};
}

/// The browser step of **Update all** as a view over the backend controller. It listens for page requests, reports
/// finished downloads with a command and renders the status the backend returns. It holds no session, step map, index
/// or undo list of its own, so closing the dialog or restarting the window cannot lose the sequence.
export function useManualUpdates({
	downloads,
	openDialog,
	closeDialogIfManual,
	selectGame,
	activeGameId,
}: {
	downloads: Download[];
	openDialog: (dialog: AppDialog) => void;
	closeDialogIfManual: () => void;
	selectGame?: (gameId: string) => void;
	activeGameId?: string;
}) {
	const { t } = useTranslation();
	const { notifyError } = useNoticeActions();
	const { gameId: workspaceGameId } = useWorkspace();
	const { selectGame: workspaceSelectGame } = useWorkspaceActions();
	const shownGame = activeGameId ?? workspaceGameId;
	const switchGame = selectGame ?? workspaceSelectGame;

	const [backend, setBackend] = useState<ManualUpdateView_Serialize | null>(null);
	const [matcher, setMatcher] = useState<(target: UpdateTarget, download: Download) => boolean>(() => () => false);
	const actionsRef = useRef<ManualActions | null>(null);
	const gameRef = useRef<string | null>(null);
	const startedRef = useRef<{ game: string; ids: string } | null>(null);
	const reported = useRef<Set<string>>(new Set());
	const pendingOpen = useRef<{ game: string; targets: UpdateTarget[] } | null>(null);
	/// The current target for the event/callback handlers, without making a changing view a hook dependency.
	const currentRef = useRef<UpdateTarget | undefined>(undefined);

	const view = backend
		? toView(backend, matcher, downloads, t)
		: ({
				active: false,
				targets: [],
				steps: {},
				errors: {},
				index: 0,
				current: undefined,
				percent: null,
				status: "",
			} satisfies ManualUpdatesView);
	useEffect(() => {
		currentRef.current = view.current;
	});

	/// The Stop action of the working task ends the sequence but leaves downloads that already started to import
	/// themselves, exactly like the dialog's Close did before.
	const stop = useCallback(() => {
		const game = gameRef.current;

		if (!game) return;
		void commands.manualUpdateCancel(game);
		removeTask(taskId(game));
		startedRef.current = null;
		gameRef.current = null;
		setBackend(null);
		closeDialogIfManual();
	}, [closeDialogIfManual]);
	const stopRef = useRef(stop);
	useEffect(() => {
		stopRef.current = stop;
	}, [stop]);

	/// Open from Working must switch to the run's game before showing the browser step. A game switch closes every shell
	/// dialog, so the dialog opens only after that game is active.
	const openManualDialog = useCallback(
		(game: string, targets: UpdateTarget[]) => {
			if (needsGameSwitch(game, shownGame)) {
				pendingOpen.current = { game, targets };
				switchGame(game);
				return;
			}

			void commands.manualUpdateReopen(game);
			openDialog({ kind: "manual-updates", targets });
		},
		[openDialog, shownGame, switchGame],
	);
	const openManualDialogRef = useRef(openManualDialog);
	useEffect(() => {
		openManualDialogRef.current = openManualDialog;
	}, [openManualDialog]);
	const openFromTask = useCallback((game: string, targets: UpdateTarget[]) => openManualDialogRef.current(game, targets), []);
	useEffect(() => {
		const pending = pendingOpen.current;

		if (!pending || pending.game !== shownGame) return;
		pendingOpen.current = null;
		void commands.manualUpdateReopen(pending.game);
		openDialog({ kind: "manual-updates", targets: pending.targets });
	}, [openDialog, shownGame]);

	/// Starts one backend run per dialog. A second dialog with the same targets is the Open action bringing the wizard
	/// back, so it must not reset the sequence. Different targets replace the previous run.
	const start = useCallback(
		async (targets: UpdateTarget[], game: string, actions: ManualActions) => {
			const started = startedRef.current;
			const ids = targets.map((target) => target.id).join("|");

			if (started?.game === game && started.ids === ids) return;

			if (started) removeTask(taskId(started.game));
			actionsRef.current = actions;
			gameRef.current = game;
			startedRef.current = { game, ids };
			setMatcher(() => actions.matchesDownload);
			reported.current = new Set();
			setBackend(
				await commands.startManualUpdates(
					game,
					targets.map((target) => toBackendTarget(target)),
				),
			);
			registerTask({
				id: taskId(game),
				game,
				kind: "update",
				title: t("tasks.manualUpdates"),
				detail: t("tasks.waitingForYou"),
				percent: 0,
				state: "waiting",
				cancel: { label: t("tasks.stop"), run: () => stopRef.current() },
				open: {
					label: t("tasks.openWindow"),
					run: () => openFromTask(game, targets),
				},
				source: "local",
			});
		},
		[openFromTask, t],
	);

	/// The window listens for page requests from the backend and opens the matching target through the captured action.
	/// A browser preview has no events; the backend still owns whatever run the real window started.
	useTauriListener(async (scope) => {
		const off = await scope.listen<{
			game: string;
			id: string;
			name: string;
			provider: string;
		}>(PAGE_EVENT, (event) => {
			const actions = actionsRef.current;

			if (!actions || event.payload.game !== gameRef.current) return;
			// The page request names the mod and its provider; the normal page action only needs those two fields.
			actions.openPage({
				id: event.payload.id,
				name: event.payload.name,
				provider: event.payload.provider,
				modId: "",
				folder: "",
				current: null,
				latest: null,
				latestFileId: null,
				installed: false,
			});
		});
		scope.add(off);
	});

	/// A matching download means the user pressed the provider's button. Report the finished one to the backend, which
	/// selects the version and drives the next page; transient steps keep the list honest while the download runs.
	useEffect(() => {
		const game = gameRef.current;
		const actions = actionsRef.current;
		const current = currentRef.current;

		if (!(game && actions && current)) return;
		const matches = downloads.filter((download) => actions.matchesDownload(current, download));
		const latest = matches.reduce<Download | null>((left, right) => (!left || right.id > left.id ? right : left), null);

		if (!latest) return;

		if (latest.state === "failed") {
			if (!reported.current.has(`${current.id}:failed`)) {
				reported.current.add(`${current.id}:failed`);

				if (latest.error) notifyError(actions.describeDownloadError(latest.error));
				void commands.manualUpdateStep(game, current.id, "failed");
			}

			return;
		}

		if (latest.state === "installed") {
			void (async () => {
				setBackend(await unwrap(commands.manualUpdateDownloadDone(game, current.id)));
			})();
			return;
		}

		void commands.manualUpdateStep(game, current.id, latest.state === "downloading" ? "downloading" : "installing");
	}, [downloads, notifyError]);

	/// A run with no failures closes itself once every mod is updated or skipped. A failure stays on screen with its
	/// message and a Finish button; `finish` closes the dialog, so this cannot restart a settled run.
	useEffect(() => {
		if (!backend?.active || backend.index < backend.targets.length) return;

		if (Object.keys(backend.errors).length > 0) return;
		const game = gameRef.current;
		const actions = actionsRef.current;

		if (!(game && actions)) return;
		void (async () => {
			const handoff = await commands.manualUpdateFinish(game);
			removeTask(taskId(game));
			startedRef.current = null;
			gameRef.current = null;
			setBackend(null);
			actions.finish(handoff.game, handoff.changes, handoff.failures);
		})();
	}, [backend]);

	/// The percentage in the Working panel follows how many mods are settled, not how many pages were opened.
	useEffect(() => {
		const game = gameRef.current;

		if (!(game && backend) || backend.targets.length === 0) return;
		updateTask(taskId(game), { percent: backend.percent });
	}, [backend]);

	/// A manual Next skips a mod the user does not want to touch.
	const next = useCallback(async () => {
		const game = gameRef.current;

		if (!game) return;
		setBackend(await commands.manualUpdateNext(game));
	}, []);

	const openCurrentPage = useCallback(() => {
		const current = currentRef.current;
		const actions = actionsRef.current;

		if (current && actions) actions.openPage(current);
	}, []);

	const finish = useCallback(async () => {
		const game = gameRef.current;
		const actions = actionsRef.current;

		if (!(game && actions)) return;
		const handoff = await commands.manualUpdateFinish(game);
		removeTask(taskId(game));
		startedRef.current = null;
		gameRef.current = null;
		setBackend(null);
		actions.finish(handoff.game, handoff.changes, handoff.failures);
	}, []);

	return { view, start, next, finish, stop, openCurrentPage };
}
