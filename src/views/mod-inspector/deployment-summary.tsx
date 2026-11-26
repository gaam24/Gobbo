import { cn } from "cn";
import { Check, Download, FileStack, Play, Upload } from "lucide-react";
import { type ReactNode, useState } from "react";
import { gamePresentation } from "@/app/selectors/game-presentation";
import type { Status, UpdateReport } from "@/bindings";
import { BoxList, BoxListRow, Disclosure, SectionHeading } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { Empty, EmptyDescription, EmptyHeader } from "@/components/ui/empty";
import { Item, ItemActions, ItemContent, ItemDescription, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { formatTime, hasUpdate } from "@/lib/mods";
import { FactList, fact } from "@/views/mod-inspector/fact-list";

/// The steps this card can highlight. Only finishing setup has no id here; the header owns that action. The fix step is
/// informational: it is shown as current when the backend says fix-problems and has no button, so no tick claims the
/// header's action.
type StepId = "add" | "enable" | "loader" | "fix" | "deploy" | null;

/// The next steps of a game that was never deployed, and of a list that changed since the last deployment.
export interface GettingStartedActions {
	onImport?: () => void | Promise<void>;
	/// Shows what the deployment changes, then deploys.
	onDeploy?: () => void | Promise<void>;
	onOpenDownloads?: () => void;
	/// Installs the mod loader an enabled mod is waiting for.
	onInstallLoader?: () => void | Promise<void>;
}

/// A compact deployment summary. The actions live in the game header; the settings show the overlay.
export function DeploymentSummary({
	status,
	updates,
	gameRunning,
	busy,
	onOpenOverwrite,
	onRunTool,
	onImport,
	onDeploy,
	onOpenDownloads,
	onInstallLoader,
}: GettingStartedActions & {
	status: Status;
	updates: Record<string, UpdateReport>;
	/// The game is running, so an apply waits until it stops.
	gameRunning: boolean;
	/// The long action running now. The deployment buttons wait for it.
	busy: "applying" | "launching" | "stopping" | "updating" | null;
	onOpenOverwrite?: () => void;
	onRunTool?: (id: string) => void | Promise<void>;
}) {
	const { t, i18n } = useTranslation();
	const enabled = status.mods.filter((mod) => mod.enabled);
	const updatesCount = status.mods.filter((mod) => hasUpdate(updates, mod.id)).length;
	const presentation = gamePresentation(status, { gameRunning, busy });
	const blocker = presentation.blockedReason;
	/// A draft stage still reports `canApply: true`; the blocker is the other half of the answer. Both apply buttons
	/// read this one value, so neither can offer an apply the header would refuse.
	const applyDisabled = !presentation.canApply || blocker !== undefined;
	/// The backend's next action owns what to do now; the counts below only draw the progress ticks. The header runs the
	/// actions this card cannot (fixing problems, enabling mods, playing), so no button here may contradict them.
	const next = presentation.nextStep?.action ?? presentation.primary;
	const loader = next.id === "install-loader" ? next : null;
	/// The highlighted step follows the backend's next action, not the raw counts: a fix-problems action must not
	/// highlight deploying just because every mod is already enabled.
	const nextTitle = presentation.nextStep?.titleKey;
	let currentStepId: StepId = null;

	if (next.id === "import") currentStepId = "add";
	else if (nextTitle === "start.fix.title") currentStepId = "fix";
	else if (nextTitle === "start.enable.title") currentStepId = "enable";
	else if (next.id === "install-loader" || nextTitle === "start.loader.title") currentStepId = "loader";
	else if (next.id === "apply" || next.id === "apply-and-play" || nextTitle === "start.changes.title" || nextTitle === "start.deploy.title")
		currentStepId = "deploy";
	/// Only the steps this card owns get a button here, and only when the backend says that step is next.
	const startActions = () => {
		if (next.id === "import") {
			return (
				<>
					{onImport && (
						<Button size="sm" onClick={() => void onImport()}>
							<Upload /> {t(next.labelKey, next.params)}
						</Button>
					)}
					{onOpenDownloads && (
						<Button size="sm" variant="outline" onClick={onOpenDownloads}>
							<Download /> {t("start.fromSources")}
						</Button>
					)}
				</>
			);
		}

		if (loader && onInstallLoader) {
			return (
				<Button size="sm" onClick={() => void onInstallLoader()}>
					<Download /> {t(loader.labelKey, loader.params)}
				</Button>
			);
		}

		if ((next.id === "apply" || next.id === "apply-and-play") && onDeploy) {
			return <ApplyButton disabled={applyDisabled} onClick={() => void onDeploy()} />;
		}

		return null;
	};

	/// The list of enabled mods is open only on request; a closed disclosure does not mount its rows.
	const [deployListOpen, setDeployListOpen] = useState(false);

	return (
		<section className="grid content-start gap-6 p-5">
			{!status.deployed && (
				<GettingStarted
					steps={[
						{
							id: "add",
							done: status.mods.length > 0,
							title: t("start.add.title"),
							hint: t("start.add.hint"),
						},
						{
							id: "enable",
							done: enabled.length > 0,
							title: t("start.enable.title"),
							hint: t("start.enable.hint"),
						},
						...(loader
							? [
									{
										id: "loader" as const,
										done: false,
										title: t("start.loader.title", loader.params),
										hint: t("start.loader.hint", loader.params),
									},
								]
							: []),
						...(currentStepId === "fix"
							? [
									{
										id: "fix" as const,
										done: false,
										title: t("start.fix.title"),
										hint: t("start.fix.hint"),
									},
								]
							: []),
						{
							id: "deploy",
							done: false,
							title: t("start.deploy.title"),
							hint: t("start.deploy.hint"),
						},
					]}
					currentId={currentStepId}
					actions={startActions()}
				/>
			)}
			{status.deployed && status.draft_changes > 0 && onDeploy && (
				<Item variant="outline" className="border-draft/40 bg-draft/5">
					<ItemContent>
						<ItemTitle>{t("start.changes.title", { count: status.draft_changes })}</ItemTitle>
						<ItemDescription>{t("start.changes.hint")}</ItemDescription>
					</ItemContent>
					<ItemActions>
						<ApplyButton variant="outline" disabled={applyDisabled} onClick={() => void onDeploy()} />
					</ItemActions>
				</Item>
			)}
			{/* The game name, the enabled count and the filter counts are already in the header and the toolbar. This block
			    says only what the game runs now; while changes wait, the card above says that, so the state line is left out. */}
			{status.deployed && (
				<div className="grid gap-1">
					<h2 className="text-base font-semibold">{t("deploy.title")}</h2>
					{status.draft_changes === 0 && <p className="text-muted-foreground text-sm">{t("deploy.state.deployed")}</p>}
				</div>
			)}

			{status.deployed && (
				<FactList
					facts={[
						fact(t("deploy.lastDeployed"), status.deployed_at ? formatTime(status.deployed_at * 1000, i18n.language) : null),
						status.conflict_count > 0
							? fact(t("deploy.conflicts"), status.conflict_count, {
									className: "tabular-nums",
								})
							: null,
						updatesCount > 0
							? fact(t("deploy.updates"), updatesCount, {
									className: "tabular-nums",
								})
							: null,
					]}
				/>
			)}
			{blocker && <p className="text-muted-foreground text-sm">{t(blocker)}</p>}

			{status.has_overwrite && onOpenOverwrite && (
				<Item variant="outline" className="flex-col items-stretch gap-2">
					<ItemContent>
						<div className="flex gap-2">
							<FileStack className="text-muted-foreground mt-0.5 size-4 shrink-0" aria-hidden />
							<span className="text-sm">{t("overwrite.notice")}</span>
						</div>
					</ItemContent>
					<ItemActions>
						<Button variant="outline" size="sm" onClick={onOpenOverwrite}>
							{t("overwrite.show")}
						</Button>
					</ItemActions>
				</Item>
			)}

			{status.tools.length > 0 && onRunTool && (
				<section className="grid gap-2">
					<SectionHeading title={t("tools.title")} hint={t("tools.hint")} />
					<BoxList>
						{status.tools.map((tool) => (
							<BoxListRow key={tool.id}>
								<ItemContent>
									<ItemTitle>{tool.name}</ItemTitle>
								</ItemContent>
								<ItemActions>
									<Button
										variant="outline"
										size="sm"
										disabled={gameRunning || !status.deployed}
										title={t("tools.needsOverlay")}
										onClick={() => void onRunTool(tool.id)}
									>
										<Play /> {t("tools.run")}
									</Button>
								</ItemActions>
							</BoxListRow>
						))}
					</BoxList>
				</section>
			)}

			<Disclosure title={t("deploy.toDeploy")} count={enabled.length} open={deployListOpen} onOpenChange={setDeployListOpen}>
				{enabled.length === 0 ? (
					<Empty className="border border-dashed">
						<EmptyHeader>
							<EmptyDescription>{t("deploy.toDeployEmpty")}</EmptyDescription>
						</EmptyHeader>
					</Empty>
				) : (
					<BoxList ordered scroll tall>
						{enabled.map((mod) => (
							<BoxListRow key={mod.id} className="flex-nowrap py-1.5">
								<ItemContent className="min-w-0">
									<ItemTitle className="block w-full truncate" title={mod.info.name}>
										{mod.info.name}
									</ItemTitle>
								</ItemContent>
								<ItemActions>
									<span className="text-subtle-foreground font-mono text-xs tabular-nums">{mod.priority}</span>
								</ItemActions>
							</BoxListRow>
						))}
					</BoxList>
				)}
				<p className="text-muted-foreground text-xs">{t("mods.help")}</p>
			</Disclosure>
		</section>
	);
}

/// The apply label and disabled state are shared by both buttons: one for a first deployment, one for a draft.
function ApplyButton({
	disabled,
	onClick,
	variant = "default",
}: {
	disabled: boolean;
	onClick: () => void;
	variant?: "default" | "outline";
}) {
	const { t } = useTranslation();
	return (
		<Button size="sm" variant={variant} disabled={disabled} onClick={onClick}>
			{t("start.deploy.action")}
		</Button>
	);
}

/// The way to a first deployment, as steps with a tick. The highlighted step is the backend's next action; the ticks
/// only draw progress and stay decorative.
function GettingStarted({
	steps,
	currentId,
	actions,
}: {
	steps: Array<{ id: StepId; done: boolean; title: string; hint: string }>;
	currentId: StepId;
	/// The buttons of the step to do now.
	actions: ReactNode;
}) {
	const { t } = useTranslation();
	const current = steps.findIndex((step) => step.id === currentId);
	return (
		<section aria-label={t("start.title")} className="bg-muted grid gap-2 rounded-lg p-4">
			<SectionHeading title={t("start.title")} />
			<ol className="grid gap-3">
				{steps.map((step, index) => (
					<li key={step.title} className="flex gap-3" aria-current={index === current ? "step" : undefined}>
						<span
							aria-hidden
							className={cn(
								"grid size-6 shrink-0 place-items-center rounded-full border text-xs font-semibold tabular-nums",
								step.done && "bg-applied text-background border-transparent",
								index === current && "border-foreground text-foreground",
								!step.done && index !== current && "text-subtle-foreground",
							)}
						>
							{step.done ? <Check className="size-3.5" /> : index + 1}
						</span>
						<div className="grid min-w-0 gap-0.5">
							<p
								className={cn(
									"text-sm font-medium",
									step.done && "text-muted-foreground line-through",
									index > current && current >= 0 && "text-muted-foreground",
								)}
							>
								{step.title}
							</p>
							{index === current && <p className="text-muted-foreground text-xs">{step.hint}</p>}
							{index === current && <div className="mt-2 flex flex-wrap gap-2 empty:hidden">{actions}</div>}
						</div>
					</li>
				))}
			</ol>
		</section>
	);
}
