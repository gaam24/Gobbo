import { useEffect, useRef } from "react";
import type { RunReport } from "@/bindings";
import { Empty, EmptyDescription, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { useTranslation } from "@/i18n";
import { formatTime } from "@/lib/mods";
import { GameChanges } from "@/views/console/game-changes";
import { GameLogs } from "@/views/console/game-logs";
import { LaunchDlls } from "@/views/console/launch-dlls";
import { MountsPanel } from "@/views/console/mounts-panel";
import { OverlayDecisions } from "@/views/console/overlay-decisions";
import { OverlayProbe } from "@/views/console/overlay-probe";
import { StepLine } from "@/views/console/step-line";

/// A small count on a tab. Zero is not shown: a tab without content should not advertise one.
function TabCount({ value }: { value: number }) {
	if (value === 0) return null;
	return <span className="text-subtle-foreground font-mono text-xs tabular-nums">{value}</span>;
}

/// The facts of the launch, the game's output, and the diagnostics, split into tabs so a long report stays readable.
export function LaunchBody({ report, gameId }: { report: RunReport; gameId: string }) {
	const { t, i18n } = useTranslation();
	const outputRef = useRef<HTMLTextAreaElement>(null);
	const pinned = useRef(true);

	const output = report.output ?? [];
	const outputText = output.join("\n");

	/// Follows the end only when the text changed: the console reads the report again every two seconds, and a poll that
	/// found nothing new must not move the field.
	useEffect(() => {
		const field = outputRef.current;
		if (field && pinned.current) field.scrollTop = field.scrollHeight;
		// oxlint-disable-next-line react/exhaustive-effect-dependencies -- outputText is the trigger; the field is read through the ref.
	}, [outputText]);
	const dllCount = (report.dlls?.injected.length ?? 0) + (report.dlls?.layer.length ?? 0);
	const logCount = (report.game_logs?.length ?? 0) + (report.mod_logs?.length ?? 0) + ((report.app_log?.length ?? 0) > 0 ? 1 : 0);
	let gameState = t("runs.gameNotStarted");
	if (report.game_started) {
		if (report.game_exit_code === null) gameState = t("runs.gameRunning");
		else if (report.game_exit_code === 0) gameState = t("runs.gameClosed");
		else gameState = t("runs.gameExited", { code: report.game_exit_code });
	}

	return (
		<Tabs defaultValue="summary" className="flex min-h-0 flex-1 flex-col gap-0">
			<div className="shrink-0 overflow-x-auto border-b px-6">
				<TabsList variant="line" aria-label={t("console.title")} className="h-9 w-fit min-w-full justify-start gap-1">
					<TabsTrigger value="summary" className="flex-none px-3">
						{t("console.tabSummary")}
					</TabsTrigger>
					<TabsTrigger value="output" className="flex-none px-3">
						{t("console.tabOutput")}
						<TabCount value={output.length} />
					</TabsTrigger>
					<TabsTrigger value="logs" className="flex-none px-3">
						{t("console.tabLogs")}
						<TabCount value={logCount} />
					</TabsTrigger>
					<TabsTrigger value="changes" className="flex-none px-3">
						{t("console.tabChanges")}
						<TabCount value={report.game_changes_total ?? 0} />
					</TabsTrigger>
					<TabsTrigger value="dlls" className="flex-none px-3">
						{t("console.tabDlls")}
						<TabCount value={dllCount} />
					</TabsTrigger>
					<TabsTrigger value="overlay" className="flex-none px-3">
						{t("console.tabOverlay")}
						<TabCount value={report.decisions_total ?? 0} />
					</TabsTrigger>
					<TabsTrigger value="mounts" className="flex-none px-3">
						{t("console.tabMounts")}
					</TabsTrigger>
				</TabsList>
			</div>

			<div className="min-h-0 flex-1 overflow-y-auto px-6 py-4">
				<TabsContent value="summary" className="grid gap-4">
					<dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1">
						<dt className="text-muted-foreground">{report.finished_at ? t("runs.finished") : t("runs.running")}</dt>
						<dd className="tabular-nums">{formatTime(report.started_at * 1000, i18n.language)}</dd>
						<dt className="text-muted-foreground">{t("runs.game")}</dt>
						<dd>{gameState}</dd>
					</dl>

					<section aria-labelledby="console-steps" className="grid gap-3">
						<h3 id="console-steps" className="text-sm font-medium">
							{t("console.steps")}
						</h3>
						{report.steps.length === 0 ? (
							<p className="text-muted-foreground text-sm">{t("runs.noSteps")}</p>
						) : (
							<ol className="grid gap-4">
								{report.steps.map((step) => (
									<StepLine key={`${step.id}-${step.started_at}`} step={step} />
								))}
							</ol>
						)}
					</section>
				</TabsContent>

				<TabsContent value="output" className="h-full min-h-0">
					{output.length === 0 ? (
						<Empty className="border border-dashed">
							<EmptyHeader>
								<EmptyTitle>{t("console.outputEmpty")}</EmptyTitle>
							</EmptyHeader>
						</Empty>
					) : (
						// A read-only field keeps the text selectable and reachable by keyboard, and it scrolls by itself.
						<Textarea
							ref={outputRef}
							readOnly
							aria-label={t("console.output")}
							value={outputText}
							className="bg-muted field-sizing-fixed h-full min-h-40 resize-none font-mono text-xs break-all whitespace-pre-wrap"
							onScroll={(event) => {
								const field = event.currentTarget;
								pinned.current = field.scrollHeight - field.scrollTop - field.clientHeight < 16;
							}}
						/>
					)}
				</TabsContent>

				<TabsContent value="logs" className="h-full min-h-0">
					<GameLogs report={report} />
				</TabsContent>

				<TabsContent value="changes" className="grid gap-2">
					<GameChanges report={report} />
				</TabsContent>

				<TabsContent value="dlls" className="grid gap-2">
					{report.dlls ? (
						<LaunchDlls dlls={report.dlls} />
					) : (
						<Empty className="border border-dashed">
							<EmptyHeader>
								<EmptyTitle>{t("console.dllsNoReport")}</EmptyTitle>
								<EmptyDescription>{t("console.dllsHint")}</EmptyDescription>
							</EmptyHeader>
						</Empty>
					)}
				</TabsContent>

				<TabsContent value="overlay" className="grid gap-6">
					<OverlayProbe gameId={gameId} />
					<OverlayDecisions report={report} />
				</TabsContent>

				<TabsContent value="mounts" className="grid gap-6">
					<MountsPanel gameId={gameId} />
				</TabsContent>
			</div>
		</Tabs>
	);
}
