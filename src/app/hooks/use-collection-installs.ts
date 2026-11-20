import { Channel } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";

import { useDownloadsContext } from "@/app/contexts/downloads-context";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { useWorkspace, useWorkspaceActions } from "@/app/contexts/workspace-context";
import {
	type CollectionDecision_Deserialize,
	type CollectionInstallView_Serialize,
	type CollectionProgress,
	type CollectionRoundView_Serialize,
	type CollectionSessionView_Serialize,
	commands,
	type FomodStepView,
} from "@/bindings";
import { useTranslation } from "@/i18n";
import type { FomodChoice } from "@/lib/fomod";
import { type CommandResult, messageOf, needsChoiceOf, translate, unwrap } from "@/lib/ipc";
import { type RegisteredTask, registerTask, removeTask, updateTask } from "@/lib/tasks-store";
import { useTauriListener } from "@/lib/use-tauri-listener";

export type CollectionMode = "automatic" | "guided";

export interface PendingDownload {
	owner: string;
	item: string;
	url: string;
}

export interface CollectionChoice {
	key: string;
	name: string;
	game: string;
	path: string;
	module_name: string;
	steps: FomodStepView[];
}

export interface CollectionManual {
	item: string;
	url: string | null;
	instructions: string;
}

/// The identity never changes for the lifetime of an install. Every command, status read and reload uses its game id,
/// so an install keeps running after the active game changes.
export interface InstallIdentity {
	gameId: string;
	profile: string;
	collection: string;
	name: string;
}

interface Prompts {
	waiting: PendingDownload | null;
	choice: CollectionChoice | null;
	manual: CollectionManual | null;
}

export interface CollectionInstallState {
	identity: InstallIdentity;
	mode: CollectionMode;
	session: CollectionSessionView_Serialize | null;
	installView: CollectionInstallView_Serialize | null;
	progress: ReadonlyMap<string, CollectionProgress>;
	busy: boolean;
	failure: string | null;
	prompts: Prompts;
	deferred: ReadonlySet<string>;
	importing: boolean;
	stopped: boolean;
}

/// The engine is a view over the backend controller. It keeps no round, prompt queue or guided loop: every step calls
/// one backend command that drives a round and returns the fresh session plus the single prompt to render.
export interface CollectionInstallsApi {
	installs: ReadonlyMap<string, CollectionInstallState>;
	getInstall: (key: string) => CollectionInstallState | undefined;
	fetchSession: (key: string) => Promise<CollectionSessionView_Serialize | null>;
	create: (
		key: string,
		identity: InstallIdentity,
		mode: CollectionMode,
		session: CollectionSessionView_Serialize,
		preserve?: boolean,
	) => void;
	stop: (key: string) => void;
	run: (key: string, mode: CollectionMode, session: CollectionSessionView_Serialize) => Promise<void>;
	guide: (key: string) => Promise<void>;
	chooseFomod: (key: string, choices: FomodChoice[]) => Promise<void>;
	importManual: (key: string, path: string) => Promise<void>;
	markDone: (key: string) => Promise<void>;
	skip: (key: string, permanent: boolean) => Promise<void>;
	finish: (key: string) => Promise<void>;
}

export function installKey(gameId: string, profile: string, collection: string): string {
	return `${gameId}|${profile}|${collection}`;
}

/// Task ids are namespaced so a collection row never replaces a task from another feature with the same key.
function taskId(key: string): string {
	return `collection:${key}`;
}

export interface UseCollectionInstallsOptions {
	/// Opens the dialog for an install from its Working-panel row. The context owns focus and navigation.
	onOpen: (key: string) => void;
}

/// All long-running collection work, keyed by game/profile/collection. It deliberately never reads the active game:
/// every command uses the install's own identity, so a game switch cannot retarget a running round.
export function useCollectionInstalls({ onOpen }: UseCollectionInstallsOptions): CollectionInstallsApi {
	const { t } = useTranslation();
	const { status } = useWorkspace();
	const { reload } = useWorkspaceActions();
	const { downloads } = useDownloadsContext();
	const { notify, notifyError } = useNoticeActions();

	const [installs, setInstalls] = useState<ReadonlyMap<string, CollectionInstallState>>(new Map());
	const installsRef = useRef(installs);
	const downloadsRef = useRef(downloads);
	const registered = useRef<Set<string>>(new Set());
	/// `syncTask` registers the Cancel action before `stop` exists, so it calls through this late-bound ref.
	const stopRef = useRef<(key: string) => void>(() => {
		// Replaced once `stop` is defined below.
	});

	useEffect(() => {
		downloadsRef.current = downloads;
	}, [downloads]);

	/// The single low-level write: a synchronous ref mirror plus React state, so a channel callback never reads a stale
	/// install from a render closure. This is view state only; the round and its prompts live in the backend.
	const write = useCallback((key: string, next: CollectionInstallState) => {
		const map = new Map(installsRef.current);
		map.set(key, next);
		installsRef.current = map;
		setInstalls(map);
	}, []);

	const read = useCallback((key: string) => installsRef.current.get(key), []);

	const syncTask = useCallback(
		(key: string, install: CollectionInstallState) => {
			if (install.stopped || install.session?.counts.pending === 0) {
				if (registered.current.has(key)) {
					removeTask(taskId(key));
					registered.current.delete(key);
				}

				return;
			}

			const counts = install.session?.counts;
			const done = counts ? counts.installed + counts.manual : 0;
			const total = counts ? counts.total - counts.skipped - counts.unsupported : 0;
			const pending = counts ? counts.pending : 0;
			const percent = total > 0 ? Math.min(100, Math.round((done * 100) / total)) : 100;
			const hasPrompt = install.prompts.choice !== null || install.prompts.manual !== null || install.prompts.waiting !== null;
			// A hand-picked file the download will supply is a passive wait, not a choice the user has to make.
			const waitingOnly = install.prompts.waiting !== null && install.prompts.choice === null && install.prompts.manual === null;
			let title = t("tasks.collectionInstalling", { name: install.identity.name });

			if (hasPrompt) title = t("tasks.collectionWaiting");

			if (waitingOnly) title = t("tasks.waitingForYou");
			const task: RegisteredTask = {
				id: taskId(key),
				game: install.identity.gameId,
				kind: "collection",
				title,
				detail: t("collection.install.progress", { done, total, pending }),
				percent,
				state: hasPrompt ? "waiting" : "running",
				cancel: { label: t("tasks.collectionStop"), run: () => stopRef.current(key) },
				open: { label: t("tasks.collectionOpen"), run: () => onOpen(key) },
				source: "local",
				dedupe: {
					Collection: {
						game: install.identity.gameId,
						profile: install.identity.profile,
						collection: install.identity.collection,
					},
				},
			};

			if (registered.current.has(key)) updateTask(taskId(key), task);
			else {
				registerTask(task);
				registered.current.add(key);
			}
		},
		[onOpen, t],
	);

	/// Every state change flows through patch. Completion removes the task and marks the install stopped.
	const patchImpl = useCallback(
		(key: string, fn: (current: CollectionInstallState) => CollectionInstallState) => {
			const current = installsRef.current.get(key);

			if (!current) return;
			let next = fn(current);

			if (!next.stopped && next.session?.counts.pending === 0) {
				next = { ...next, stopped: true, busy: false };
			}

			write(key, next);
			syncTask(key, next);
		},
		[syncTask, write],
	);
	const patchRef = useRef(patchImpl);
	useEffect(() => {
		patchRef.current = patchImpl;
	}, [patchImpl]);
	const patch = useCallback(
		(key: string, fn: (current: CollectionInstallState) => CollectionInstallState) => patchRef.current(key, fn),
		[],
	);

	// Text can change with the locale without an install state change; refresh the registered rows then.
	useEffect(() => {
		for (const [key, install] of installsRef.current) syncTask(key, install);
	}, [syncTask]);

	const fail = useCallback(
		(key: string, error: unknown) => {
			const text = messageOf(error);
			patch(key, (install) => ({ ...install, failure: text, busy: false }));
			notifyError(text);
		},
		[notifyError, patch],
	);

	const stop = useCallback(
		(key: string) => {
			const install = installsRef.current.get(key);

			if (!install || install.stopped) return;
			const { identity, prompts } = install;
			patch(key, (current) => ({ ...current, stopped: true, busy: false }));
			void commands.collectionStop(identity.gameId, identity.profile, identity.collection);
			const owner = prompts.waiting?.owner;

			if (owner !== undefined) void commands.forgetCollectionDownload(owner);
		},
		[patch],
	);
	useEffect(() => {
		stopRef.current = stop;
	}, [stop]);

	const fetchSession = useCallback(async (key: string): Promise<CollectionSessionView_Serialize | null> => {
		const install = installsRef.current.get(key);

		if (!install?.session) return install?.session ?? null;

		try {
			return await unwrap(commands.collectionStatus(install.identity.gameId, install.session.profile, install.identity.collection));
		} catch {
			return install.session;
		}
	}, []);

	const create = useCallback(
		(key: string, identity: InstallIdentity, mode: CollectionMode, session: CollectionSessionView_Serialize, preserve = false) => {
			const previous = preserve ? installsRef.current.get(key) : undefined;
			const next: CollectionInstallState = {
				identity,
				mode,
				session,
				installView: null,
				progress: new Map(),
				busy: false,
				failure: null,
				prompts: { waiting: null, choice: null, manual: null },
				deferred: previous?.deferred ?? new Set(),
				importing: false,
				stopped: false,
			};
			write(key, next);
			syncTask(key, next);
		},
		[syncTask, write],
	);

	/// Translates one backend round into view state. The backend chose the member and the prompt; the window only draws
	/// it. "Already in library" stays a notice from the round's warnings, never a loop in the window.
	const applyRound = useCallback(
		(key: string, round: CollectionRoundView_Serialize) => {
			const game = installsRef.current.get(key)?.identity.gameId ?? "";
			const choice = round.install.needs_choice[0];
			const manualKey = round.install.needs_user[0];
			const manualItem = manualKey ? round.install.session.plan.items.find((item) => item.key === manualKey) : undefined;

			for (const warning of round.install.warnings) notify(translate(warning));
			patch(key, (state) => ({
				...state,
				session: round.install.session,
				installView: round.install,
				busy: false,
				prompts: {
					waiting: round.waiting
						? {
								owner: `collection:${round.waiting.profile}:${round.waiting.item}`,
								item: round.waiting.item,
								url: round.waiting.url,
							}
						: null,
					choice: choice
						? {
								key: choice.key,
								name: choice.name,
								game,
								path: choice.path,
								module_name: choice.module_name,
								steps: choice.steps,
							}
						: null,
					manual: manualKey
						? {
								item: manualKey,
								url: manualItem?.source.kind === "browse" ? manualItem.source.url : null,
								instructions: manualItem?.instructions ?? "",
							}
						: null,
				},
			}));
		},
		[notify, patch],
	);

	/// The one path every command takes: open a progress channel, run the backend step and render its round. There is
	/// no round counter, guided loop or recursion here; the backend repeats rounds until it needs a decision.
	const drive = useCallback(
		async (key: string, invoke: (channel: Channel<CollectionProgress>) => Promise<CommandResult<CollectionRoundView_Serialize>>) => {
			const install = installsRef.current.get(key);

			if (!install || install.stopped) return;
			const game = install.identity.gameId;
			patch(key, (state) => ({ ...state, busy: true, failure: null }));
			const channel = new Channel<CollectionProgress>();
			// `onmessage` is the Channel class callback from the Tauri API, not a DOM event handler.
			// oxlint-disable-next-line unicorn/prefer-add-event-listener
			channel.onmessage = (step) => {
				const live = installsRef.current.get(key);

				if (!live) return;
				const next = new Map(live.progress);
				next.set(step.key, step);
				write(key, { ...live, progress: next });
			};

			try {
				const round = await unwrap(invoke(channel));
				applyRound(key, round);
				await reload(game);
			} catch (error) {
				fail(key, error);
			}

			patch(key, (state) => ({ ...state, busy: false }));
		},
		[applyRound, fail, patch, reload, write],
	);

	const resolve = useCallback(
		async (key: string, item: string, decision: CollectionDecision_Deserialize) => {
			const install = installsRef.current.get(key);

			if (!install) return;
			await drive(key, (channel) =>
				commands.collectionResolvePrompt(
					install.identity.gameId,
					install.identity.profile,
					install.identity.collection,
					item,
					decision,
					channel,
				),
			);
		},
		[drive],
	);

	const run = useCallback(
		async (key: string, _mode: CollectionMode, session: CollectionSessionView_Serialize) => {
			const install = installsRef.current.get(key);

			if (!install || install.stopped) return;

			if (session.counts.pending === 0) {
				patch(key, (state) => ({ ...state, session, busy: false }));
				return;
			}

			await drive(key, (channel) =>
				commands.collectionRunRound(install.identity.gameId, install.identity.profile, install.identity.collection, channel),
			);
		},
		[drive, patch],
	);

	const guide = useCallback(
		async (key: string) => {
			const fresh = await fetchSession(key);

			if (fresh) await run(key, "guided", fresh);
		},
		[fetchSession, run],
	);

	const chooseFomod = useCallback(
		async (key: string, choices: FomodChoice[]) => {
			const choice = installsRef.current.get(key)?.prompts.choice;

			if (!choice) return;
			await resolve(key, choice.key, { kind: "choice", path: choice.path, choices });
		},
		[resolve],
	);

	const importManual = useCallback(
		async (key: string, path: string) => {
			const install = installsRef.current.get(key);
			const manual = install?.prompts.manual;

			if (!(install && manual)) return;

			try {
				await resolve(key, manual.item, { kind: "file", path });
			} catch (error) {
				if (!needsChoiceOf(error)) {
					fail(key, error);
					return;
				}
				// A hand-picked FOMOD comes back as a fresh choice prompt from the backend. Inspect it once so the shared
				// wizard can render, without restarting the round here.
				try {
					const inspect = await unwrap(commands.inspectZip(install.identity.gameId, path));

					if (inspect.kind !== "fomod") {
						fail(key, error);
						return;
					}

					patch(key, (state) => ({
						...state,
						prompts: {
							...state.prompts,
							manual: null,
							choice: {
								key: manual.item,
								name: install.session?.plan.items.find((item) => item.key === manual.item)?.name ?? manual.item,
								game: install.identity.gameId,
								path,
								module_name: inspect.module_name,
								steps: inspect.steps,
							},
						},
					}));
				} catch (inspectError) {
					fail(key, inspectError);
				}
			}
		},
		[fail, patch, resolve],
	);

	const markDone = useCallback(
		async (key: string) => {
			const manual = installsRef.current.get(key)?.prompts.manual;

			if (!manual) return;
			await resolve(key, manual.item, { kind: "mark_done" });
		},
		[resolve],
	);

	const skip = useCallback(
		async (key: string, permanent: boolean) => {
			const install = installsRef.current.get(key);

			if (!install) return;
			const item = install.prompts.waiting?.item ?? install.prompts.choice?.key ?? install.prompts.manual?.item ?? install.session?.next;

			if (!item) return;
			const owner = install.prompts.waiting?.owner;

			if (owner !== undefined) {
				void commands.forgetCollectionDownload(owner);

				for (const download of downloadsRef.current) {
					if (download.owner === owner) void commands.removeDownload(download.id);
				}
			}

			await resolve(key, item, { kind: "skip", permanent });
		},
		[resolve],
	);

	const finish = useCallback(
		async (key: string) => {
			const install = installsRef.current.get(key);

			if (!install?.session) return;
			const game = install.identity.gameId;
			patch(key, (state) => ({ ...state, busy: true }));

			try {
				const view = await unwrap(commands.finishCollection(game, install.session.profile, install.identity.collection));
				patch(key, (state) => ({ ...state, session: view }));
				await reload(game);
			} catch (error) {
				fail(key, error);
			}

			patch(key, (state) => ({ ...state, busy: false }));
		},
		[fail, patch, reload],
	);

	// A new status answer means the library or a profile changed. Re-read every running session with its own game id.
	useEffect(() => {
		if (!status) return;

		for (const [key, install] of installsRef.current) {
			if (install.stopped || !install.session) continue;
			void (async () => {
				const current = installsRef.current.get(key);

				if (!current?.session) return;
				const fresh = await fetchSession(key);

				if (fresh && installsRef.current.get(key)?.session === current.session) {
					patch(key, (state) => ({ ...state, session: fresh }));
				}
			})();
		}
	}, [fetchSession, patch, status]);

	// An automatic round asks the window to open a provider page for a member whose file the user must pick by hand.
	// The controller already registered the owned download, so opening the page is all the window does here; the round
	// keeps waiting in the backend and no loop is added to React.
	// A browser preview has no events; the backend still owns whatever round the real window started.
	useTauriListener(async (scope) => {
		const off = await scope.listen<{
			game: string;
			profile: string;
			collection: string;
			item: string;
			name: string;
			provider: string;
			game_key: string;
			mod_id: string;
			url: string;
		}>("collection-open-page", (event) => {
			const page = event.payload;
			// The payload names the mod and its provider; the normal page action only needs those and the game key.
			void unwrap(commands.openSourcePage(page.provider, page.game_key, page.mod_id, "files")).catch(() => {
				// Opening the page is a convenience: a failed opener still leaves the round waiting for the download.
			});
		});
		scope.add(off);
	});

	// A download the collection asked for imports itself with the stored FOMOD choices. Once it is ready, hand its path
	// to the backend as a file decision; the backend imports it and continues the round. This covers both the automatic
	// round's owned download and a guided member the user is downloading. It is a one-shot reaction to a queue event,
	// not a loop or a stored round counter.
	useEffect(() => {
		for (const [key, install] of installsRef.current) {
			const item = install.prompts.waiting?.item ?? install.prompts.manual?.item;

			if (install.stopped || !item || install.importing || !install.session) continue;
			const owner = install.prompts.waiting?.owner ?? `collection:${install.identity.profile}:${item}`;
			const download = downloads.find((entry) => entry.state === "ready" && entry.path && !entry.error && entry.owner === owner);

			if (!download?.path) continue;
			write(key, { ...install, importing: true });
			void (async () => {
				await resolve(key, item, { kind: "file", path: download.path ?? "" });
				patch(key, (state) => ({ ...state, importing: false }));
			})();
		}
	}, [downloads, patch, resolve, write]);

	return {
		installs,
		getInstall: read,
		fetchSession,
		create,
		stop,
		run,
		guide,
		chooseFomod,
		importManual,
		markDone,
		skip,
		finish,
	};
}
