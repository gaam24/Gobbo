import { useCallback, useEffect, useRef, useState } from "react";
import {
	commands,
	type FomodStepView,
	type ImportedModView,
	type ImportProgress,
	type ImportView,
	type SkippedFile,
	type SourceRef,
	type Status,
} from "@/bindings";
import { useTranslation } from "@/i18n";
import { ARCHIVE_EXTENSIONS, isArchive } from "@/lib/archives";
import { clearFailedTask, recordFailedTask } from "@/lib/failed-tasks-store";
import type { FomodChoice } from "@/lib/fomod";
import { alreadyInLibraryOf, cancelledOf, messageOf, unwrap } from "@/lib/ipc";
import type { ToastAction } from "@/lib/notify";
import { fileName } from "@/lib/paths";
import { createGameQueues, setImportProgress } from "@/lib/progress-store";
import { withoutKey } from "@/lib/records";
import { dedupeKey } from "@/lib/tasks";
import { newProgressChannel, pickFiles } from "@/lib/tauri";
import type { DownloadImported } from "@/sources/downloads";

/// Where archives come from when it is not the selected game and its file picker: a download for a game.
interface ImportSource {
	game: string;
	source: SourceRef | null;
}

/// The FOMOD wizard of one game: the installer the shell shows, and the archive its answers are read from.
interface FomodWizardEntry {
	game: string;
	path: string;
	moduleName: string;
	steps: FomodStepView[];
	fileName: string;
}

/// Answers a FOMOD wizard was confirmed with, kept for the length of a session. The key joins the game and the wizard
/// key, so one game's answers never reach another and a finished import cannot leave answers for a later archive.
export type FomodChoicesCache = Map<string, FomodChoice[]>;

const fomodChoicesCache: FomodChoicesCache = new Map();

/// The wizard key shared with the shell: the archive file name and the installer module name.
export function fomodWizardKey(archiveFile: string, moduleName: string): string {
	return `${archiveFile}\n${moduleName}`;
}

function choicesKey(gameId: string, wizardKey: string): string {
	return `${gameId}|${wizardKey}`;
}

/// The cache rules stay pure so they can be tested without a hook, a Tauri command or any game data. The copies keep a
/// caller from changing a stored answer after it was saved.
export function rememberChoices(cache: FomodChoicesCache, gameId: string, wizardKey: string, choices: FomodChoice[]): void {
	cache.set(choicesKey(gameId, wizardKey), [...choices]);
}

export function forgetChoices(cache: FomodChoicesCache, gameId: string, wizardKey: string): void {
	cache.delete(choicesKey(gameId, wizardKey));
}

/// Drops the answers of one cancelled archive, looked up by its path.
export function forgetArchivedChoices(cache: FomodChoicesCache, gameId: string, wizardKeys: Map<string, string>, path: string): void {
	const key = wizardKeys.get(path);

	if (key) forgetChoices(cache, gameId, key);
}

/// Drops every answer kept for one import run, once that run has ended.
export function forgetRunChoices(cache: FomodChoicesCache, gameId: string, wizardKeys: Map<string, string>): void {
	for (const key of wizardKeys.values()) forgetChoices(cache, gameId, key);
}

export function storedChoices(cache: FomodChoicesCache, gameId: string, wizardKey: string): FomodChoice[] | undefined {
	const choices = cache.get(choicesKey(gameId, wizardKey));
	return choices ? [...choices] : undefined;
}

/// The answers of one wizard, for the shell to pass back as `initialChoices` when it shows that wizard again.
export function savedChoices(gameId: string, wizardKey: string): FomodChoice[] | undefined {
	return storedChoices(fomodChoicesCache, gameId, wizardKey);
}

/// An archive past its preview, waiting to be imported. `choices` are the FOMOD answers, or null for a plain archive.
interface ImportJob {
	path: string;
	index: number;
	choices: FomodChoice[] | null;
}

interface UseImportOptions {
	/// The selected game, or null when no game is added.
	gameId: string | null;
	/// Called before the first command so pending profile edits of this game land first. The id is optional for existing
	/// callers; a queued import passes the game it captured before its first await.
	beforeImport: (gameId?: string) => Promise<void>;
	/// The fresh status of a game after its import. The game may no longer be the one shown.
	onStatus: (gameId: string, status: Status) => void;
	onShowMods: () => void;
	onError: (message: string) => void;
	notify: (message: string, action?: ToastAction) => void;
	/// Opens the archive report of a mod whose import left files out.
	onArchiveReport: (gameId: string, folder: string, name: string) => void;
	/// Called after a download with a known source is imported, with the ids of the mods it added to the profile.
	onSourceImport: (gameId: string, ids: string[]) => void;
}

/// The activity row of one archive. A running import and its failure share the id, so a retry cannot leave two rows.
function importTaskId(path: string): string {
	return `import:${path}`;
}

/// The failed-refresh row of one game. Refreshing the profiles is part of the import, so its recovery is a row too and
/// shares one id per game: a retry cannot leave two rows for the same game.
function syncTaskId(gameId: string): string {
	return `import-sync:${gameId}`;
}

/// Adds the files no rule took from one imported archive, when it has any.
function collectUnplaced(
	view: ImportView,
	fallbackName: string,
	into: Array<{ folder: string; name: string; files: SkippedFile[] }>,
): void {
	const folder = view.folders[0];
	const files = view.skipped_files.filter((file) => file.reason === "no_rule");

	if (!folder || files.length === 0) return;
	into.push({
		folder,
		name: view.mods.find((mod) => mod.folder === folder)?.name ?? fallbackName,
		files,
	});
}

/// Runs `task` on every item, with at most `workers` running at once. A worker takes the next item when its task ends.
async function runPool<T>(items: T[], workers: number, task: (item: T) => Promise<void>): Promise<void> {
	const queue = items.values();
	const worker = async () => {
		for (const item of queue) await task(item);
	};
	await Promise.all(Array.from({ length: Math.min(workers, items.length) }, worker));
}

/// The import queue and the FOMOD wizard. The rest of the app only renders the returned state and calls the
/// returned actions, so a drop and a file picker share one implementation. Both the queue and the wizard resolver are
/// per game: an unanswered wizard for one game must not block imports of another.
export function useImport({
	gameId: selectedGame,
	beforeImport,
	onStatus,
	onShowMods,
	onError,
	notify,
	onArchiveReport,
	onSourceImport,
}: UseImportOptions) {
	const { t } = useTranslation();
	/// The wizard of each game that is waiting for choices. Only the shown game's wizard is returned, but the others stay
	/// alive so switching back to their game brings their wizard back instead of leaving their import waiting forever.
	const [wizards, setWizards] = useState<Record<string, FomodWizardEntry>>({});
	const fomodWizard = (selectedGame ? wizards[selectedGame] : undefined) ?? null;
	/// An import that added a version of a mod which already had a selected one. Switching is the user's choice, so it
	/// waits in a banner. The game is kept with the list, so switching games hides it without losing the other game's.
	const [pending, setPending] = useState<{
		game: string;
		mods: ImportedModView[];
	} | null>(null);
	const pendingVersions = pending?.game === selectedGame ? pending.mods : [];
	const fomodResolvers = useRef(new Map<string, (choices: FomodChoice[] | null) => void>());
	/// One import queue per game: the wizard can show one archive at a time per game, and a second import of the same
	/// game must not steal its resolver. A different game has its own queue and wizard.
	const [gameQueues] = useState(createGameQueues);
	/// The serialized import entry point, for a failed task's retry. It is a ref because the failure is recorded inside
	/// `runImport`, which is defined before `importPaths`. It carries the game of the failed import.
	const retryImport = useRef<(paths: string[], source?: ImportSource, gameId?: string) => void>(() => {
		// Replaced by the effect below before any failure can be retried.
	});

	/// Opens the FOMOD wizard of one game and waits for its choices, or for a refusal (`null`). A previous wizard of
	/// that game cannot survive a second one: its import is told to skip instead of waiting forever.
	const openFomodWizard = useCallback(
		(gameId: string, inspect: { module_name: string; steps: FomodStepView[] }, file: string, path: string) =>
			new Promise<FomodChoice[] | null>((resolve) => {
				fomodResolvers.current.get(gameId)?.(null);
				fomodResolvers.current.set(gameId, resolve);
				setWizards((prev) => ({
					...prev,
					[gameId]: {
						game: gameId,
						path,
						moduleName: inspect.module_name,
						steps: inspect.steps,
						fileName: file,
					},
				}));
			}),
		[],
	);

	/// Answers the wizard of one game and drops its card. `null` skips the archive.
	const settleWizard = useCallback((gameId: string | undefined, choices: FomodChoice[] | null) => {
		if (!gameId) return;
		const resolve = fomodResolvers.current.get(gameId);
		fomodResolvers.current.delete(gameId);
		resolve?.(choices);
		setWizards((prev) => withoutKey(prev, gameId));
	}, []);

	/// Tells the user about files no rule took and offers the report of the first affected mod.
	const announceSkipped = useCallback(
		(items: Array<{ folder: string; name: string; files: SkippedFile[] }>, gameId: string) => {
			if (items.length === 0) return;
			const count = items.reduce((total, item) => total + item.files.length, 0);
			const first = items[0];
			notify(
				t("import.skippedFiles", { count, archives: items.length }),
				first && gameId === selectedGame
					? {
							label: t("import.showSkipped"),
							onClick: () => onArchiveReport(gameId, first.folder, first.name),
						}
					: undefined,
			);
		},
		[notify, onArchiveReport, selectedGame, t],
	);

	/// What every finished import shows, whoever ran it: the fresh status, the added folders, the banner for a version
	/// that waits next to the selected one, and the version check of a download with a known source.
	const settleImport = useCallback(
		(gameId: string, fresh: Status, imported: string[], added: ImportedModView[], fromSource: boolean) => {
			onStatus(gameId, fresh);

			if (imported.length > 0) notify(t("import.added", { list: imported.join(", ") }));
			// A version added to a family that already has a selected one waits for the user: an import must not silently
			// replace the version the profile uses.
			const versions = added.filter((item) => fresh.mods.some((mod) => mod.id === item.family && mod.folder !== item.folder));

			if (versions.length > 0) {
				setPending((current) => ({
					game: gameId,
					mods: [...(current?.game === gameId ? current.mods : []), ...versions].filter(
						(item, index, list) => list.findIndex((other) => other.folder === item.folder) === index,
					),
				}));
			}
			// A download with a known source is checked for its version right away, so the mod does not stay "Not checked".
			if (fromSource && imported.length > 0) {
				onSourceImport(
					gameId,
					fresh.mods.filter((mod) => imported.includes(mod.folder)).map((mod) => mod.id),
				);
			}
		},
		[notify, onSourceImport, onStatus, t],
	);

	/// A link's download the backend imported itself, with the same `acquire` path as a collection member.
	const acceptImported = useCallback(
		(event: DownloadImported) => {
			if (event.existing !== null || !event.view) {
				onStatus(event.game, event.status);
				notify(t("notice.downloadAlreadyInLibrary", { name: event.name }));
				return;
			}

			const skippedFiles: Array<{
				folder: string;
				name: string;
				files: SkippedFile[];
			}> = [];
			collectUnplaced(event.view, event.name, skippedFiles);
			announceSkipped(skippedFiles, event.game);
			settleImport(event.game, event.status, event.view.folders, event.view.mods, true);
		},
		[announceSkipped, notify, onStatus, settleImport, t],
	);

	/// Dropped archives go to the library of the game captured by the caller. They reach the game only after the next
	/// deployment. Each archive is previewed first: an FOMOD installer opens the wizard, a plain one is imported whole.
	const runImport = useCallback(
		async (paths: string[], gameId: string, source?: ImportSource) => {
			await beforeImport(gameId);
			// A download installs where it is; only the user's own archives bring the list into view.
			if (!source) onShowMods();
			const archives = paths.filter((path) => isArchive(path));

			for (const path of archives) clearFailedTask(importTaskId(path));
			const imported: string[] = [];
			const added: ImportedModView[] = [];
			const failed: string[] = [];
			const failedImports: Array<{
				path: string;
				name: string;
				message: string;
			}> = [];
			const skippedByUser: string[] = [];
			const skippedFiles: Array<{
				folder: string;
				name: string;
				files: SkippedFile[];
			}> = [];
			// Each archive is previewed first: an FOMOD installer opens the wizard, a plain one is imported whole. The
			// previews run one at a time, since the wizard shows one archive at a time. The wizard key of each FOMOD
			// archive is remembered so its saved answers can be dropped when this import ends.
			const jobs: ImportJob[] = [];
			const wizardKeys = new Map<string, string>();

			for (const [index, path] of archives.entries()) {
				const name = fileName(path);

				try {
					const inspect = await unwrap(commands.inspectZip(gameId, path));
					let choices: FomodChoice[] | null = null;

					if (inspect.kind === "fomod") {
						wizardKeys.set(path, fomodWizardKey(name, inspect.module_name));
						choices = await openFomodWizard(gameId, inspect, name, path);

						if (choices === null) {
							skippedByUser.push(name);
							continue;
						}
					}

					jobs.push({ path, index, choices });
				} catch (failure) {
					failed.push(`${name}: ${messageOf(failure)}`);
				}
			}
			// The imports run side by side, up to four at once: each one writes to a folder of its own, and the backend
			// keeps the index in order.
			await runPool(jobs, Math.min(4, navigator.hardwareConcurrency || 2), async ({ path, index, choices }) => {
				const name = fileName(path);
				const progress = newProgressChannel<ImportProgress>((step) =>
					setImportProgress(
						path,
						{
							...step,
							name,
							index: index + 1,
							total: archives.length,
							onCancel: () => void commands.cancelImport(gameId, path),
						},
						gameId,
					),
				);

				try {
					const view = await unwrap(commands.importZip(gameId, path, choices, source?.source ?? null, progress));
					imported.push(...view.folders);
					added.push(...view.mods);
					collectUnplaced(view, name, skippedFiles);
				} catch (failure) {
					// A cancel is the user's own action, not a failure to report or retry. Its saved answers are dropped
					// here too, since this archive will not be installed.
					if (cancelledOf(failure)) {
						forgetArchivedChoices(fomodChoicesCache, gameId, wizardKeys, path);
					} else if (alreadyInLibraryOf(failure)) {
						// The exact archive is already installed. That is a no-op, not a broken import: point at the copy
						// the user has and let the profile sync put it on the list. No retry row can change the outcome.
						notify(messageOf(failure));
					} else {
						const message = messageOf(failure);
						failed.push(`${name}: ${message}`);
						failedImports.push({ path, name, message });
					}
				}

				setImportProgress(path, null, gameId);
			});
			announceSkipped(skippedFiles, gameId);

			if (skippedByUser.length > 0) failed.push(t("import.skippedArchives", { list: skippedByUser.join(", ") }));

			if (paths.length > archives.length) failed.push(t("import.notArchives"));
			// Each failed import now has one durable, retryable row in the Working panel. Do not also emit a toast and a
			// persisted notice for the same failure. Notes with no recovery row (skipped archives, non-archives) still
			// get reported when no import failed, so they are never silently dropped.
			if (failed.length > 0 && failedImports.length === 0) onError(failed.join("\n"));

			for (const item of failedImports) {
				recordFailedTask({
					id: importTaskId(item.path),
					kind: "import",
					game: gameId,
					title: t("tasks.importFailed", { name: item.name }),
					detail: item.message,
					onRetry: () => retryImport.current([item.path], source, gameId),
					// The backend also keeps a lifecycle row for this import. The shared identity merges the two into one
					// Working row, so a failure is not counted twice.
					dedupeKey: dedupeKey({ Import: { game: gameId, path: item.path } }),
				});
			}

			try {
				const fresh = await unwrap(commands.syncProfiles(gameId));
				clearFailedTask(syncTaskId(gameId));
				settleImport(gameId, fresh, imported, added, Boolean(source?.source));
			} catch (failure) {
				// Refreshing the profiles has a recovery too: retry it from one durable row, not a toast and a notice.
				recordFailedTask({
					id: syncTaskId(gameId),
					kind: "import",
					game: gameId,
					title: t("tasks.importRefreshFailed"),
					detail: messageOf(failure),
					onRetry: () => retryImport.current([], undefined, gameId),
				});
			}
			// The import has ended. Answers kept for a reopened wizard must not leak into a later archive with the
			// same file and module names, so only the keys of this run are forgotten.
			forgetRunChoices(fomodChoicesCache, gameId, wizardKeys);
		},
		[announceSkipped, beforeImport, notify, onError, onShowMods, openFomodWizard, settleImport, t],
	);

	/// Serializes imports per game: the wizard holds one resolver per game, and two concurrent imports of the same game
	/// would orphan one archive. A different game has its own queue, so a pending wizard no longer blocks it.
	const enqueueImport = useCallback(
		(paths: string[], source: ImportSource | undefined, gameId: string) =>
			gameQueues.forGame(gameId).run(() => runImport(paths, gameId, source)),
		[gameQueues, runImport],
	);

	const importPaths = useCallback(
		(paths: string[], source?: ImportSource) => {
			// Capture the game before the first await; a later switch must not move this import to the new game's queue.
			const gameId = source?.game ?? selectedGame;

			if (!gameId) {
				onError(t("import.noGame"));
				return Promise.resolve();
			}

			return enqueueImport(paths, source, gameId);
		},
		[enqueueImport, onError, selectedGame, t],
	);

	useEffect(() => {
		retryImport.current = (paths, source, gameId) => {
			const target = source?.game ?? gameId;

			if (target) void enqueueImport(paths, source, target);
		};
	}, [enqueueImport]);

	/// Picking archives in the native dialog. The rest of the flow is shared with dragging files.
	const pickArchives = useCallback(async () => {
		try {
			const picked = await pickFiles(t("import.filterName"), ARCHIVE_EXTENSIONS, true);

			if (picked.length > 0) await importPaths(picked);
		} catch (failure) {
			onError(messageOf(failure));
		}
	}, [importPaths, onError, t]);

	/// Takes one imported version out of the banner, after the user chose it or kept the selected one.
	const dismissPendingVersion = useCallback((folder: string) => {
		setPending((current) =>
			current
				? {
						...current,
						mods: current.mods.filter((item) => item.folder !== folder),
					}
				: null,
		);
	}, []);

	/// Drops the banner entries of one mod family after an update step selected the imported version itself.
	const dismissPendingFamily = useCallback((family: string) => {
		setPending((current) =>
			current
				? {
						...current,
						mods: current.mods.filter((item) => item.family !== family),
					}
				: null,
		);
	}, []);

	/// Remembers the answers before resolving the import, so a wizard shown again for the same archive starts from them.
	/// Skipping resolves with `null` and deliberately leaves no answers behind.
	const confirmFomod = (choices: FomodChoice[]) => {
		const wizard = selectedGame ? wizards[selectedGame] : undefined;

		if (selectedGame && wizard) {
			rememberChoices(fomodChoicesCache, selectedGame, fomodWizardKey(wizard.fileName, wizard.moduleName), choices);
		}

		settleWizard(selectedGame ?? undefined, choices);
	};

	const skipFomod = () => settleWizard(selectedGame ?? undefined, null);

	return {
		setProgress: setImportProgress,
		importPaths,
		acceptImported,
		pickArchives,
		fomodWizard,
		savedChoices,
		confirmFomod,
		skipFomod,
		pendingVersions,
		dismissPendingVersion,
		dismissPendingFamily,
	};
}
