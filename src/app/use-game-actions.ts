import { type RefObject, useCallback, useEffect, useRef, useState } from "react";
import type { GameRuntime } from "@/app/selectors/game-presentation";
import {
	commands,
	type DeployProgress,
	type ManagedGame,
	type Message,
	type ModGroup,
	type ModInfoEdit,
	type RelationKind,
	type Relations,
	type Settings as SettingsData,
	type SourceRef,
	type SourceRequirement,
	type Status,
	type UpdateAllProgress,
	type UpdateChange,
	type UpdateProgress,
	type UpdateReport,
} from "@/bindings";
import { useTranslation } from "@/i18n";
import { clearFailedTask, recordFailedTask } from "@/lib/failed-tasks-store";
import type { FomodChoice } from "@/lib/fomod";
import { type CommandResult, codeOf, messageOf, needsChoicePathOf, paymentRequiredOf, translate, unwrap } from "@/lib/ipc";
import { knownRows, type ListEdit, listItems, pagePartsOf, statusItems, toRows } from "@/lib/mods";
import { createGameQueues, setDeployProgress, setUpdateAllCancel, setUpdateAllProgress, setUpdateProgress } from "@/lib/progress-store";
import { createQueue } from "@/lib/serial-queue";
import { type BackendTaskPayload, registerBackendActions, registerTask, removeTask, unregisterBackendActions } from "@/lib/tasks-store";
import { newProgressChannel, pickFiles } from "@/lib/tauri";
import { useLatestRef } from "@/lib/use-latest-ref";
import type { useNotices } from "@/lib/use-notices";
import { useTauriListener } from "@/lib/use-tauri-listener";
import { splitUpdateTargets, type UpdateTarget, updateTargets } from "@/sources/update-targets";

/// The translated text of each backend message, joined with `separator`.
function translateJoin(messages: Message[], separator: string): string {
	return messages.map((message) => translate(message)).join(separator);
}

/// The payload of the backend's `tool-exited` event. The tool's Working row is owned by the backend task registry.
interface ToolExitedPayload {
	game: string;
	tool: string;
	code: number | null;
	cancelled: boolean;
}

/// The busy presentation belongs to a game, so a long action keeps its own rows and cancel callbacks when the user
/// switches away. Only the shown game's values are exposed; the map holds the others.
interface GameBusy {
	busy: GameRuntime["busy"];
	updateKind: "update" | "requirement";
	updateModName: string | null;
}

const IDLE_BUSY: GameBusy = {
	busy: null,
	updateKind: "update",
	updateModName: null,
};

export function useGameActions({
	gameId: id,
	gameRunning,
	setStatusFor,
	latestStatus,
	statusOf,
	setManaged,
	setGameId,
	setSelection,
	nameOf,
	onNoGames,
	onGameAdded,
	forget,
	forgetAll,
	attempt,
	notify,
	notifyDurable,
	notifyError,
	notifyFailure,
	openManualUpdates,
	currentBrowserProviders,
	refreshProviders,
	installWithChoices,
}: Pick<ReturnType<typeof useNotices>, "attempt" | "notify" | "notifyDurable" | "notifyError"> & {
	gameId: string | undefined;
	/// The game runs with the list it started with; a profile switch changes only the list shown and edited.
	gameRunning: boolean;
	setStatusFor: (gameId: string, status: Status) => boolean;
	/// The status of the shown game, kept current synchronously by the workspace.
	latestStatus: RefObject<{ game: string; status: Status } | null>;
	/// The newest status of any game, from the workspace cache. A queued action for a background game reads its own base
	/// status instead of early-returning because another game is shown. Optional for callers without a cache.
	statusOf?: (gameId: string) => Status | null;
	setManaged: (games: ManagedGame[]) => void;
	setGameId: (gameId: string) => void;
	setSelection: (modIds: string[]) => void;
	nameOf: (gameId: string) => string;
	/// Called when the active game is removed or the added list becomes empty, so the games page opens.
	onNoGames: () => void;
	/// Called after a game is added: the first game opens its mod list, and with it the setup check.
	onGameAdded: () => void;
	forget: (modId: string) => void;
	forgetAll: () => void;
	/// Opens the browser step of a bulk update.
	openManualUpdates: (targets: UpdateTarget[]) => void;
	/// The providers the backend routes to the files page, from the last backend read. An action that chains on a key
	/// save uses this, because React may not have committed the new context value when the action runs.
	currentBrowserProviders: () => ReadonlySet<string>;
	/// Reads the provider list again, after a download showed that the saved account cannot download directly.
	refreshProviders: () => Promise<void>;
	/// Opens the import of an archive whose installer needs the FOMOD wizard (an update of a mod with options).
	installWithChoices: (gameId: string, path: string) => void;
	/// Reports a failed call with its translated text and its technical detail.
	notifyFailure: (failure: unknown) => void;
}) {
	const { t } = useTranslation();
	/// The busy presentation of every game. Only the shown game's row is read below, but a switch no longer clears the
	/// work of the game left behind: its busy flag, progress and cancel callback stay with it.
	const [busyByGame, setBusyByGame] = useState<ReadonlyMap<string, GameBusy>>(() => new Map());
	const [displayGame, setDisplayGame] = useState(id);
	/// The visible row is derived, not written by a long action: a retry from an older render updates the map under its
	/// own game id, so it can never put that game's busy flag into the game that happens to be shown now.
	const shown = busyByGame.get(displayGame ?? "") ?? IDLE_BUSY;

	if (displayGame !== id) setDisplayGame(id);
	const setBusyOf = useCallback((gameId: string | undefined, patch: Partial<GameBusy>) => {
		if (!gameId) return;
		setBusyByGame((previous) => {
			const next = new Map(previous);
			next.set(gameId, { ...(previous.get(gameId) ?? IDLE_BUSY), ...patch });
			return next;
		});
	}, []);

	/// Profile edits go through one queue per game. Rapid toggles of one game cannot overwrite each other with an older
	/// status reply, and a long edit of one game no longer blocks another game's queue. The tail of each queue never
	/// rejects: a failed edit was already reported by its caller, and the next edit starts from the last successful state.
	const [gameQueues] = useState(createGameQueues);
	/// The optional game id keeps existing callers working; a long action passes the id it captured before its first await.
	const queueProfile = useCallback(
		(action: () => Promise<unknown>, gameId?: string) => gameQueues.run(action, gameId ?? id ?? ""),
		[gameQueues, id],
	);
	/// The queue-shaped handle the workspace uses for `idle()`. With no id it waits every game's queue; with an id only
	/// that game's, so an import for game B does not wait for a long edit of game A.
	const profileQueue = {
		run: queueProfile,
		idle: (gameId?: string) => gameQueues.idle(gameId),
	};
	/// Settings saves and the changes to a settings list (a Proton switch, the profile data) share one queue, apart from
	/// profile edits. Each one reads the newest settings when its turn comes, so a save never sends a list that an earlier
	/// change has already moved on from.
	const [settingsQueue] = useState(createQueue);

	/// Actions apply to the selected game, so its id goes to every command. Errors go to the notices. The boolean says
	/// whether the action worked, so a caller can chain a launch only after a deployment that succeeded.
	const forGameResult = useCallback(
		async (action: (gameId: string) => Promise<void>, reportFailure = true) => {
			if (!id) return false;
			await gameQueues.forGame(id).idle();

			if (reportFailure) return attempt(() => action(id));
			// The caller owns a durable failure row (apply): a second, transient error toast would report the same event.
			try {
				await action(id);
				return true;
			} catch {
				return false;
			}
		},
		[attempt, gameQueues, id],
	);

	/// Most callers only need the side effect and the notices; the boolean stays internal to the actions that chain.
	const forGame = useCallback(
		async (action: (gameId: string) => Promise<void>) => {
			await forGameResult(action);
		},
		[forGameResult],
	);

	/// A list or profile mutation of the shown game. It joins the profile queue, so a switch and a quick toggle cannot
	/// overwrite each other with rows computed from the old profile. The game id is taken before the queue waits.
	const queued = (action: (gameId: string) => Promise<void>) => {
		const gameId = id;

		if (!gameId) return Promise.resolve();
		return gameQueues.forGame(gameId).run(() => attempt(() => action(gameId)));
	};

	/// A queued command that answers with the game's status. `notice` is said once it worked.
	const changeQueued = (call: (gameId: string) => Promise<CommandResult<Status>>, notice?: string) =>
		queued(async (gameId) => {
			setStatusFor(gameId, await unwrap(call(gameId)));

			if (notice) notify(notice);
		});

	/// The newest status of `gameId`, also when it has not been drawn or is no longer shown. The workspace cache keeps a
	/// background game's list so a queued edit of it still has its base status and does not silently early-return.
	const currentStatus = (gameId: string) => {
		if (statusOf) return statusOf(gameId);
		const current = latestStatus.current;
		return current?.game === gameId ? current.status : null;
	};

	/// The id of the profile the list of `gameId` shows, from its newest status.
	const activeProfileId = (gameId: string) => currentStatus(gameId)?.profiles.find((entry) => entry.active)?.id;

	/// A different list has other selected mods and other update results, so both are dropped.
	const resetListSelection = () => {
		setSelection([]);
		forgetAll();
	};

	/// Reads the game's status again, after a command that answers with something else.
	const reload = useCallback(
		async (gameId: string) => {
			setStatusFor(gameId, await unwrap(commands.syncProfiles(gameId)));
		},
		[setStatusFor],
	);

	/// Reads the status again after a command that already worked. A failed read is reported, but it must not turn the
	/// command itself into a failure.
	const reloadAfter = async (gameId: string) => {
		try {
			await reload(gameId);
		} catch (failure) {
			notifyFailure(failure);
		}
	};

	/// The newest status of the shown game. The actions read it when they run, not from the render, so they keep their
	/// identity across status answers and the memoized list rows that take them do not render again.
	const shownStatus = () => (id ? currentStatus(id) : null);

	const toolName = (toolId: string) => shownStatus()?.tools.find((tool) => tool.id === toolId)?.name ?? toolId;

	const addGame = async (target: string, gameRoot: string) =>
		attempt(async () => {
			setManaged(await unwrap(commands.addGame(target, gameRoot)));
			setGameId(target);
			notify(t("notice.gameAdded", { name: nameOf(target) }));
			onGameAdded();
		});

	const removeGame = (target: string) => {
		void attempt(async () => {
			const left = await unwrap(commands.removeGame(target));
			setManaged(left);
			// Removing the game being shown must leave its pages: the fallback would silently be another game.
			if (left.length === 0 || target === id) onNoGames();
			notify(t("notice.gameRemoved", { name: nameOf(target) }));
		});
	};

	/// A list edit applies to the list as the edits before it left it, so two quick clicks never undo each other.
	const saveProfile = (edit: ListEdit) =>
		queued(async (gameId) => {
			const current = currentStatus(gameId);

			if (!current) return;
			const rows = edit(statusItems(current));

			if (!rows) return;
			const fresh = await unwrap(commands.saveProfile(gameId, rows, knownRows(current)));
			// The user may have moved to another game while the edit ran.
			if (currentStatus(gameId)) setStatusFor(gameId, fresh);
		});

	/// List edits are owned by the backend now: each command answers with the fresh status and joins the profile queue
	/// through `changeQueued`, so the workspace sees the new list and a quick second edit starts from it. These wrappers
	/// keep their identity across status answers, which the memoized rows rely on.
	const moveMod = (modId: string, target: string, side: "above" | "below") =>
		changeQueued((gameId) => commands.moveMod(gameId, modId, target, side));
	const setModEnabled = (modId: string, enabled: boolean) => changeQueued((gameId) => commands.setModEnabled(gameId, modId, enabled));
	/// The menu calls this for "sort alphabetically": the backend sorts by name and moves groups as one block.
	const sortList = () => changeQueued((gameId) => commands.sortListByName(gameId));
	const groupMods = (ids: string[]) => changeQueued((gameId) => commands.groupMods(gameId, ids));
	const ungroupMods = (groupId: string) => changeQueued((gameId) => commands.ungroupMods(gameId, groupId));
	const setSeparator = (separatorId: string, name: string) => changeQueued((gameId) => commands.setSeparator(gameId, separatorId, name));
	const setModGroup = (modId: string, groupId: string | null) => changeQueued((gameId) => commands.setModGroup(gameId, modId, groupId));

	/// A change to one mod that also drops this session's update failure for it.
	const changeMod = async (modId: string, call: (gameId: string) => Promise<CommandResult<Status>>, notice?: string) => {
		await changeQueued(call, notice);
		forget(modId);
	};

	const selectVersion = (modId: string, folder: string) => changeMod(modId, (gameId) => commands.setModVersion(gameId, modId, folder));

	/// The game whose apply waits on the global state lock. The single task listener reads this ref to match a finish
	/// event to the waiting row, without resubscribing when the shown game or language changes.
	const waitingApplyGame = useRef<string | null>(null);
	/// One auto-retry per waiting row: several finish events must not start the same apply twice.
	const autoRetrying = useRef(false);
	/// Applies one game's active profile. The game is a parameter, not the shown game, so a Working row restored after
	/// a restart can retry the apply it belongs to even while another game is on screen. Its failure row is the only
	/// report: no second toast is raised for an apply the window is tracking.
	const applyFor = async (target: string | undefined): Promise<boolean> => {
		if (!target) return false;
		const gameId = target;
		await gameQueues.forGame(gameId).idle();
		waitingApplyGame.current = null;
		clearFailedTask(`apply:${gameId}`);
		// A retry replaces the waiting row with a real attempt; success or another failure leaves no waiting row.
		removeTask(`apply:${gameId}`);
		setDeployProgress(null, gameId);
		setBusyOf(gameId, { busy: "applying" });
		const progress = newProgressChannel<DeployProgress>((step) => setDeployProgress(step, gameId));
		let detail: string | undefined;
		let errorCode: string | null = null;
		let deployed = false;

		try {
			const applied = await unwrap(commands.apply(gameId, progress));
			notify(t("notice.deployedOverlay", { files: applied.files }));
			await reloadAfter(gameId);
			deployed = true;
		} catch (failure) {
			detail = messageOf(failure);
			errorCode = codeOf(failure);
		}

		setBusyOf(gameId, { busy: null });
		setDeployProgress(null, gameId);

		if (!deployed) {
			if (errorCode === "state.busy") {
				// The global lock belongs to another apply. Show a waiting row instead of a failure and retry when that
				// apply finishes; the backend lock order stays untouched.
				waitingApplyGame.current = gameId;
				clearFailedTask(`apply:${gameId}`);
				registerTask({
					id: `apply:${gameId}`,
					game: gameId,
					kind: "apply",
					state: "waiting",
					title: t("tasks.waitingForApply"),
					detail,
					retry: { label: t("common.retry"), run: () => void applyFor(gameId) },
					source: "local",
				});
			} else {
				recordFailedTask({
					id: `apply:${gameId}`,
					kind: "apply",
					game: gameId,
					title: t("tasks.applyFailed"),
					detail,
					onRetry: () => {
						void applyFor(gameId);
					},
				});
			}
		}

		return deployed;
	};
	const runDeploy = () => applyFor(id);

	/// The listener and the registered Working-row action read the newest `applyFor` after a game switch or a language
	/// change without resubscribing or re-registering.
	const latestApply = useRef(applyFor);
	useEffect(() => {
		latestApply.current = applyFor;
	});
	useEffect(() => {
		registerBackendActions("apply", {
			retry: (payload) => ({
				label: t("common.retry"),
				run: () => {
					void latestApply.current(payload.game ?? undefined);
				},
			}),
		});
		return () => unregisterBackendActions("apply");
	}, [t]);

	/// The tool-exited handler reads the newest callbacks without resubscribing on a game switch or a language change.
	const latestTool = useLatestRef({ reload, statusOf, notify, t });

	// A browser preview has no events, so there is no other apply or tool to wait for.
	useTauriListener(async (scope) => {
		const unlisteners = await Promise.all([
			scope.listen<BackendTaskPayload>("task-finished", (event) => {
				if (!scope.active || autoRetrying.current) return;
				const waitingGame = waitingApplyGame.current;

				if (!waitingGame) return;

				if (event.payload.kind !== "apply" || event.payload.game !== waitingGame) return;
				autoRetrying.current = true;
				waitingApplyGame.current = null;
				removeTask(`apply:${waitingGame}`);
				void latestApply.current(waitingGame).then(
					() => {
						autoRetrying.current = false;
					},
					() => {
						autoRetrying.current = false;
					},
				);
			}),
			scope.listen<ToolExitedPayload>("tool-exited", (event) => {
				if (!scope.active) return;
				const { reload: reloadNow, statusOf: statusNow, notify: notifyNow, t: translateNow } = latestTool.current;
				const { game, tool, code, cancelled } = event.payload;
				const name = statusNow?.(game)?.tools.find((item) => item.id === tool)?.name ?? tool;

				if (cancelled) notifyNow(translateNow("notice.toolCancelled", { name }));
				else
					notifyNow(
						translateNow(code === 0 ? "notice.toolDone" : "notice.toolFailed", {
							name,
							code: code ?? -1,
						}),
					);
				void reloadNow(game);
			}),
		]);
		scope.add(...unlisteners);
	});

	const undo = () =>
		queued(async (gameId) => {
			const warnings = await unwrap(commands.undo(gameId));
			notify(
				warnings.length > 0
					? t("notice.undoneWarnings", {
							warnings: translateJoin(warnings, "; "),
						})
					: t("notice.undone"),
			);
			await reloadAfter(gameId);
		});

	/// Drops the draft changes: the deployed mods are enabled in their deployed order, the rest are disabled, and the
	/// separators stay with their mods. The deployment itself is not touched, so this only edits the list. The rows are
	/// computed when the edit reaches the queue, from the newest status, not from the render that clicked the button.
	const discardChanges = () =>
		queued(async (gameId) => {
			const current = currentStatus(gameId);

			if (!current) return;
			const byId = new Map(current.mods.map((mod) => [mod.id, mod]));
			const deployed = current.deployed_list.flatMap((row) => {
				const mod = byId.get(row.id);
				return mod ? [{ ...mod, folder: row.folder, enabled: true }] : [];
			});
			const kept = new Set(deployed.map((mod) => mod.id));
			const rest = current.mods.filter((mod) => !kept.has(mod.id)).map((mod) => ({ ...mod, enabled: false }));
			const rows = toRows(listItems([...deployed, ...rest], current.separators));
			setStatusFor(gameId, await unwrap(commands.saveProfile(gameId, rows, knownRows(current))));
			notify(t("notice.changesDiscarded"));
		});

	/// The game starts from Gobbo (through Proton on Linux). Without a deployment it starts with no mods, so we say so plainly.
	/// Starting the game deploys the shown profile first when it is not the deployed one, so the status is read again.
	const launchNow = (executable: string | null, applyDraft = false) =>
		forGame(async (gameId) => {
			const withMods = await unwrap(commands.launchGame(gameId, executable, applyDraft));
			// A start with mods needs no notice: the header shows that the game runs. Only a start without mods is worth one.
			if (!withMods) notify(t("notice.launchedWithoutMods"));
			await reloadAfter(gameId);
		});
	const runLaunch = async (executable: string | null, applyDraft = false) => {
		const gameId = id;
		setBusyOf(gameId, { busy: "launching" });
		await launchNow(executable, applyDraft);
		setBusyOf(gameId, { busy: null });
	};

	/// Puts the previous version back after an in-app update. The new version stays installed, so the user can switch
	/// again from the picker.
	const undoUpdate = (gameId: string, modId: string, folder: string) => {
		void queueProfile(
			() =>
				attempt(async () => {
					if (!currentStatus(gameId)) return;
					setStatusFor(gameId, await unwrap(commands.setModVersion(gameId, modId, folder)));
					forget(modId);
					notify(t("notice.updateUndone"));
				}),
			gameId,
		);
	};

	/// Puts back the version every mod of a bulk update selected before it ran. Version selection is not a list edit, so
	/// one toast action can undo the whole run.
	const undoUpdateAll = (gameId: string, changes: UpdateChange[]) => {
		void queueProfile(
			() =>
				attempt(async () => {
					const start = currentStatus(gameId);

					if (!start) return;
					let fresh = start;

					for (const change of changes) {
						fresh = await unwrap(commands.setModVersion(gameId, change.id, change.previous));
					}

					setStatusFor(gameId, fresh);
					notify(t("notice.updatesUndone", { count: changes.length }));
				}),
			gameId,
		);
	};

	/// Opens the files page of a library mod. The backend reads the game key and the provider mod id, so a page opens even
	/// when the window's game catalog is older than the definition on disk.
	const openUpdatePage = (modId: string, provider: string) =>
		forGameResult(async (gameId) => {
			await unwrap(commands.openUpdatePage(gameId, modId, provider, "files"));
		});

	/// The busy state of an update or a requirement install. The progress of an earlier run is cleared first. The game
	/// id is captured by the caller before its first await, so a switch cannot mix two runs' presentation.
	const beginUpdate = (gameId: string, kind: "update" | "requirement", name: string | null) => {
		setBusyOf(gameId, {
			updateKind: kind,
			updateModName: name,
			busy: "updating",
		});
		setUpdateProgress(null, gameId);
		setUpdateAllProgress(null, gameId);
	};

	/// Downloads the newest provider file, installs it and selects it. A provider the backend routes to the page opens the
	/// files page instead. A download that answers `source.payment_required` opens the page for this update, and the
	/// provider list is read again so the rest of the run uses the page too.
	const updateMod = async (modId: string, name: string, update: UpdateReport) => {
		const ownerGame = id;

		if (!ownerGame) return;

		if (currentBrowserProviders().has(update.provider)) {
			await openUpdatePage(modId, update.provider);
			return;
		}

		beginUpdate(ownerGame, "update", name);
		const progress = newProgressChannel<UpdateProgress>((step) => setUpdateProgress(step, ownerGame));
		await forGameResult(async (gameId) => {
			const previous = currentStatus(gameId)?.mods.find((mod) => mod.id === modId)?.folder;
			let fresh: Status;

			try {
				fresh = await unwrap(commands.updateMod(gameId, modId, update.provider, progress));
			} catch (failure) {
				const choicePath = needsChoicePathOf(failure);

				if (choicePath) {
					installWithChoices(gameId, choicePath);
					return;
				}

				if (!paymentRequiredOf(failure)) throw failure;
				await openUpdatePage(modId, update.provider);
				await refreshProviders();
				return;
			}

			setStatusFor(gameId, fresh);
			const selected = fresh.mods.find((mod) => mod.id === modId)?.folder;
			const text = t("notice.modUpdated", { name });
			const undoAction =
				previous && selected && previous !== selected
					? {
							label: t("notice.undo"),
							onClick: () => undoUpdate(gameId, modId, previous),
						}
					: undefined;
			if (undoAction) notifyDurable(text, undoAction);
			else notify(text);
		});
		setBusyOf(ownerGame, { busy: null, updateModName: null });
		setUpdateProgress(null, ownerGame);
	};

	/// Updates every mod with a saved newer version, one after another. The backend's route puts a provider's mods in the
	/// browser step; the rest of the list still runs through the backend in one pass. The backend keeps going after a
	/// failure, so the window shows the failures together and still takes the fresh status.
	const updateAll = async () => {
		const updateGameId = id;
		const current = updateGameId ? currentStatus(updateGameId) : null;

		if (!(updateGameId && current)) return;
		const targets = updateTargets(current.mods);

		if (targets.length === 0) return;
		const { direct, manual } = splitUpdateTargets(targets, currentBrowserProviders());
		const directIds = direct.map((target) => target.id);

		if (manual.length > 0) openManualUpdates(manual);

		if (directIds.length === 0) return;

		beginUpdate(updateGameId, "update", null);
		setUpdateAllCancel(() => void commands.cancelUpdateAll(updateGameId), updateGameId);
		const progress = newProgressChannel<UpdateAllProgress>((step) => setUpdateAllProgress(step, updateGameId));
		await forGameResult(async (gameId) => {
			const result = await unwrap(commands.updateAll(gameId, directIds, progress));
			setStatusFor(gameId, result.status);

			if (result.updated > 0) {
				const text = t("notice.modsUpdated", { count: result.updated });
				const undoAction =
					result.changes.length > 0
						? {
								label: t("notice.undo"),
								onClick: () => undoUpdateAll(gameId, result.changes),
							}
						: undefined;
				if (undoAction) notifyDurable(text, undoAction);
				else notify(text);
			}

			if (result.cancelled) notify(t("notice.updateAllCancelled"));

			if (result.failed.length > 0) {
				notifyError(translateJoin(result.failed, "\n"));
			}
			// A refused paid download makes the backend route its provider to the page; the list shows that next time.
			await refreshProviders();
		});
		setBusyOf(updateGameId, { busy: null });
		setUpdateAllProgress(null, updateGameId);
		setUpdateAllCancel(null, updateGameId);
	};

	/// The progress channel and the busy state shared by the one-requirement and the whole-closure install.
	const requirementProgress = (gameId: string, name: string | null) => {
		beginUpdate(gameId, "requirement", name);
		return newProgressChannel<UpdateProgress>((step) => setUpdateProgress(step, gameId));
	};

	const finishRequirement = (gameId: string) => {
		setBusyOf(gameId, {
			busy: null,
			updateKind: "update",
			updateModName: null,
		});
		setUpdateProgress(null, gameId);
	};

	/// Installs or enables one requirement of the selected mod. Providers that cannot download directly keep the page
	/// link as the only way, so this is called only when the requirement says the provider can.
	const installRequirement = async (source: SourceRef, name: string) => {
		const ownerGame = id;

		if (!ownerGame) return false;
		const progress = requirementProgress(ownerGame, name);
		const installed = await forGameResult(async (gameId) => {
			const fresh = await unwrap(commands.installRequirement(gameId, source, progress));
			setStatusFor(gameId, fresh);
			notify(t("notice.requirementInstalled", { name }));
		});
		finishRequirement(ownerGame);
		return installed;
	};

	/// Installs the whole requirement closure of one mod. The backend saves the profile list once, so the result is one
	/// undo; blocked steps and failed downloads come back as messages while the rest still runs.
	const installRequirements = async (folder: string) => {
		const ownerGame = id;

		if (!ownerGame) return false;
		const progress = requirementProgress(ownerGame, null);
		const installed = await forGameResult(async (gameId) => {
			const result = await unwrap(commands.installRequirements(gameId, folder, progress));
			setStatusFor(gameId, result.status);

			if (result.installed > 0) notify(t("notice.requirementsInstalled", { count: result.installed }));
			const problems = [...result.blocked, ...result.failed];

			if (problems.length > 0) notifyError(translateJoin(problems, "\n"));
		});
		finishRequirement(ownerGame);
		return installed;
	};

	/// Creates a normal profile from an r2modman/Gale `mods.yml`. The import waits in the profile queue, like any other
	/// profile change, and the backend switches to the new profile.
	const importCommunityProfile = async (path: string, name: string) => {
		const ownerGame = id;
		setBusyOf(ownerGame, { busy: "updating" });
		let imported = false;
		await queued(async (gameId) => {
			const result = await unwrap(commands.importCommunityProfile(gameId, path, name));
			setStatusFor(gameId, result.status);
			notify(t("notice.communityImported", { profile: name, mods: result.mods }));

			if (result.missing.length > 0) {
				notifyError(t("notice.communityMissing", { count: result.missing.length }), result.missing.join(", "));
			}

			if (result.other_versions.length > 0) {
				notifyError(
					t("notice.communityVersions", {
						count: result.other_versions.length,
					}),
					result.other_versions.join(", "),
				);
			}

			imported = true;
		});
		setBusyOf(ownerGame, { busy: null });
		return imported;
	};

	/// Stops every process of the game, also one started outside Gobbo. The command does not change the profile, so it
	/// does not wait for the profile queue: a stuck game must be stoppable at once.
	const stopNow = async () => {
		const gameId = id;

		if (!gameId) return;
		await attempt(async () => {
			const stopped = await unwrap(commands.stopGame(gameId));
			notify(t(stopped ? "notice.gameStopped" : "notice.gameNotRunning"));
		});
	};
	const stopGame = async () => {
		const gameId = id;
		setBusyOf(gameId, { busy: "stopping" });
		await stopNow();
		setBusyOf(gameId, { busy: null });
	};

	const saveSettings = (patch: Partial<SettingsData>) => {
		const gameId = id;

		if (!gameId) return Promise.resolve();
		return settingsQueue.run(async () => {
			const base = currentStatus(gameId)?.settings;

			if (!base) return;
			await attempt(async () => {
				setStatusFor(gameId, await unwrap(commands.saveSettings(gameId, { ...base, ...patch })));
				// The rail caches the managed game roots; a changed game folder must show up there too.
				try {
					setManaged(await unwrap(commands.managedGames()));
				} catch (failure) {
					notifyFailure(failure);
				}
			});
		});
	};

	/// A change to one settings list. The backend merges it into its newest settings under its own lock and answers the
	/// fresh status, so the list shown is the one it saved. It waits in the settings queue like a save.
	const settingsChange = (call: (gameId: string) => Promise<CommandResult<Status>>) => {
		const gameId = id;

		if (!gameId) return Promise.resolve();
		return settingsQueue.run(() =>
			attempt(async () => {
				setStatusFor(gameId, await unwrap(call(gameId)));
			}),
		);
	};
	const setProtonOption = (option: string, on: boolean) => settingsChange((gameId) => commands.setProtonOption(gameId, option, on));
	const addProfileData = (path: string) => settingsChange((gameId) => commands.addProfileData(gameId, path));
	const removeProfileData = (path: string) => settingsChange((gameId) => commands.removeProfileData(gameId, path));

	/// A change to the active profile of the shown game. It joins the profile queue; the answer is the fresh status.
	const profileChange = (call: (gameId: string, profile: string) => Promise<CommandResult<Status>>, notice: string) =>
		queued(async (gameId) => {
			const profile = activeProfileId(gameId);

			if (!profile) return;
			const fresh = await unwrap(call(gameId, profile));

			if (!currentStatus(gameId)) return;
			setStatusFor(gameId, fresh);
			notify(notice);
		});

	/// The user-data choice belongs to the active profile; the fresh status carries the effective redirect/copy mode.
	const setProfileUserData = (set: string, isolated: boolean) =>
		profileChange((gameId, profile) => commands.setProfileUserData(gameId, profile, set, isolated), t("notice.userDataSaved"));

	/// The size of the profile's own data for one set, read on demand. A read does not join the queue.
	const userDataSize = (set: string): Promise<number> => {
		const gameId = id;

		if (!gameId) return Promise.resolve(0);
		const profile = activeProfileId(gameId);

		if (!profile) return Promise.resolve(0);
		return unwrap(commands.userDataSize(gameId, profile, set));
	};

	/// Moves the profile's layer back into the game folder and makes the set shared.
	const moveProfileUserData = (set: string) =>
		profileChange((gameId, profile) => commands.moveProfileUserData(gameId, profile, set), t("notice.userDataMoved"));

	/// A new profile becomes the one shown, so the user can fill it at once. Creating it changes nothing on disk.
	const createProfile = (name: string, copyFrom: string | null, copyData = false) =>
		queued(async (gameId) => {
			const created = await unwrap(commands.createProfile(gameId, name, copyFrom, copyData));
			const fresh = created.profiles.find((profile) => profile.name === name);
			setStatusFor(gameId, fresh ? await unwrap(commands.switchProfile(gameId, fresh.id)) : created);
			resetListSelection();
			notify(t("notice.profileCreated", { name }));
		});

	/// Copies another profile's list and data into the active one. The target's earlier data is kept as a snapshot.
	const copyProfileData = (source: string, target: string) =>
		queued(async (gameId) => {
			const fresh = await unwrap(commands.copyProfileData(gameId, source, target));
			setStatusFor(gameId, fresh);
			resetListSelection();
			notify(t("notice.profileSettingsCopied"));
		});

	const renameProfile = (profileId: string, name: string) => changeQueued((gameId) => commands.renameProfile(gameId, profileId, name));

	const switchProfile = async (profileId: string) => {
		const name = shownStatus()?.profiles.find((profile) => profile.id === profileId)?.name ?? profileId;
		await changeQueued((gameId) => commands.switchProfile(gameId, profileId));
		resetListSelection();

		if (gameRunning) notify(t("notice.profileSwitchedRunning", { name }));
	};

	const deleteProfile = async (profileId: string) => {
		await changeQueued((gameId) => commands.deleteProfile(gameId, profileId), t("notice.profileDeleted"));
		resetListSelection();
	};

	const rescan = () => changeQueued((gameId) => commands.rescanLibrary(gameId), t("notice.libraryRescanned"));

	const deleteMod = async (modId: string) => {
		const name = shownStatus()?.mods.find((mod) => mod.id === modId)?.info.name ?? modId;
		await changeQueued((gameId) => commands.deleteMod(gameId, modId), t("notice.modDeleted", { name }));
		setSelection([]);
	};

	/// Removes every installed part of one source page. The backend takes a family id, so the first part of the page in
	/// the newest status names it; the backend finds the rest of the page from there.
	const deleteModPage = async (pageId: string) => {
		const parts = pagePartsOf(shownStatus()?.mods ?? [], pageId);
		const first = parts[0];

		if (!first) return;
		const name = first.page_name ?? first.info.name;
		await changeQueued((gameId) => commands.deleteModPage(gameId, first.id), t("notice.pageDeleted", { name }));
		setSelection([]);
	};

	/// One installed version. A profile that still selects it moves to `replacement`; the backend undoes a prepared
	/// deployment that holds the version before its files go.
	const deleteModVersion = (modId: string, folder: string, replacement: string | null, version: string) =>
		changeMod(modId, (gameId) => commands.deleteModVersion(gameId, folder, replacement), t("notice.versionDeleted", { version }));

	const deleteUnusedVersions = () => changeQueued((gameId) => commands.deleteUnusedVersions(gameId), t("notice.unusedVersionsDeleted"));

	/// Unpacks the mod's folder again from its archive copy. The notice names the mod. When the active profile has the
	/// folder applied, the reinstall takes that deployment out, so the notice says it needs an apply.
	const reinstallMod = (folder: string, choices?: FomodChoice[] | null) => {
		const status = shownStatus();
		const name = status?.mods.find((mod) => mod.folder === folder)?.info.name ?? folder;
		const applied = status?.deployed === true && status.deployed_list.some((row) => row.folder === folder);
		return changeQueued(
			(gameId) => commands.reinstallMod(gameId, folder, choices ?? null),
			t(applied ? "notice.modReinstalledApply" : "notice.modReinstalled", {
				name,
			}),
		);
	};

	/// The mod's own data: each command answers with the refreshed status, so the UI sees the change at once.
	const setModTags = (modId: string, tags: string[]) => changeQueued((gameId) => commands.setModTags(gameId, modId, tags));
	const setModNote = (modId: string, note: string) => changeQueued((gameId) => commands.setModNote(gameId, modId, note));
	/// The load priority the user sets for a mod. `null` clears it, and the game orders that mod itself.
	const setModPriority = (modId: string, priority: number | null) =>
		changeQueued((gameId) => commands.setModPriority(gameId, modId, priority));
	const setModInstructions = (modId: string, text: string) => changeQueued((gameId) => commands.setModInstructions(gameId, modId, text));
	const setModRelations = (modId: string, relations: Relations) =>
		changeQueued((gameId) => commands.setModRelations(gameId, modId, relations));

	/// The relations of a mod to other mods, each changed on its own. The backend merges the change with its newest
	/// list, so two quick changes keep both; a relation already listed is not added again.
	const addRelation = (modId: string, kind: RelationKind, otherId: string) =>
		changeQueued((gameId) => commands.addRelation(gameId, modId, kind, otherId));
	const removeRelation = (modId: string, kind: RelationKind, otherId: string) =>
		changeQueued((gameId) => commands.removeRelation(gameId, modId, kind, otherId));

	/// The requirements of a mod, each changed on its own. The backend merges the change with its newest list, so two quick
	/// changes keep both; a requirement already listed for the same provider and mod id is not added again.
	const addRequirement = (modId: string, requirement: SourceRequirement) =>
		changeQueued((gameId) => commands.addRequirement(gameId, modId, requirement));
	const removeRequirement = (modId: string, provider: string, requirementModId: string) =>
		changeQueued((gameId) => commands.removeRequirement(gameId, modId, provider, requirementModId));
	const setRequirementIgnored = (modId: string, provider: string, requirementModId: string, ignored: boolean) =>
		changeQueued((gameId) => commands.setRequirementIgnored(gameId, modId, provider, requirementModId, ignored));

	/// The list's history and its order by the rules between mods.
	const undoList = () => changeQueued((gameId) => commands.undoList(gameId));
	const redoList = () => changeQueued((gameId) => commands.redoList(gameId));
	const restoreList = (historyId: number) => changeQueued((gameId) => commands.restoreList(gameId, historyId), t("notice.listRestored"));
	const sortByRules = () => changeQueued((gameId) => commands.sortByRules(gameId), t("notice.sortedByRules"));

	/// The search for the mod that breaks the game. Each test starts the game without a preview: the list changes on purpose.
	const bisectStart = () => changeQueued((gameId) => commands.bisectStart(gameId));
	const bisectAnswer = (broken: boolean) => changeQueued((gameId) => commands.bisectAnswer(gameId, broken));
	const bisectStop = (disableFound: boolean) => changeQueued((gameId) => commands.bisectStop(gameId, disableFound), t("notice.bisectDone"));

	const overwriteToMod = (name: string) =>
		changeQueued((gameId) => commands.overwriteToMod(gameId, name), t("notice.overwriteToMod", { name }));
	const clearOverwrite = () => changeQueued((gameId) => commands.clearOverwrite(gameId), t("notice.overwriteCleared"));
	/// The command only starts the tool; its Working row and its result belong to the backend's task and `tool-exited`.
	const runTool = (toolId: string) =>
		forGame(async (gameId) => {
			await unwrap(commands.runTool(gameId, toolId));
		});
	/// Installs what a tool needs in the game's Proton prefix. The settings ask first.
	const installToolPrerequisites = (toolId: string) =>
		forGame(async (gameId) => {
			const name = toolName(toolId);
			await unwrap(commands.installToolPrerequisites(gameId, toolId));
			notify(t("notice.toolPrerequisitesDone", { name }));
			await reload(gameId);
		});
	/// Saves the active profile's mod groups. The whole array is sent, so the edit joins the profile queue like a list save.
	const saveModGroups = (groups: ModGroup[]) => changeQueued((gameId) => commands.saveModGroups(gameId, groups));

	/// Chooses which mod's copy of a shared file reaches the game. `null` goes back to the list order.
	const setFileWinner = (path: string, familyId: string | null) => changeQueued((gameId) => commands.setFileWinner(gameId, path, familyId));
	/// The same choice for every path a translation shares with its base, sent as one edit.
	const setFileWinners = (paths: string[], family: string) => changeQueued((gameId) => commands.setFileWinners(gameId, paths, family));
	const setModSteps = (folder: string, beforeStart: string[], afterEnd: string[]) =>
		changeQueued((gameId) => commands.setModSteps(gameId, folder, beforeStart, afterEnd), t("notice.stepsSaved"));

	/// A changed source ID or version drops the failure of this session for that mod. The saved check is the library's: it
	/// follows the version the profile selects, and a changed ID clears it in the library.
	const setModSource = (modId: string, provider: string, sourceId: string | null) =>
		changeMod(modId, (gameId) => commands.setModSource(gameId, modId, provider, sourceId), t("notice.sourceIdSaved"));
	const setModInfo = (modId: string, folder: string, info: ModInfoEdit) =>
		changeMod(modId, (gameId) => commands.setModInfo(gameId, modId, folder, info), t("notice.modInfoSaved"));
	/// Hides the provider's current version until it lists another one, or shows it again.
	const ignoreVersion = (modId: string, provider: string) =>
		changeQueued((gameId) => commands.ignoreVersion(gameId, modId, provider), t("notice.versionIgnored"));
	const unignoreVersion = (modId: string, provider: string) =>
		changeQueued((gameId) => commands.unignoreVersion(gameId, modId, provider), t("notice.versionShown"));

	/// Makes the active profile's list the one of a Mod Organizer 2 `modlist.txt` the user picks. Undo brings the old list back.
	const importMo2List = async () => {
		const [path] = await pickFiles("modlist.txt", ["txt"], false);

		if (!path) return;
		await queued(async (gameId) => {
			const report = await unwrap(commands.importMo2List(gameId, path));
			await reload(gameId);
			setSelection([]);
			notify(
				t("notice.mo2Imported", {
					mods: report.mods,
					separators: report.separators,
				}),
			);

			if (report.missing.length > 0) {
				notifyError(t("notice.mo2Missing", { count: report.missing.length }), report.missing.join(", "));
			}
		});
	};

	/// Selects the installed version that the newest provider file matches. The browser update step calls it after its
	/// download imported, so a hand-downloaded update also becomes the profile's version. It answers with its own failure
	/// so the step can show it next to the mod.
	const selectUpdateVersion = async (modId: string, provider: string) => {
		const gameId = id;

		if (!gameId) return;
		await queueProfile(async () => {
			const fresh = await unwrap(commands.selectUpdateVersion(gameId, modId, provider));
			setStatusFor(gameId, fresh);
			forget(modId);
		}, gameId);
	};

	return {
		busy: shown.busy,
		updateKind: shown.updateKind,
		updateModName: shown.updateModName,
		profileQueue,
		queueProfile,
		forGame,
		reload,
		addGame,
		removeGame,
		saveProfile,
		moveMod,
		setModEnabled,
		sortList,
		groupMods,
		ungroupMods,
		setSeparator,
		setModGroup,
		selectVersion,
		runDeploy,
		undo,
		discardChanges,
		runLaunch,
		updateMod,
		updateAll,
		openUpdatePage,
		selectUpdateVersion,
		undoUpdates: undoUpdateAll,
		installRequirement,
		installRequirements,
		stopGame,
		saveSettings,
		setProtonOption,
		addProfileData,
		removeProfileData,
		setProfileUserData,
		userDataSize,
		moveProfileUserData,
		createProfile,
		copyProfileData,
		renameProfile,
		switchProfile,
		deleteProfile,
		rescan,
		deleteMod,
		deleteModPage,
		deleteModVersion,
		deleteUnusedVersions,
		reinstallMod,
		setModTags,
		setModNote,
		setModPriority,
		setModInstructions,
		setModRelations,
		addRelation,
		removeRelation,
		addRequirement,
		removeRequirement,
		setRequirementIgnored,
		undoList,
		redoList,
		restoreList,
		sortByRules,
		bisectStart,
		bisectAnswer,
		bisectStop,
		overwriteToMod,
		clearOverwrite,
		runTool,
		installToolPrerequisites,
		saveModGroups,
		setFileWinner,
		setFileWinners,
		setModSteps,
		setModSource,
		setModInfo,
		ignoreVersion,
		unignoreVersion,
		importMo2List,
		importCommunityProfile,
	};
}
