import { useCallback, useEffect, useRef } from "react";

import { type AppDialog, useUiActions } from "@/app/contexts/ui-context";
import { commands, type ImportProgress, type PackDownload, type PackImportView, type PackOptions } from "@/bindings";
import { useTranslation } from "@/i18n";
import { unwrap } from "@/lib/ipc";
import { missingList } from "@/lib/mods";
import { fileName } from "@/lib/paths";
import type { ImportState } from "@/lib/progress-store";
import { needsGameSwitch } from "@/lib/task-open";
import { registerTask, removeTask, updateTask } from "@/lib/tasks-store";
import { newProgressChannel, pickFiles, pickSaveFile } from "@/lib/tauri";
import { useDeferredOpen } from "@/lib/use-deferred-open";

/// Namespaced ids so a task from one game, pack or link never replaces another one in the Working panel.
const importTaskId = (game: string, path: string) => `pack-import:${game}|${path}`;
const downloadTaskId = (game: string, path: string) => `pack-download:${game}|${path}`;

/// Mod packs: export the chosen profiles, import one after a preview, and download the archives a pack is missing.
export function usePackActions({
	forGame,
	reload,
	fillSourceInfo,
	setProgress,
	notify,
	attempt,
	selectGame,
	activeGameId,
}: {
	forGame: (action: (gameId: string) => Promise<void>) => Promise<void>;
	reload: (gameId: string) => Promise<void>;
	fillSourceInfo: (gameId: string) => Promise<unknown>;
	setProgress: (key: string, state: ImportState | null, gameId?: string) => void;
	notify: (message: string) => void;
	attempt: (action: () => Promise<void>) => Promise<boolean>;
	selectGame?: (gameId: string) => void;
	activeGameId?: string;
}) {
	const { t } = useTranslation();
	const { openDialog, closeDialog } = useUiActions();

	/// A task's Open can be clicked while another game is shown. A switch closes every shell dialog, so remember the
	/// dialog and open it after the task's game becomes active. This hook stays mounted, unlike the dialog that started
	/// the task. The ref keeps the registered callback fresh.
	const { defer: deferOpen } = useDeferredOpen<{
		gameId: string;
		dialog: AppDialog;
	}>(
		activeGameId,
		(pending) => pending.gameId === activeGameId,
		(pending) => openDialog(pending.dialog),
	);
	const openDialogOf = useCallback(
		(gameId: string, dialog: AppDialog) => {
			if (selectGame && needsGameSwitch(gameId, activeGameId)) {
				deferOpen({ gameId, dialog });
				selectGame(gameId);
				return;
			}

			openDialog(dialog);
		},
		[activeGameId, deferOpen, openDialog, selectGame],
	);
	const openDialogOfRef = useRef(openDialogOf);
	useEffect(() => {
		openDialogOfRef.current = openDialogOf;
	}, [openDialogOf]);
	/// Opens a shell dialog for `gameId`, switching to that game first when another one is shown.
	const openForGame = useCallback((gameId: string, dialog: AppDialog) => openDialogOfRef.current(gameId, dialog), []);
	const openFromTask = (gameId: string, path: string) => openForGame(gameId, { kind: "pack-import", path });

	const packImported = (result: PackImportView) => {
		const done = t("notice.packImported", {
			archives: result.imported,
			created: result.profiles_created,
			updated: result.profiles_updated,
		});
		const failed = result.failed.length > 0 ? ` ${t("notice.packFailed", { list: result.failed.join(", ") })}` : "";
		const missing = missingList(result.missing);
		notify(`${done}${failed}${missing ? ` ${t("notice.packMissing", { list: missing })}` : ""}`);
	};

	const exportPack = (options: PackOptions) =>
		forGame(async (gameId) => {
			const path = await pickSaveFile(`${options.info.name || gameId}.gobbopack`, t("pack.filterName"), "gobbopack");

			if (!path) return;
			closeDialog();
			const result = await unwrap(commands.exportPack(gameId, path, options));
			notify(
				t("notice.packExported", {
					path,
					mods: result.mods,
					archives: result.archives,
				}),
			);
		});

	const pickPack = () => {
		void attempt(async () => {
			const [path] = await pickFiles(t("pack.filterName"), ["gobbopack"], false);

			if (path) openDialog({ kind: "pack-import", path });
		});
	};

	const pickPackLink = () => openDialog({ kind: "pack-link", link: "" });

	const importPack = (path: string) =>
		forGame(async (gameId) => {
			const name = fileName(path);
			const id = importTaskId(gameId, path);
			const onCancel = () => {
				void attempt(async () => {
					await commands.cancelPackImport(gameId, path);
				});
			};
			registerTask({
				id,
				game: gameId,
				kind: "import",
				title: t("tasks.packImport"),
				detail: name,
				percent: 0,
				state: "running",
				cancel: { label: t("common.cancel"), run: onCancel },
				open: {
					label: t("tasks.openWindow"),
					run: () => openFromTask(gameId, path),
				},
				source: "local",
				dedupe: { Pack: { game: gameId, path } },
			});
			const progress = newProgressChannel<ImportProgress>((step) => {
				setProgress(
					path,
					{
						stage: step.stage,
						percent: step.percent,
						name,
						index: 1,
						total: 1,
						onCancel,
					},
					gameId,
				);
				updateTask(id, { percent: step.percent });
			});
			// The bar goes however the import ends; `unwrap` then reports a failure. No `finally`: React Compiler would
			// skip the whole component.
			const importing = commands.importPackWithProgress(gameId, path, progress);
			let result: PackImportView;

			try {
				result = await unwrap(importing);
			} catch (failure) {
				setProgress(path, null, gameId);
				removeTask(id);
				throw failure;
			}

			setProgress(path, null, gameId);
			removeTask(id);
			packImported(result);
			await reload(gameId);
			void fillSourceInfo(gameId);
		});

	const downloadPack = (path: string) =>
		forGame(async (gameId) => {
			const id = downloadTaskId(gameId, path);
			const name = fileName(path);
			const onCancel = () => {
				void attempt(async () => {
					await commands.cancelPackDownload(gameId, path);
				});
			};
			registerTask({
				id,
				game: gameId,
				kind: "download",
				title: t("tasks.packDownload"),
				detail: name,
				percent: 0,
				state: "running",
				cancel: { label: t("common.cancel"), run: onCancel },
				open: {
					label: t("tasks.openWindow"),
					run: () => openFromTask(gameId, path),
				},
				source: "local",
				dedupe: { Pack: { game: gameId, path } },
			});
			const progress = newProgressChannel<PackDownload>((step) => {
				setProgress(path, { stage: "downloading", ...step }, gameId);
				updateTask(id, {
					percent: step.percent,
					detail: `${step.name} · ${step.index}/${step.total}`,
				});
			});
			// The bar goes however the download ends; `unwrap` then reports a failure. No `finally`: React Compiler
			// would skip the whole component.
			const download = commands.downloadPackMods(gameId, path, progress);

			try {
				await download;
			} catch (failure) {
				setProgress(path, null, gameId);
				removeTask(id);
				throw failure;
			}

			setProgress(path, null, gameId);
			removeTask(id);
			packImported(await unwrap(download));
			await reload(gameId);
			void fillSourceInfo(gameId);
		});

	return {
		exportPack,
		pickPack,
		pickPackLink,
		importPack,
		downloadPack,
		openForGame,
	};
}
