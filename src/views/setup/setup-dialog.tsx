import { RefreshCw } from "lucide-react";
import { type ReactNode, useState } from "react";

import { useNavigation } from "@/app/contexts/navigation-context";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { commands, type LoaderChoice, type PreflightReport } from "@/bindings";
import { BoxList, Disclosure, StatusLine } from "@/components/app/content-blocks";
import { FailureAlert } from "@/components/app/failure-alert";
import { StatusBadge } from "@/components/app/status-badge";
import { DialogCloseFooter, ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { messageOf, translate, unwrap } from "@/lib/ipc";
import { formatSize } from "@/lib/mods";
import { useLoad } from "@/lib/use-load";
import { CheckRow } from "@/views/setup/check-row";
import { LoaderCard } from "@/views/setup/loader-card";

/// The setup checklist of a game: every check says whether it is ready, and a failing check has the action that fixes
/// it. It reads the same status as the header, so the two never disagree.
export function SetupDialog({ gameId, onClose }: { gameId: string; onClose: () => void }) {
	const { t } = useTranslation();
	const { openGameSettings, openMods, openStorage } = useNavigation();
	const { attempt } = useNoticeActions();
	const { gameActions } = useWorkspace();
	const [retry, setRetry] = useState(0);
	const [failure, setFailure] = useState<string | null>(null);
	const [installing, setInstalling] = useState(false);
	const report = useLoad(`${gameId}\n${retry}`, () => unwrap(commands.preflight(gameId)), {
		onError: (error) => setFailure(messageOf(error)),
		onSuccess: () => setFailure(null),
	});

	const installLoader = async (choice: LoaderChoice) => {
		setInstalling(true);
		const ok = await gameActions.installLoader(gameId, choice);
		setInstalling(false);

		if (ok) setRetry((count) => count + 1);
	};

	let body: ReactNode;

	if (failure) {
		body = <FailureAlert title={t("setupCheck.failed", { detail: failure })} onRetry={() => setRetry((count) => count + 1)} />;
	} else if (report) {
		body = (
			<ReportBody
				report={report}
				installing={installing}
				onInstallLoader={(choice) => void installLoader(choice)}
				// Close the checklist first: navigating underneath a modal leaves the destination behind the scrim.
				onOpenSettings={() => {
					onClose();
					openGameSettings();
				}}
				onShowMods={() => {
					onClose();
					openMods();
				}}
				onOpenStorage={() => {
					onClose();
					openStorage();
				}}
				onGetRequirement={(path) =>
					void attempt(async () => {
						await unwrap(commands.openRequirementPage(gameId, path));
					})
				}
				onRetry={() => setRetry((count) => count + 1)}
			/>
		);
	} else {
		body = <StatusLine tone="loading">{t("setupCheck.loading")}</StatusLine>;
	}

	return (
		<ModalDialog title={t("setupCheck.title")} description={t("setupCheck.hint")} width="lg" onClose={onClose}>
			{body}
			<DialogCloseFooter onClose={onClose} />
		</ModalDialog>
	);
}

function ReportBody({
	report,
	installing,
	onInstallLoader,
	onOpenSettings,
	onShowMods,
	onOpenStorage,
	onGetRequirement,
	onRetry,
}: {
	report: PreflightReport;
	installing: boolean;
	onInstallLoader: (choice: LoaderChoice) => void;
	onOpenSettings: () => void;
	onShowMods: () => void;
	onOpenStorage: () => void;
	onGetRequirement: (path: string) => void;
	onRetry: () => void;
}) {
	const { t, i18n } = useTranslation();
	const attention = report.checks.filter((check) => check.state !== "ok");
	const passed = report.checks.filter((check) => check.state === "ok");
	let badgeKey: "setupCheck.notReady" | "setupCheck.readyWithWarnings" | "setupCheck.ready" = "setupCheck.ready";

	if (!report.ready) badgeKey = "setupCheck.notReady";
	else if (attention.length > 0) badgeKey = "setupCheck.readyWithWarnings";
	const row = (check: PreflightReport["checks"][number]) => (
		<CheckRow
			key={check.id}
			check={check}
			free={check.id === "space" && report.free_space_kb > 0 ? formatSize(report.free_space_kb, i18n.language) : null}
			onOpenSettings={onOpenSettings}
			onShowMods={onShowMods}
			onOpenStorage={onOpenStorage}
			onGetRequirement={onGetRequirement}
		/>
	);
	return (
		<>
			<div className="flex items-center justify-between gap-2">
				<StatusBadge tone={report.ready && attention.length === 0 ? "success" : "warning"}>{t(badgeKey)}</StatusBadge>
				<Button variant="ghost" size="sm" onClick={onRetry}>
					<RefreshCw /> {t("setupCheck.checkAgain")}
				</Button>
			</div>
			{report.loader && <LoaderCard loader={report.loader} installing={installing} onInstall={onInstallLoader} />}
			{/* What needs the user comes first; the checks that pass fold away, so one warning is not lost among eight ticks. */}
			{attention.length > 0 && <BoxList>{attention.map((check) => row(check))}</BoxList>}
			{passed.length > 0 &&
				(attention.length === 0 ? (
					<BoxList>{passed.map((check) => row(check))}</BoxList>
				) : (
					<Disclosure title={t("setupCheck.passed")} count={passed.length}>
						<BoxList>{passed.map((check) => row(check))}</BoxList>
					</Disclosure>
				))}
			{(report.problems.length > 0 || report.warnings.length > 0) && (
				<Disclosure title={t("setupCheck.details")}>
					<ul className="grid gap-1 pl-4 text-sm">
						{report.problems.map((problem) => (
							<li key={`${problem.code}${JSON.stringify(problem.params)}`} className="text-destructive list-disc">
								{translate(problem)}
							</li>
						))}
						{report.warnings.map((warning) => (
							<li key={`${warning.code}${JSON.stringify(warning.params)}`} className="text-muted-foreground list-disc">
								{translate(warning)}
							</li>
						))}
					</ul>
				</Disclosure>
			)}
		</>
	);
}
