import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { useGames } from "@/app/contexts/games-context";
import { useNavigation } from "@/app/contexts/navigation-context";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { useSettingsActions, useSettingsData } from "@/app/contexts/settings-context";
import { useUiActions } from "@/app/contexts/ui-context";
import { useUnsavedChanges } from "@/app/contexts/unsaved-context";
import { usePackActions } from "@/app/hooks/use-pack-actions";
import { useSourceFill } from "@/app/hooks/use-source-fill";
import { useTrayContext } from "@/app/hooks/use-tray";
import { GAME_PREF, isGameId } from "@/app/prefs";
import type { GameRuntime } from "@/app/selectors/game-presentation";
import { useGameActions } from "@/app/use-game-actions";
import { commands, type GameView, type Status, type UpdateReport } from "@/bindings";
import { useTranslation } from "@/i18n";
import type { FomodChoice } from "@/lib/fomod";
import { messageOf, unwrap } from "@/lib/ipc";
import { copyText, openPath } from "@/lib/open";
import { setActiveGame } from "@/lib/progress-store";
import { nameOfId } from "@/lib/records";
import { loadWindow } from "@/lib/tauri";
import { readPref, writePref } from "@/lib/ui-prefs";
import { createStatusCache, useGameState } from "@/lib/use-game-state";
import { useImport } from "@/lib/use-import";
import { useTauriListener } from "@/lib/use-tauri-listener";
import type { KeyCheck } from "@/sources/key-check";
import { anyReady } from "@/sources/ready";
import type { UpdateTarget } from "@/sources/update-targets";
import { useModUpdates } from "@/sources/use-updates";

/// The actions of the workspace that do not change with the status: importing, opening and copying paths and pages,
/// moving between screens. A component that needs only these reads `useWorkspaceActions`, so a status answer does not
/// render it again.
interface WorkspaceActions {
	importPaths: ReturnType<typeof useImport>["importPaths"];
	/// Shows a link's download the backend imported itself.
	acceptImported: ReturnType<typeof useImport>["acceptImported"];
	/// Reads the game's status again and stores it. A feature that changes the library outside the list uses it.
	reload: (gameId: string) => Promise<void>;
	/// Stores a status a `change` command already answered with, so a feature does not read the same game twice.
	applyStatus: (gameId: string, status: Status) => void;
	/// The same read through the profile queue. `syncProfiles` saves the profile step, so a feature that changes the
	/// library must not run it beside a list edit.
	reloadQueued: (gameId: string) => Promise<void>;
	pickArchives: () => Promise<void>;
	/// Opens a shell dialog for a game, switching to that game first when another one is shown.
	openForGame: ReturnType<typeof usePackActions>["openForGame"];
	selectGame: (gameId: string) => void;
	showGames: () => void;
	toMods: () => void;
	/// Opens a mod's page on any provider the game has a key for.
	openSource: (provider: string, modId: string, tab?: "files") => void;
	copyToClipboard: (text: string) => void | Promise<void>;
	copyPath: (path: string) => void | Promise<void>;
	openFolderPath: (path: string) => void | Promise<void>;
	revealFolderPath: (path: string) => void | Promise<void>;
	/// Opens a folder for a specific added game, for the rail and the Games page.
	openPathFor: (gameId: string | null, path: string, reveal?: boolean) => void | Promise<void>;
	validateSourceKey: (provider: string, key: string | null) => Promise<KeyCheck>;
	deploy: () => void;
}

/// Everything that belongs to the game currently shown: its status, selection, update reports, import queue and the
/// actions that change it. A game switch resets this state; app-wide state (settings, downloads, notices) lives in the
/// providers around it.
interface Workspace extends WorkspaceActions {
	packExport: ReturnType<typeof usePackActions>["exportPack"];
	pickPack: ReturnType<typeof usePackActions>["pickPack"];
	pickPackLink: ReturnType<typeof usePackActions>["pickPackLink"];
	importPack: ReturnType<typeof usePackActions>["importPack"];
	downloadPack: ReturnType<typeof usePackActions>["downloadPack"];
	game: GameView | undefined;
	gameId: string | undefined;
	gameName: string;
	/// Whether a provider of this game can be asked now (no key needed, or its key is saved): update checks and page reads.
	updatesReady: boolean;
	status: Status | null;
	/// The failure that left the status empty, and the way to try the same game again.
	statusError: unknown;
	retryStatus: () => void;
	selection: string[];
	/// The selection filtered to mods that still exist in the status.
	selected: string[];
	setSelection: (mods: string[]) => void;
	updates: Record<string, UpdateReport>;
	gameRunning: boolean;
	/// The long action of this game that is running now, or null. It comes from the actions, so the header and the palette
	/// show the same busy state.
	busy: GameRuntime["busy"];
	/// `requirement` is an install of a missing requirement; the activity title uses it instead of "Updating".
	updateKind: "update" | "requirement";
	updateModName: string | null;
	pendingVersions: ReturnType<typeof useImport>["pendingVersions"];
	dismissPendingVersion: ReturnType<typeof useImport>["dismissPendingVersion"];
	dismissPendingFamily: ReturnType<typeof useImport>["dismissPendingFamily"];
	fomodWizard: ReturnType<typeof useImport>["fomodWizard"];
	confirmFomod: (choices: FomodChoice[]) => void;
	skipFomod: () => void;
	setProgress: ReturnType<typeof useImport>["setProgress"];
	launch: (executable: string | null, applyDraft?: boolean) => void | Promise<void>;
	gameActions: ReturnType<typeof useGameActions>;
	modUpdates: ReturnType<typeof useModUpdates>;
}

const WorkspaceContext = createContext<Workspace | null>(null);
const WorkspaceActionsContext = createContext<WorkspaceActions | null>(null);

/// The stable actions of the workspace, for a component that does not draw the status.
export function useWorkspaceActions(): WorkspaceActions {
	const context = useContext(WorkspaceActionsContext);

	if (!context) {
		throw new Error("useWorkspaceActions must be used inside <WorkspaceProvider>.");
	}

	return context;
}

export function useWorkspace(): Workspace {
	const context = useContext(WorkspaceContext);

	if (!context) {
		throw new Error("useWorkspace must be used inside <WorkspaceProvider>.");
	}

	return context;
}

/// The active game. This is the one provider that changes on every status answer, which is exactly what the mod list,
/// header and inspector need; the provider tree keeps the download queue and the notice list out of that path.
export function WorkspaceProvider({ children }: { children: ReactNode }) {
	const { t } = useTranslation();
	const { games, managedGames, setManaged } = useGames();
	const { navigate, openGames, openMods } = useNavigation();
	const { requestLeave } = useUnsavedChanges();
	const { readyProviders } = useSettingsData();
	const { rememberSourceAccount, refreshProviders, currentBrowserProviders } = useSettingsActions();
	const { openDialog } = useUiActions();
	const { notify, notifyDurable, notifyError, notifyFailure, attempt } = useNoticeActions();

	const [pickedGameId, setGameId] = useState<string | null>(() => readPref<string | null>(GAME_PREF, null, isGameId));
	const game = managedGames.find((item) => item.id === pickedGameId) ?? managedGames[0];
	const id = game?.id;
	const updatesReady = anyReady(
		readyProviders,
		(game?.providers ?? []).map((provider) => provider.id),
	);

	/// Written for the next start. Nothing is written before the games load, so an early render keeps the saved choice.
	useEffect(() => {
		if (id) writePref(GAME_PREF, id);
	}, [id]);

	/// A `gobbo://pack` link reaches the window as an event; one that arrived before the window is kept by the backend.
	/// The pack preview needs a game, so a link without one is reported instead of being lost.
	const openPackLink = useCallback(
		(link: string) => {
			if (id) openDialog({ kind: "pack-link", link });
			else notifyError(t("pack.link.needsGame"));
		},
		[id, notifyError, openDialog, t],
	);
	useTauriListener(async (scope) => {
		const unlisten = await scope.listen<string>("pack-link", (event) => openPackLink(event.payload));
		scope.add(unlisten);
	}, openPackLink);
	useEffect(() => {
		void (async () => {
			const link = await commands.takePackLink();

			if (link) openPackLink(link);
		})();
	}, [openPackLink]);

	/// The window title says which game is active. It is cosmetic; a failure has nowhere to go.
	useEffect(() => {
		const name = game?.name;
		void (async () => {
			try {
				const { getCurrentWindow } = await loadWindow();
				await getCurrentWindow().setTitle(id ? t("app.windowTitleGame", { game: name ?? id }) : t("app.name"));
			} catch {
				// a browser preview or a closing window has no title to set
			}
		})();
	}, [game?.name, id, t]);

	const [selection, setSelection] = useState<string[]>([]);
	const [prevId, setPrevId] = useState(id);

	if (id !== prevId) {
		setPrevId(id);
		setSelection([]);
	}

	/// Statuses are cached by game, not only for the one shown. A long command of a background game still gets its newest
	/// list from here, so its queued edits do not silently early-return while another game is on screen.
	const statusCache = useRef(createStatusCache());
	const statusOf = useCallback((gameId: string) => statusCache.current.get(gameId), []);
	const writeStatus = useCallback((gameId: string, next: Status) => {
		statusCache.current.merge(gameId, next);
	}, []);
	const { status, setStatus, gameRunning, statusError, retryStatus } = useGameState(id ?? null, true, notifyError, statusOf, writeStatus);

	/// The game shown now, for answers that come back after the user switched games. Every visible status write goes
	/// through this helper, so a slow command cannot put another game's mods under the current heading. The progress
	/// store switches its visible slot here too; each long action still writes to its own game by captured id.
	const shownGame = useRef(id);
	useEffect(() => {
		shownGame.current = id;
		setActiveGame(id ?? undefined);
	}, [id]);
	/// The newest status of the shown game, updated synchronously by `setStatusFor`. A queued list edit reads it before
	/// React has committed the render, so a version change followed by a toggle cannot send the old folder.
	const latestStatus = useRef<{ game: string; status: Status } | null>(null);
	useEffect(() => {
		latestStatus.current = id && status ? { game: id, status } : null;
	}, [id, status]);
	const setStatusFor = useCallback(
		(gameId: string, next: Status) => {
			const merged = statusCache.current.merge(gameId, next);

			if (!merged) return false;
			// Any game's status lands in the cache; only the shown game's row is drawn.
			if (gameId === shownGame.current) {
				latestStatus.current = { game: gameId, status: merged };
				setStatus(merged);
			}

			return true;
		},
		[setStatus],
	);

	const modUpdates = useModUpdates({
		gameId: id ?? null,
		status,
		onStatus: setStatusFor,
		nameOf: (target) => nameOfId(games, target),
		notify,
		notifyError,
	});
	/// The mod's page on any provider whose key the game has.
	const openSource = useCallback(
		(provider: string, modId: string, tab?: "files") => {
			const key = game?.providers.find((item) => item.id === provider)?.key;

			if (!key) return;
			void attempt(async () => {
				await commands.openSourcePage(provider, key, modId, tab ?? null);
			});
		},
		[attempt, game],
	);
	const openManualUpdates = (targets: UpdateTarget[]) => openDialog({ kind: "manual-updates", targets });
	/// The import flow is set up below the game actions; an update that needs the FOMOD wizard reaches it through this.
	const importLater = useRef<ReturnType<typeof useImport>["importPaths"] | null>(null);
	const gameActions = useGameActions({
		gameId: id,
		gameRunning,
		setStatusFor,
		latestStatus,
		statusOf,
		setManaged,
		setGameId,
		setSelection,
		nameOf: (target) => nameOfId(games, target),
		onNoGames: () => navigate("games"),
		onGameAdded: () => {
			navigate("mods");
			// The first game starts with its setup check. Later games keep the list as their landing screen.
			if (managedGames.length === 0) openDialog({ kind: "setup" });
		},
		forget: modUpdates.forget,
		forgetAll: modUpdates.forgetAll,
		attempt,
		notify,
		notifyDurable,
		notifyError,
		openManualUpdates,
		currentBrowserProviders,
		refreshProviders,
		notifyFailure,
		openElevation: () => openDialog({ kind: "elevation" }),
		installWithChoices: (gameId, path) => void importLater.current?.([path], { game: gameId, source: null }),
	});
	useTrayContext(id, gameActions.busy);
	const { profileQueue, queueProfile, forGame, reload, runLaunch } = gameActions;
	const reloadQueued = useCallback((gameId: string) => queueProfile(() => reload(gameId), gameId), [queueProfile, reload]);

	/// A profile switched from the tray changes the backend's active list; the window reads the fresh status so its header
	/// and mod list do not keep showing the old profile.
	const onTrayProfile = useCallback(
		(payload: string) => {
			if (!id || payload !== id) return;
			void reloadQueued(id);
		},
		[id, reloadQueued],
	);
	useTauriListener(async (scope) => {
		if (!id) return;
		const unlisten = await scope.listen<string>("tray-profile-switched", (event) => onTrayProfile(event.payload));
		scope.add(unlisten);
	}, onTrayProfile);

	const fillSourceInfo = useSourceFill({
		enabled: updatesReady,
		activeGameId: id ?? null,
		setStatusFor,
		queueProfile,
	});

	const {
		setProgress,
		importPaths,
		acceptImported,
		pickArchives,
		fomodWizard,
		confirmFomod,
		skipFomod,
		pendingVersions,
		dismissPendingVersion,
		dismissPendingFamily,
	} = useImport({
		gameId: id ?? null,
		beforeImport: (importGame) => profileQueue.idle(importGame),
		onStatus: (gameId, fresh) => {
			setStatusFor(gameId, fresh);
			void fillSourceInfo(gameId);
		},
		onShowMods: () => navigate("mods"),
		onArchiveReport: (gameId, folder, name) => {
			if (gameId !== shownGame.current) return;
			openDialog({ kind: "archive-report", folder, name });
		},
		onSourceImport: (gameId, ids) => {
			if (gameId !== shownGame.current) return;

			for (const modId of ids) void modUpdates.checkModFromMenu(modId);
		},
		onError: notifyError,
		notify,
	});
	useEffect(() => {
		importLater.current = importPaths;
	}, [importPaths]);

	const selectGame = useCallback(
		(gameId: string) => {
			// The switch itself is what must wait for the unsaved-changes question; navigating first would show the new
			// game's list under the old form for a frame.
			requestLeave(() => {
				setGameId(gameId);
				navigate("mods");
			});
		},
		[navigate, requestLeave],
	);

	/// A game picked in the tray becomes the window's game too, so the menu and the header follow the same choice.
	useTauriListener(async (scope) => {
		const unlisten = await scope.listen<string>("tray-game-switched", (event) => selectGame(event.payload));
		scope.add(unlisten);
	}, selectGame);

	const { exportPack, pickPack, pickPackLink, importPack, downloadPack, openForGame } = usePackActions({
		forGame,
		reload,
		fillSourceInfo,
		setProgress,
		notify,
		attempt,
		selectGame,
		activeGameId: id,
	});

	const gameName = game?.name ?? "";
	const selected = useMemo(() => {
		if (!status) return [];
		const wanted = new Set(selection);
		return status.mods.filter((mod) => wanted.has(mod.id)).map((mod) => mod.id);
	}, [selection, status]);

	/// Copies text to the clipboard and says so; a failure goes to the notices instead.
	const copy = useCallback(
		(text: string, notice: string) =>
			attempt(async () => {
				if (await copyText(text, notifyError)) notify(notice);
			}),
		[attempt, notify, notifyError],
	);
	const copyToClipboard = useCallback(
		(text: string) => {
			void copy(text, t("notice.copied"));
		},
		[copy, t],
	);
	const copyPath = useCallback(
		(path: string) => {
			void copy(path, t("notice.pathCopied"));
		},
		[copy, t],
	);

	/// Opening folders in the file manager, and showing a file in its folder. The game id must be the one that owns the
	/// path: the backend only permits the data folder and that game's configured roots.
	const openPathFor = useCallback(
		(gameId: string | null, path: string, reveal = false) => openPath(path, gameId, reveal, notifyError),
		[notifyError],
	);
	const openFolderPath = useCallback((path: string) => openPathFor(id ?? null, path, false), [id, openPathFor]);
	const revealFolderPath = useCallback((path: string) => openPathFor(id ?? null, path, true), [id, openPathFor]);

	/// Checks a typed key at the source, or the saved one when no new key was typed.
	const validateSourceKey = useCallback(
		async (provider: string, key: string | null): Promise<KeyCheck> => {
			try {
				const account = await unwrap(commands.validateSourceKey(provider, key));
				// A typed key is only a check before saving. The saved key decides the update behavior, so its result alone
				// becomes the account this session uses.
				if (key === null) rememberSourceAccount(provider, account);
				return {
					ok: true,
					text: t(account.premium ? "providers.check.okPremium" : "providers.check.ok", { name: account.name }),
				};
			} catch (failure) {
				return { ok: false, text: messageOf(failure) };
			}
		},
		[rememberSourceAccount, t],
	);

	/// A deployment shows its preview first. `launch` starts the game after it, with `executable` (the default when null).
	const deploy = useCallback(() => {
		openDialog({
			kind: "deploy",
			launch: false,
			executable: null,
			request: Date.now(),
		});
	}, [openDialog]);
	const launch = useCallback(
		async (executable: string | null, applyDraft = false) => {
			/// Only an explicit "apply" opens the preview. A plain play starts the game as it is, which is how a game with
			/// no enabled mods can start without a deployment.
			const needsPreview = applyDraft && status !== null && (!status.deployed || status.draft_changes > 0);

			if (needsPreview) {
				openDialog({
					kind: "deploy",
					launch: true,
					executable,
					request: Date.now(),
				});
				return;
			}

			await runLaunch(executable, applyDraft);
		},
		[openDialog, runLaunch, status],
	);

	const actions = useMemo(
		(): WorkspaceActions => ({
			importPaths,
			acceptImported,
			reload,
			applyStatus: setStatusFor,
			reloadQueued,
			pickArchives,
			openForGame,
			selectGame,
			showGames: openGames,
			toMods: openMods,
			openSource,
			copyToClipboard,
			copyPath,
			openFolderPath,
			revealFolderPath,
			openPathFor,
			validateSourceKey,
			deploy,
		}),
		[
			copyPath,
			copyToClipboard,
			deploy,
			setStatusFor,
			importPaths,
			acceptImported,
			openFolderPath,
			openPathFor,
			openForGame,
			openSource,
			pickArchives,
			revealFolderPath,
			reload,
			reloadQueued,
			openGames,
			openMods,
			selectGame,
			validateSourceKey,
		],
	);

	const value = useMemo(
		(): Workspace => ({
			...actions,
			game,
			gameId: id,
			gameName,
			updatesReady,
			status,
			statusError,
			retryStatus,
			selection,
			selected,
			setSelection,
			updates: modUpdates.updates,
			gameRunning,
			busy: gameActions.busy,
			updateKind: gameActions.updateKind,
			updateModName: gameActions.updateModName,
			pendingVersions,
			dismissPendingVersion,
			dismissPendingFamily,
			fomodWizard,
			confirmFomod,
			skipFomod,
			setProgress,
			packExport: exportPack,
			pickPack,
			pickPackLink,
			importPack,
			downloadPack,
			launch,
			gameActions,
			modUpdates,
		}),
		[
			actions,
			confirmFomod,
			downloadPack,
			exportPack,
			pickPackLink,
			dismissPendingVersion,
			dismissPendingFamily,
			fomodWizard,
			game,
			gameActions,
			gameName,
			gameRunning,
			id,
			importPack,
			launch,
			modUpdates,
			pendingVersions,
			pickPack,
			retryStatus,
			selected,
			selection,
			setProgress,
			skipFomod,
			status,
			statusError,
			updatesReady,
		],
	);

	return (
		<WorkspaceActionsContext.Provider value={actions}>
			<WorkspaceContext.Provider value={value}>{children}</WorkspaceContext.Provider>
		</WorkspaceActionsContext.Provider>
	);
}
