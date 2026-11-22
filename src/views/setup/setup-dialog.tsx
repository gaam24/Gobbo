import { CircleAlert, CircleCheck, CircleX, Download, ExternalLink, RefreshCw } from "lucide-react";
import { type ReactNode, useState } from "react";

import { useNavigation } from "@/app/contexts/navigation-context";
import { useNoticeActions } from "@/app/contexts/notices-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import {
	type CheckState,
	commands,
	type LoaderChoice,
	type LoaderCopyView,
	type LoaderSetupView,
	type PreflightCheck,
	type PreflightReport,
} from "@/bindings";
import { BoxList, BoxListRow, Disclosure } from "@/components/app/content-blocks";
import { StatusBadge } from "@/components/app/status-badge";
import { DialogCloseFooter, ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { Spinner } from "@/components/ui/spinner";
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
			{report.loader && <LoaderCard loader={report.loader} installing={installing} onInstall={onInstallLoader} />}
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

/// The loader package, its source and the copies in the library. The official release is offered, never forced: a build
/// a mod site publishes for this game (often newer, or patched for it) is a copy the user can pick just as well. The one
/// the user picks becomes the only enabled copy; the overlay serves it like every other mod and the game folder is not
/// written.
function LoaderCard({
	loader,
	installing,
	onInstall,
}: {
	loader: LoaderSetupView;
	installing: boolean;
	onInstall: (choice: LoaderChoice) => void;
}) {
	const { t, i18n } = useTranslation();
	let tone: "success" | "update" | "warning" = "warning";
	let state = t("setupCheck.loader.missing");

	if (loader.state === "installed") {
		tone = "success";
		state = t("setupCheck.loader.installed");
	} else if (loader.state === "update") {
		tone = "update";
		state = t("setupCheck.loader.update");
	}

	const active = loader.copies.find((copy) => copy.enabled);
	let installed = loader.installed_version;

	if (installed === null && active) {
		installed = active.name;
	} else if (installed === null && loader.state === "installed") {
		installed = t("setupCheck.loader.manual");
	}

	const size = loader.size > 0 ? formatSize(Math.ceil(loader.size / 1024), i18n.language) : null;
	// A manual install in the game folder needs no button; any other state offers the official release unless it is
	// already the copy in use.
	const manual = loader.state === "installed" && !active;
	const officialInUse = active?.official === true;
	let officialLabel = t("setupCheck.loader.installAction", { name: loader.name, version: loader.version });

	if (loader.state === "update") {
		officialLabel = t("setupCheck.loader.updateAction", { name: loader.name, version: loader.version });
	} else if (loader.copies.some((copy) => copy.official)) {
		officialLabel = t("setupCheck.loader.useOfficialAction", { name: loader.name, version: loader.version });
	}

	return (
		<section className="grid gap-2 rounded-lg border p-3" aria-labelledby="setup-loader-title">
			<div className="flex flex-wrap items-center justify-between gap-2">
				<div className="grid gap-0.5">
					<h3 id="setup-loader-title" className="text-sm font-semibold">
						{loader.name}
					</h3>
					<p className="text-muted-foreground text-xs">{t("setupCheck.loader.hint", { name: loader.name })}</p>
				</div>
				<StatusBadge tone={tone}>{state}</StatusBadge>
			</div>
			{installed && (
				<p className="text-sm">
					<span className="text-muted-foreground">{t("setupCheck.loader.inUseLabel")}</span> {installed}
				</p>
			)}
			{loader.copies.length > 0 && (
				<BoxList>
					{loader.copies.map((copy) => (
						<LoaderCopyRow
							key={copy.folder}
							copy={copy}
							installing={installing}
							onUse={() => onInstall({ kind: "library", folder: copy.folder })}
						/>
					))}
				</BoxList>
			)}
			<Disclosure title={t("setupCheck.loader.official", { version: loader.version })}>
				<dl className="grid gap-1 text-xs sm:grid-cols-[auto_1fr] sm:gap-x-3">
					<dt className="text-muted-foreground">{t("setupCheck.loader.source")}</dt>
					<dd className="truncate font-mono" title={loader.source}>
						{loader.source}
					</dd>
					<dt className="text-muted-foreground">{t("setupCheck.loader.hash")}</dt>
					<dd className="truncate font-mono" title={loader.sha256}>
						{loader.sha256.slice(0, 16)}
					</dd>
					{size && (
						<>
							<dt className="text-muted-foreground">{t("setupCheck.loader.size")}</dt>
							<dd className="tabular-nums">{size}</dd>
						</>
					)}
					<dt className="text-muted-foreground">{t("setupCheck.loader.target")}</dt>
					<dd className="font-mono break-all">{loader.target === "" ? t("setupCheck.loader.gameFolder") : loader.target}</dd>
				</dl>
			</Disclosure>
			{!(manual || officialInUse) && (
				<Button
					type="button"
					variant={active ? "ghost" : "outline"}
					size="sm"
					className="w-fit"
					disabled={installing}
					onClick={() => onInstall({ kind: "official" })}
				>
					{installing ? <Spinner aria-hidden /> : <Download />}
					{officialLabel}
				</Button>
			)}
		</section>
	);
}

/// One copy of the loader in the library, with the button that makes it the copy in use.
function LoaderCopyRow({ copy, installing, onUse }: { copy: LoaderCopyView; installing: boolean; onUse: () => void }) {
	const { t } = useTranslation();
	return (
		<BoxListRow className="flex-nowrap items-center gap-2">
			<div className="grid min-w-0 flex-1 gap-0.5">
				<p className="truncate text-sm" title={copy.name}>
					{copy.name}
					{copy.version && <span className="text-muted-foreground"> {copy.version}</span>}
				</p>
				<p className="text-muted-foreground text-xs">
					{copy.official ? t("setupCheck.loader.copyOfficial") : t("setupCheck.loader.copyOther")}
				</p>
			</div>
			{copy.enabled ? (
				<StatusBadge tone="success">{t("setupCheck.loader.inUse")}</StatusBadge>
			) : (
				<Button type="button" variant="outline" size="sm" disabled={installing} onClick={onUse}>
					{t("setupCheck.loader.useAction")}
				</Button>
			)}
		</BoxListRow>
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
