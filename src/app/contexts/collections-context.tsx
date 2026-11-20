import { createContext, type ReactNode, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";

import { useNoticeActions } from "@/app/contexts/notices-context";
import { useSettingsData } from "@/app/contexts/settings-context";
import { useUiActions, useUiState } from "@/app/contexts/ui-context";
import { useWorkspace, useWorkspaceActions } from "@/app/contexts/workspace-context";
import {
	type CollectionChoice,
	type CollectionInstallState,
	type CollectionManual,
	type CollectionMode,
	installKey,
	type PendingDownload,
	useCollectionInstalls,
} from "@/app/hooks/use-collection-installs";
import {
	type CollectionInstallRequest_Deserialize,
	type CollectionInstallView_Serialize,
	type CollectionOrigin_Serialize,
	type CollectionPreview_Serialize,
	type CollectionProgress,
	type CollectionSessionView_Serialize,
	commands,
} from "@/bindings";
import { useTranslation } from "@/i18n";
import { collectionKey } from "@/lib/collections";
import type { FomodChoice } from "@/lib/fomod";
import { messageOf, unwrap } from "@/lib/ipc";
import { runBusyAction } from "@/lib/use-busy-action";
import { useDeferredOpen } from "@/lib/use-deferred-open";
import { useLatestRef } from "@/lib/use-latest-ref";
import { useTauriListener } from "@/lib/use-tauri-listener";

export type { CollectionMode } from "@/app/hooks/use-collection-installs";
export type CollectionScreen = "link" | "preview" | "install" | "panel";

const EMPTY_PROGRESS: ReadonlyMap<string, CollectionProgress> = new Map();

interface Collections {
	screen: CollectionScreen;
	preview: CollectionPreview_Serialize | null;
	session: CollectionSessionView_Serialize | null;
	installView: CollectionInstallView_Serialize | null;
	progress: ReadonlyMap<string, CollectionProgress>;
	mode: CollectionMode | null;
	busy: boolean;
	failure: string | null;
	waiting: PendingDownload | null;
	choice: CollectionChoice | null;
	manual: CollectionManual | null;
	/// Every install, including finished ones, keyed by game/profile/collection. The Working panel draws their rows.
	installs: ReadonlyMap<string, CollectionInstallState>;
	/// Whether a revision check is running, for the badge menu and the collection panel.
	checking: boolean;
	checkUpdates: (profile: string) => Promise<void>;
	stop: (key: string) => void;
	reopen: (key: string) => void;
	openLink: () => void;
	openPreview: (link: string, revision?: number | null) => Promise<void>;
	openPanel: (profile: string, collection: string) => Promise<void>;
	start: (request: CollectionInstallRequest_Deserialize, mode: CollectionMode) => Promise<void>;
	guide: () => Promise<void>;
	chooseFomod: (choices: FomodChoice[]) => Promise<void>;
	importManual: (path: string) => Promise<void>;
	markDone: () => Promise<void>;
	skip: (permanent: boolean) => Promise<void>;
	skipItem: (item: string, permanent: boolean) => Promise<void>;
	removeCollection: (session: CollectionSessionView_Serialize) => Promise<void>;
	openPage: (profile: string, origin: Pick<CollectionOrigin_Serialize, "provider" | "game_key" | "slug">) => Promise<void>;
	finish: () => Promise<void>;
	resume: (session: CollectionSessionView_Serialize, mode: CollectionMode) => Promise<void>;
}

const CollectionsContext = createContext<Collections | null>(null);

export function useCollections(): Collections {
	const context = useContext(CollectionsContext);

	if (!context) {
		throw new Error("useCollections must be used inside <CollectionsProvider>.");
	}

	return context;
}

/// The collection dialogs' state and navigation. The install engine lives in `useCollectionInstalls` so an install keeps
/// running while this window shows another game or another screen.
export function CollectionsProvider({ children }: { children: ReactNode }) {
	const { t } = useTranslation();
	const { gameId, status } = useWorkspace();
	const { checkCollectionsOnLaunch } = useSettingsData();
	const { reload, selectGame, applyStatus } = useWorkspaceActions();
	const { openDialog } = useUiActions();
	const { dialog } = useUiState();
	const { notifyError } = useNoticeActions();

	const [screen, setScreen] = useState<CollectionScreen>("link");
	const [preview, setPreview] = useState<CollectionPreview_Serialize | null>(null);
	const [panelSession, setPanelSession] = useState<CollectionSessionView_Serialize | null>(null);
	const [focusedKey, setFocusedKey] = useState<string | null>(null);
	const [dialogBusy, setDialogBusy] = useState(false);
	const [dialogFailure, setDialogFailure] = useState<string | null>(null);
	const [checking, setChecking] = useState(false);
	const checkingRef = useRef(false);

	const dialogRef = useLatestRef(dialog);
	const panelRef = useRef(panelSession);
	useEffect(() => {
		panelRef.current = panelSession;
	}, [panelSession]);
	const clearDialogFailure = useCallback(() => setDialogFailure(null), []);

	/// `onOpen` is registered before `reopen` exists, so it calls through this late-bound ref.
	const reopenRef = useRef<(key: string) => void>(() => {
		// Replaced once `reopen` is defined below.
	});
	const onOpen = useCallback((key: string) => reopenRef.current(key), []);
	const {
		installs,
		create,
		run,
		stop,
		fetchSession,
		guide: engineGuide,
		chooseFomod: engineChooseFomod,
		importManual: engineImportManual,
		markDone: engineMarkDone,
		skip: engineSkip,
		finish: engineFinish,
	} = useCollectionInstalls({ onOpen });

	const installsRef = useRef(installs);
	useEffect(() => {
		installsRef.current = installs;
	}, [installs]);

	const { defer: deferOpen, clear: clearDeferredOpen } = useDeferredOpen<string>(
		gameId,
		(key) => {
			const install = installsRef.current.get(key);
			return !!install && install.identity.gameId === gameId;
		},
		() => {
			if (dialogRef.current?.kind !== "collection") openDialog({ kind: "collection" });
		},
	);

	const reopen = useCallback(
		(key: string) => {
			const install = installsRef.current.get(key);

			if (!install) return;
			setFocusedKey(key);
			setScreen("install");
			const game = install.identity.gameId;

			if (game !== gameId) {
				deferOpen(key);
				selectGame(game);
				return;
			}

			clearDeferredOpen();

			if (dialog?.kind !== "collection") openDialog({ kind: "collection" });
		},
		[clearDeferredOpen, deferOpen, dialog, gameId, openDialog, selectGame],
	);
	useEffect(() => {
		reopenRef.current = reopen;
	}, [reopen]);

	const focused = focusedKey ? installs.get(focusedKey) : undefined;
	const session = screen === "panel" ? panelSession : (focused?.session ?? null);
	const progress = focused?.progress ?? EMPTY_PROGRESS;
	const mode = focused?.mode ?? null;
	const busy = dialogBusy || (focused?.busy ?? false);
	const failure = focused?.failure ?? dialogFailure;
	const waiting = focused?.prompts.waiting ?? null;
	const choice = focused?.prompts.choice ?? null;
	const manual = focused?.prompts.manual ?? null;
	const installView = focused?.installView ?? null;

	const fail = useCallback(
		(error: unknown) => {
			const text = messageOf(error);
			setDialogFailure(text);
			notifyError(text);
		},
		[notifyError],
	);

	const openLink = useCallback(() => {
		setScreen("link");
		setPreview(null);
		setPanelSession(null);
		setFocusedKey(null);
		setDialogFailure(null);
		openDialog({ kind: "collection" });
	}, [openDialog]);

	const openPreview = useCallback(
		async (link: string, revision: number | null = null) => {
			if (!gameId) {
				fail(t("collection.noGame"));
				return;
			}

			await runBusyAction(
				async () => {
					const result = await unwrap(commands.inspectCollection(gameId, link.trim(), revision));
					setPreview(result);
					setPanelSession(null);
					setFocusedKey(null);
					setScreen("preview");
					openDialog({ kind: "collection" });
				},
				setDialogBusy,
				fail,
				clearDialogFailure,
			);
		},
		[clearDialogFailure, fail, gameId, openDialog, t],
	);

	const openPanel = useCallback(
		async (profile: string, collection: string) => {
			if (!gameId) return;
			await runBusyAction(
				async () => {
					const result = await unwrap(commands.collectionStatus(gameId, profile, collection));
					setPanelSession(result);
					setPreview(null);
					setFocusedKey(null);
					setScreen("panel");
					openDialog({ kind: "collection" });
				},
				setDialogBusy,
				fail,
				clearDialogFailure,
			);
		},
		[clearDialogFailure, fail, gameId, openDialog],
	);

	/// Reads the current revision of the profile's collections. The command answers with the game's fresh status, so
	/// the badge arrow needs no second status read; the panel keeps its own session and is refreshed when it shows one
	/// of the collections just checked. A failed collection stays silent here and is named in the log.
	const checkUpdates = useCallback(
		async (profile: string) => {
			if (!gameId || checkingRef.current) return;
			checkingRef.current = true;
			await runBusyAction(
				async () => {
					const fresh = await unwrap(commands.checkCollectionUpdates(gameId, profile));
					applyStatus(gameId, fresh);
					const shown = panelRef.current;

					if (shown?.profile === profile) {
						setPanelSession(await unwrap(commands.collectionStatus(gameId, profile, collectionKey(shown.origin))));
					}
				},
				setChecking,
				(error) => notifyError(messageOf(error)),
			);
			checkingRef.current = false;
		},
		[applyStatus, gameId, notifyError],
	);

	/// One check per game per session, for the active profile only. The guard makes status answers while the check runs
	/// harmless: a reload must not start a second round of provider requests.
	const checkedGames = useRef<Set<string>>(new Set());
	useEffect(() => {
		if (!(checkCollectionsOnLaunch && gameId && status)) return;

		if (checkedGames.current.has(gameId)) return;
		const active = status.profiles.find((profile) => profile.active);

		if (!active || active.collections.length === 0) return;
		checkedGames.current.add(gameId);
		void checkUpdates(active.id);
	}, [checkCollectionsOnLaunch, checkUpdates, gameId, status]);

	const start = useCallback(
		async (request: CollectionInstallRequest_Deserialize, chosenMode: CollectionMode) => {
			if (!gameId) {
				fail(t("collection.noGame"));
				return;
			}

			await runBusyAction(
				async () => {
					const view = await unwrap(commands.startCollection(gameId, request));
					const profile = request.profile ?? view.profile;
					const collection = collectionKey(view.origin);
					const key = installKey(gameId, profile, collection);
					create(key, { gameId, profile, collection, name: view.plan.info.name }, chosenMode, view);
					setFocusedKey(key);
					setScreen("install");
					await reload(gameId);
					await run(key, chosenMode, view);
				},
				setDialogBusy,
				fail,
				clearDialogFailure,
			);
		},
		[clearDialogFailure, create, fail, gameId, reload, run, t],
	);

	const resume = useCallback(
		async (current: CollectionSessionView_Serialize, chosenMode: CollectionMode) => {
			const focusedInstall = focusedKey ? installsRef.current.get(focusedKey) : undefined;
			const game = focusedInstall?.identity.gameId ?? gameId;

			if (!game) return;
			setDialogFailure(null);
			const profile = current.profile;
			const collection = collectionKey(current.origin);
			const key = installKey(game, profile, collection);
			create(key, { gameId: game, profile, collection, name: current.plan.info.name }, chosenMode, current, true);
			setFocusedKey(key);
			setScreen("install");
			const fresh = await fetchSession(key);

			if (fresh) await run(key, chosenMode, fresh);
		},
		[create, fetchSession, focusedKey, gameId, run],
	);

	const guide = useCallback(async () => {
		if (focusedKey) await engineGuide(focusedKey);
	}, [engineGuide, focusedKey]);

	const chooseFomod = useCallback(
		async (choices: FomodChoice[]) => {
			if (focusedKey) await engineChooseFomod(focusedKey, choices);
		},
		[engineChooseFomod, focusedKey],
	);

	const importManual = useCallback(
		async (path: string) => {
			if (focusedKey) await engineImportManual(focusedKey, path);
		},
		[engineImportManual, focusedKey],
	);

	const markDone = useCallback(async () => {
		if (focusedKey) await engineMarkDone(focusedKey);
	}, [engineMarkDone, focusedKey]);

	const skip = useCallback(
		async (permanent: boolean) => {
			if (focusedKey) await engineSkip(focusedKey, permanent);
		},
		[engineSkip, focusedKey],
	);

	const finish = useCallback(async () => {
		if (focusedKey) {
			await engineFinish(focusedKey);
			return;
		}

		const current = panelSession;

		if (!(gameId && current)) return;
		await runBusyAction(
			async () => {
				const view = await unwrap(commands.finishCollection(gameId, current.profile, collectionKey(current.origin)));
				setPanelSession(view);
				await reload(gameId);
			},
			setDialogBusy,
			fail,
		);
	}, [engineFinish, fail, focusedKey, gameId, panelSession, reload]);

	const skipItem = useCallback(
		async (item: string, permanent: boolean) => {
			const current = panelSession;

			if (!(gameId && current)) return;
			await runBusyAction(
				async () => {
					const view = await unwrap(commands.skipCollectionItem(gameId, current.profile, collectionKey(current.origin), item, permanent));
					setPanelSession(view);
					await reload(gameId);
				},
				setDialogBusy,
				fail,
				clearDialogFailure,
			);
		},
		[clearDialogFailure, fail, gameId, panelSession, reload],
	);

	const removeCollection = useCallback(
		async (current: CollectionSessionView_Serialize) => {
			if (!gameId) return;
			await runBusyAction(
				async () => {
					await unwrap(commands.removeCollection(gameId, current.profile, collectionKey(current.origin)));
					setPanelSession(null);
					setScreen("panel");
					await reload(gameId);
				},
				setDialogBusy,
				fail,
				clearDialogFailure,
			);
		},
		[clearDialogFailure, fail, gameId, reload],
	);

	const openPage = useCallback(
		async (profile: string, origin: Pick<CollectionOrigin_Serialize, "provider" | "game_key" | "slug">) => {
			if (!gameId) return;

			try {
				await unwrap(commands.openCollectionPage(gameId, profile, collectionKey(origin)));
			} catch (error) {
				fail(error);
			}
		},
		[fail, gameId],
	);

	// A collection link from the site. The pending command covers the window that starts after the click.
	useTauriListener(async (scope) => {
		const pending = await commands.takeCollectionLink();

		if (pending) await openPreview(pending);
		const unlisten = await scope.listen<string>("collection-link", (event) => {
			void openPreview(event.payload);
		});
		scope.add(unlisten);
	}, openPreview);

	const value = useMemo(
		(): Collections => ({
			screen,
			preview,
			session,
			installView,
			progress,
			mode,
			busy,
			failure,
			waiting,
			choice,
			manual,
			installs,
			checking,
			checkUpdates,
			stop,
			reopen,
			openLink,
			openPreview,
			openPanel,
			start,
			guide,
			chooseFomod,
			importManual,
			markDone,
			skip,
			skipItem,
			removeCollection,
			openPage,
			finish,
			resume,
		}),
		[
			screen,
			preview,
			session,
			installView,
			progress,
			mode,
			busy,
			failure,
			waiting,
			choice,
			manual,
			installs,
			checking,
			checkUpdates,
			stop,
			reopen,
			openLink,
			openPreview,
			openPanel,
			start,
			guide,
			chooseFomod,
			importManual,
			markDone,
			skip,
			skipItem,
			removeCollection,
			openPage,
			finish,
			resume,
		],
	);

	return <CollectionsContext.Provider value={value}>{children}</CollectionsContext.Provider>;
}
