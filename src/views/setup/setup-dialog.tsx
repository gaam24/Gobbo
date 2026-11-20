import { CircleAlert, CircleCheck, CircleX, ExternalLink, RefreshCw } from "lucide-react";
import { type ReactNode, useState } from "react";

import { useNavigation } from "@/app/contexts/navigation-context";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { type CheckState, commands, type PreflightCheck, type PreflightReport } from "@/bindings";
import { BoxList, BoxListRow, Disclosure } from "@/components/app/content-blocks";
import { StatusBadge } from "@/components/app/status-badge";
import { DialogCloseFooter, ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { useTranslation } from "@/i18n";
import { messageOf, translate, unwrap } from "@/lib/ipc";
import { formatSize } from "@/lib/mods";
import { useLoad } from "@/lib/use-load";

const ICON = {
	ok: CircleCheck,
	warning: CircleAlert,
	problem: CircleX,
} as const satisfies Record<CheckState, typeof CircleCheck>;

/// The setup checklist of a game: every check says whether it is ready, and a failing check has the action that fixes
/// it. It reads the same status as the header, so the two never disagree.
export function SetupDialog({ gameId, onClose }: { gameId: string; onClose: () => void }) {
	const { t } = useTranslation();
	const { openGameSettings, openMods, openStorage } = useNavigation();
	const { attempt } = useNoticeActions();
	const [retry, setRetry] = useState(0);
	const [failure, setFailure] = useState<string | null>(null);
	const report = useLoad(`${gameId}\n${retry}`, () => unwrap(commands.preflight(gameId)), {
		onError: (error) => setFailure(messageOf(error)),
		onSuccess: () => setFailure(null),
	});

	let body: ReactNode;

	if (failure) {
		body = (
			<>
				<FieldError>{t("setupCheck.failed", { detail: failure })}</FieldError>
				<Button variant="outline" size="sm" className="w-fit" onClick={() => setRetry((count) => count + 1)}>
					<RefreshCw /> {t("common.retry")}
				</Button>
			</>
		);
	} else if (report) {
		body = (
			<ReportBody
				report={report}
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
		body = <p className="text-muted-foreground text-sm">{t("setupCheck.loading")}</p>;
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
	onOpenSettings,
	onShowMods,
	onOpenStorage,
	onGetRequirement,
	onRetry,
}: {
	report: PreflightReport;
	onOpenSettings: () => void;
	onShowMods: () => void;
	onOpenStorage: () => void;
	onGetRequirement: (path: string) => void;
	onRetry: () => void;
}) {
	const { t, i18n } = useTranslation();
	return (
		<>
			<div className="flex items-center justify-between gap-2">
				<StatusBadge tone={report.ready ? "success" : "warning"}>
					{report.ready ? t("setupCheck.ready") : t("setupCheck.notReady")}
				</StatusBadge>
				<Button variant="ghost" size="sm" onClick={onRetry}>
					<RefreshCw /> {t("setupCheck.checkAgain")}
				</Button>
			</div>
			<BoxList>
				{report.checks.map((check) => (
					<CheckRow
						key={check.id}
						check={check}
						free={check.id === "space" && report.free_space_kb > 0 ? formatSize(report.free_space_kb, i18n.language) : null}
						onOpenSettings={onOpenSettings}
						onShowMods={onShowMods}
						onOpenStorage={onOpenStorage}
						onGetRequirement={onGetRequirement}
					/>
				))}
			</BoxList>
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

function CheckRow({
	check,
	free,
	onOpenSettings,
	onShowMods,
	onOpenStorage,
	onGetRequirement,
}: {
	check: PreflightCheck;
	free: string | null;
	onOpenSettings: () => void;
	onShowMods: () => void;
	onOpenStorage: () => void;
	onGetRequirement: (path: string) => void;
}) {
	const { t } = useTranslation();
	const Icon = ICON[check.state];
	const action = (() => {
		if (check.state === "ok") return null;

		if (check.id === "game_folder" || check.id === "settings_folder" || check.id === "mods_folder" || check.id === "overlay") {
			return (
				<Button variant="outline" size="sm" onClick={onOpenSettings}>
					{t("setupCheck.openSettings")}
				</Button>
			);
		}

		if (check.id === "loader" && check.requirement_path) {
			const path = check.requirement_path;
			const name = check.requirement_name ?? path;
			return (
				<Button variant="outline" size="sm" title={t("setupCheck.getHint", { name })} onClick={() => onGetRequirement(path)}>
					<ExternalLink /> {t("setupCheck.get", { name })}
				</Button>
			);
		}

		if (check.id === "loader" || check.id === "problems" || check.id === "mods") {
			return (
				<Button variant="outline" size="sm" onClick={onShowMods}>
					{t("setupCheck.showMods")}
				</Button>
			);
		}

		if (check.id === "space") {
			return (
				<Button variant="outline" size="sm" onClick={onOpenStorage}>
					{t("setupCheck.openStorage")}
				</Button>
			);
		}

		return null;
	})();

	return (
		<BoxListRow className="flex-nowrap items-start gap-2 py-2.5">
			<Icon
				className={check.state === "problem" ? "text-destructive mt-0.5 size-4 shrink-0" : "text-muted-foreground mt-0.5 size-4 shrink-0"}
				aria-hidden
			/>
			<div className="grid min-w-0 flex-1 gap-0.5">
				<p className="text-sm">{translate(check.message)}</p>
				{free && <p className="text-muted-foreground font-mono text-xs tabular-nums">{t("setupCheck.freeSpace", { free })}</p>}
			</div>
			{action}
		</BoxListRow>
	);
}
