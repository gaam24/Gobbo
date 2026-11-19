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

/// The next steps of a game that was never deployed, and of a list that changed since the last deployment.
export interface GettingStartedActions {
	onImport?: () => void | Promise<void>;
	/// Shows what the deployment changes, then deploys.
	onDeploy?: () => void | Promise<void>;
	onOpenDownloads?: () => void;
}

/// A compact deployment summary. The actions live in the game header; the settings show the overlay.
export function DeploymentSummary({
	gameName,
	status,
	updates,
	gameRunning,
	busy,
	onOpenOverwrite,
	onRunTool,
	onImport,
	onDeploy,
	onOpenDownloads,
}: GettingStartedActions & {
	gameName: string;
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
	/// The list of enabled mods is open only on request; a closed disclosure does not mount its rows.
	const [deployListOpen, setDeployListOpen] = useState(false);

	return (
		<section className="grid content-start gap-6 p-5">
			{!status.deployed && (
				<GettingStarted
					steps={[
						{ done: status.mods.length > 0, title: t("start.add.title"), hint: t("start.add.hint") },
						{ done: enabled.length > 0, title: t("start.enable.title"), hint: t("start.enable.hint") },
						{ done: false, title: t("start.deploy.title"), hint: t("start.deploy.hint") },
					]}
					actions={
						status.mods.length === 0 ? (
							<>
								{onImport && (
									<Button size="sm" onClick={() => void onImport()}>
										<Upload /> {t("mods.import")}
									</Button>
								)}
								{onOpenDownloads && (
									<Button size="sm" variant="outline" onClick={onOpenDownloads}>
										<Download /> {t("start.fromNexus")}
									</Button>
								)}
							</>
						) : (
							enabled.length > 0 &&
							onDeploy && (
								<Button size="sm" disabled={!presentation.canApply} onClick={() => void onDeploy()}>
									{t("start.deploy.action")}
								</Button>
							)
						)
					}
				/>
			)}
			{status.deployed && status.draft_changes > 0 && onDeploy && (
				<Item variant="outline" className="border-draft/40 bg-draft/5">
					<ItemContent>
						<ItemTitle>{t("start.changes.title", { count: status.draft_changes })}</ItemTitle>
						<ItemDescription>{t("start.changes.hint")}</ItemDescription>
					</ItemContent>
					<ItemActions>
						<Button size="sm" variant="outline" disabled={!presentation.canApply} onClick={() => void onDeploy()}>
							{t("start.changes.action")}
						</Button>
					</ItemActions>
				</Item>
			)}
			<div className="grid gap-1">
				<h2 className="text-base font-semibold">{t("deploy.title")}</h2>
				<p className="text-muted-foreground text-sm">{status.deployed ? t("deploy.state.deployed") : t("deploy.state.draft")}</p>
				{status.game_subtitle && <p className="text-subtle-foreground text-sm">{status.game_subtitle}</p>}
			</div>

			<FactList
				facts={[
					fact(t("deploy.game"), gameName, { className: "truncate" }),
					fact(t("deploy.enabled"), t("deploy.enabledCount", { enabled: enabled.length, total: status.mods.length }), {
						className: "tabular-nums",
					}),
					fact(t("deploy.conflicts"), status.conflict_count, { className: "tabular-nums" }),
					fact(t("deploy.updates"), updatesCount, { className: "tabular-nums" }),
					fact(t("deploy.lastDeployed"), status.deployed_at ? formatTime(status.deployed_at * 1000, i18n.language) : null, {
						className: "tabular-nums",
					}),
				]}
			/>
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

/// The way to a first deployment, as steps with a tick: the first step not done is the one to do now, and its buttons
/// sit right under it.
function GettingStarted({
	steps,
	actions,
}: {
	steps: Array<{ done: boolean; title: string; hint: string }>;
	/// The buttons of the step to do now.
	actions: ReactNode;
}) {
	const { t } = useTranslation();
	const current = steps.findIndex((step) => !step.done);
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
