import { Suspense, useCallback, useEffect, useRef, useState } from "react";
import { useDownloadsContext } from "@/app/contexts/downloads-context";
import { useGames } from "@/app/contexts/games-context";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { useSettingsData } from "@/app/contexts/settings-context";
import { useUiActions, useUiState } from "@/app/contexts/ui-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { useManualUpdates } from "@/app/hooks/use-manual-updates";
import {
	ArchiveReportDialog,
	CollectionDialog,
	CommunityProfileDialog,
	ConflictExplorerDialog,
	DefinitionImportDialog,
	DeleteModDialog,
	DeployPreviewDialog,
	FomodWizard,
	GroupEditDialog,
	HealthDialog,
	ManualUpdateDialog,
	OverwriteDialog,
	PackExportDialog,
	PackImportDialog,
	PackLinkDialog,
	ReinstallDialog,
	SetupDialog,
	ShortcutsDialog,
	SnapshotDialog,
	SortPreviewDialog,
	TranslationHintsDialog,
	TranslationWinnersDialog,
} from "@/app/lazy-views";
import type { UpdateChange } from "@/bindings";
import { useTranslation } from "@/i18n";
import { errorText, messageOf } from "@/lib/ipc";
import { registerTask, removeTask } from "@/lib/tasks-store";
import { fomodWizardKey, savedChoices } from "@/lib/use-import";
import { matchesUpdateDownload } from "@/sources/update-targets";

/// The dialogs and wizards that belong to the shell. Each one is mounted only while it is open, so its chunk and its
/// state are created on demand. Long-running sequences live in always-mounted controllers here, not in a dialog.
export function AppDialogs() {
	const { t, i18n } = useTranslation();
	const { dialog } = useUiState();
	const { openDialog, closeAllDialogs, closeDialog } = useUiActions();
	const { reloadGames } = useGames();
	const { notify, notifyError } = useNoticeActions();
	const { readyProviders } = useSettingsData();
	const { downloads } = useDownloadsContext();
	const {
		gameId,
		status,
		gameActions,
		packExport,
		importPack,
		downloadPack,
		openFolderPath,
		dismissPendingFamily,
		fomodWizard,
		confirmFomod,
		skipFomod,
	} = useWorkspace();

	/// A dialog belongs to the game that opened it. Switching games closes the whole stack instead of retargeting it at
	/// another game. The dialogs themselves never cancel backend work; that is owned by the Working tasks.
	const lastGame = useRef(gameId);
	useEffect(() => {
		if (lastGame.current === gameId) return;
		lastGame.current = gameId;
		closeAllDialogs();
	}, [closeAllDialogs, gameId]);

	/// A ref to the top dialog lets the Stop action of a backgrounded task close the dialog only when it is actually the
	/// one showing, so it cannot close an unrelated dialog the user opened meanwhile.
	const dialogRef = useRef(dialog);
	useEffect(() => {
		dialogRef.current = dialog;
	}, [dialog]);
	const closeDialogIfManual = useCallback(() => {
		if (dialogRef.current?.kind === "manual-updates") closeDialog();
	}, [closeDialog]);

	const manual = useManualUpdates({ downloads, openDialog, closeDialogIfManual });

	const confirmPreview = async () => {
		if (dialog?.kind !== "deploy") return;
		const owner = gameId;
		const deployed = await gameActions.runDeploy();
		// The deploy can take a while. A game switch in that time closes the dialog; never launch a game the user left.
		if (deployed && dialog.launch && lastGame.current === owner) await gameActions.runLaunch(dialog.executable, false);
	};

	/// The browser step selected the hand-downloaded versions. One toast carries the whole run and one undo. The owner
	/// game is passed in so the undo keeps working after a game switch.
	const finishManualUpdates = useCallback(
		(owner: string, changes: UpdateChange[], failures: string[]) => {
			// A run can finish in the background while another dialog is on top; only close our own dialog.
			closeDialogIfManual();
			if (changes.length > 0) {
				notify(t("notice.modsUpdated", { count: changes.length }), {
					label: t("notice.undo"),
					onClick: () => gameActions.undoUpdates(owner, changes),
				});
			}
			if (failures.length > 0) notifyError(failures.join("\n"));
		},
		[closeDialogIfManual, gameActions, notify, notifyError, t],
	);

	/// Capture the actions and the owning game when a browser run starts. The callbacks close over the active game id
	/// at that moment, so they keep using it after the user switches away.
	useEffect(() => {
		if (dialog?.kind !== "manual-updates" || !gameId) return;
		const owner = gameId;
		void manual.start(dialog.targets, owner, {
			openPage: (target) => {
				void gameActions.openUpdatePage(target.id, target.provider);
			},
			selectLatest: async (target) => {
				await gameActions.selectUpdateVersion(target.id, target.provider);
				dismissPendingFamily(target.id);
			},
			describeDownloadError: errorText,
			describeSelectionError: messageOf,
			matchesDownload: (target, download) => matchesUpdateDownload(target, download, owner),
			finish: finishManualUpdates,
		});
	}, [dialog, dismissPendingFamily, finishManualUpdates, gameActions, gameId, manual]);

	/// A FOMOD import wizard waiting for choices stays in the Working panel. Closing the wizard hides it but leaves the
	/// import's resolver pending, so Open can bring it back. Stop resolves it as a skip. The hidden id is the wizard's
	/// own key, so a different wizard is shown by default without a reset effect.
	const [wizardHiddenFor, setWizardHiddenFor] = useState<string | null>(null);
	const wizardKey = fomodWizard ? fomodWizardKey(fomodWizard.fileName, fomodWizard.moduleName) : null;
	const wizardHidden = wizardKey !== null && wizardHiddenFor === wizardKey;
	useEffect(() => {
		if (!fomodWizard) return;
		const id = `import-wizard:${gameId ?? "none"}`;
		registerTask({
			id,
			game: gameId ?? null,
			kind: "install",
			title: t("tasks.customChoices"),
			detail: t("tasks.waitingForYou"),
			state: "waiting",
			open: { label: t("tasks.openWindow"), run: () => setWizardHiddenFor(null) },
			cancel: {
				label: t("tasks.stop"),
				run: () => {
					removeTask(id);
					skipFomod();
					setWizardHiddenFor(null);
				},
			},
			source: "local",
		});
		return () => {
			removeTask(id);
		};
	}, [fomodWizard, gameId, skipFomod, t]);

	return (
		<>
			{dialog?.kind === "deploy" && gameId && (
				<Suspense fallback={null}>
					<DeployPreviewDialog
						key={dialog.request}
						gameId={gameId}
						launch={dialog.launch}
						request={dialog.request}
						onConfirm={confirmPreview}
						onClose={closeDialog}
					/>
				</Suspense>
			)}
			{(dialog?.kind === "remove-mod" || dialog?.kind === "remove-version" || dialog?.kind === "remove-page") && gameId && status && (
				<Suspense fallback={null}>
					<DeleteModDialog
						gameId={gameId}
						status={status}
						modId={dialog.kind === "remove-page" ? null : dialog.modId}
						pageId={dialog.kind === "remove-page" ? dialog.pageId : null}
						versionFolder={dialog.kind === "remove-version" ? dialog.folder : null}
						onDelete={gameActions.deleteMod}
						onDeleteVersion={gameActions.deleteModVersion}
						onDeletePage={gameActions.deleteModPage}
						onClose={closeDialog}
						onError={notifyError}
					/>
				</Suspense>
			)}
			{dialog?.kind === "definition-import" && (
				<Suspense fallback={null}>
					<DefinitionImportDialog
						path={dialog.path}
						onClose={closeDialog}
						onImported={async (name) => {
							await reloadGames();
							notify(t("editor.import.imported", { name }));
						}}
					/>
				</Suspense>
			)}
			{dialog?.kind === "collection" && (
				<Suspense fallback={null}>
					<CollectionDialog onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "pack-export" && gameId && status && (
				<Suspense fallback={null}>
					<PackExportDialog gameId={gameId} profiles={status.profiles} onExport={packExport} onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "pack-link" && gameId && (
				<Suspense fallback={null}>
					<PackLinkDialog gameId={gameId} link={dialog.link} onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "pack-import" && gameId && (
				<Suspense fallback={null}>
					<PackImportDialog
						key={`${gameId}\n${dialog.path}`}
						gameId={gameId}
						path={dialog.path}
						hasNexusKey={readyProviders.has("nexus")}
						onImport={() => importPack(dialog.path)}
						onDownload={() => downloadPack(dialog.path)}
						onClose={closeDialog}
					/>
				</Suspense>
			)}
			{dialog?.kind === "reinstall" && gameId && (
				<Suspense fallback={null}>
					<ReinstallDialog
						gameId={gameId}
						folder={dialog.folder}
						name={dialog.name}
						onConfirm={(choices) => {
							closeDialog();
							const owner = gameId;
							const id = `reinstall:${owner}|${dialog.folder}`;
							registerTask({
								id,
								game: owner,
								kind: "install",
								title: t("tasks.reinstalling"),
								detail: dialog.name,
								state: "running",
								source: "local",
							});
							void (async () => {
								try {
									await gameActions.reinstallMod(dialog.folder, choices);
								} catch {
									// The action reports its own failure; the task only stops showing work.
								}
								removeTask(id);
							})();
						}}
						onClose={closeDialog}
					/>
				</Suspense>
			)}
			{dialog?.kind === "setup" && gameId && (
				<Suspense fallback={null}>
					<SetupDialog gameId={gameId} onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "health" && gameId && (
				<Suspense fallback={null}>
					<HealthDialog gameId={gameId} onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "snapshots" && gameId && (
				<Suspense fallback={null}>
					<SnapshotDialog gameId={gameId} onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "archive-report" && gameId && (
				<Suspense fallback={null}>
					<ArchiveReportDialog gameId={gameId} folder={dialog.folder} name={dialog.name} onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "community-profile" && gameId && (
				<Suspense fallback={null}>
					<CommunityProfileDialog gameId={gameId} onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "sort" && gameId && (
				<Suspense fallback={null}>
					<SortPreviewDialog gameId={gameId} onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "manual-updates" && manual.view.active && (
				<Suspense fallback={null}>
					<ManualUpdateDialog
						view={manual.view}
						onNext={() => void manual.next()}
						onFinish={() => void manual.finish()}
						onOpenPage={manual.openCurrentPage}
						onClose={closeDialog}
					/>
				</Suspense>
			)}
			{dialog?.kind === "conflicts" && gameId && (
				<Suspense fallback={null}>
					<ConflictExplorerDialog gameId={gameId} onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "overwrite" && gameId && (
				<Suspense fallback={null}>
					<OverwriteDialog
						gameId={gameId}
						locale={i18n.language}
						onToMod={gameActions.overwriteToMod}
						onClear={gameActions.clearOverwrite}
						onOpenFolder={openFolderPath}
						onClose={closeDialog}
					/>
				</Suspense>
			)}
			{dialog?.kind === "shortcuts" && (
				<Suspense fallback={null}>
					<ShortcutsDialog open onOpenChange={(open) => !open && closeDialog()} />
				</Suspense>
			)}
			{dialog?.kind === "group-edit" && status && (
				<Suspense fallback={null}>
					<GroupEditDialog id={dialog.id} members={dialog.members} onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "translation-winners" && gameId && status && (
				<Suspense fallback={null}>
					<TranslationWinnersDialog gameId={gameId} groupId={dialog.groupId} onClose={closeDialog} />
				</Suspense>
			)}
			{dialog?.kind === "translation-hints" && status && (
				<Suspense fallback={null}>
					<TranslationHintsDialog onClose={closeDialog} />
				</Suspense>
			)}
			{fomodWizard && !wizardHidden && (
				<Suspense fallback={null}>
					<FomodWizard
						key={wizardKey ?? ""}
						moduleName={fomodWizard.moduleName}
						steps={fomodWizard.steps}
						fileName={fomodWizard.fileName}
						game={fomodWizard.game}
						target={{ kind: "archive", path: fomodWizard.path }}
						initialChoices={gameId && wizardKey ? savedChoices(gameId, wizardKey) : undefined}
						onConfirm={(choices) => {
							confirmFomod(choices);
							setWizardHiddenFor(null);
						}}
						onSkip={() => setWizardHiddenFor(wizardKey)}
					/>
				</Suspense>
			)}
		</>
	);
}
