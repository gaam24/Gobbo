import { cn } from "cn";
import {
	Boxes,
	ChevronDown,
	ChevronUp,
	CircleAlert,
	CircleArrowUp,
	Download,
	HardDrive,
	Hourglass,
	Layers,
	OctagonX,
	Play,
	Upload,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { useActivity } from "@/app/hooks/use-activity";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import {
	groupedTasks,
	type Task,
	type TaskCounts,
	type TaskKind,
	type TaskStages,
	type TaskState,
	taskAnnouncement,
	taskCounts,
} from "@/lib/tasks";

/// The one place long-running work is visible, on every screen and for every game. It floats above the content, so it
/// does not take a row from the list, and it folds to its one-line summary when the user wants the space back.
export function ActivityCenter() {
	const { t } = useTranslation();
	const tasks = useActivity();
	const games = useMemo(() => groupedTasks(tasks), [tasks]);
	const counts = useMemo(() => taskCounts(tasks), [tasks]);
	// The card floats over the content, so it opens by itself only for work in progress or work that waits for the user.
	// Failures alone keep it folded: the summary still says how many failed, and a click shows them.
	const active = counts.running + counts.waiting;
	const [open, setOpen] = useState(active > 0);
	const previousActive = useRef(active);
	useEffect(() => {
		if (active > previousActive.current) setOpen(true);
		previousActive.current = active;
	}, [active]);
	/// A click on Working in the tray opens the card again even if the user had folded it.
	useEffect(() => {
		const show = () => setOpen(true);
		window.addEventListener("gobbo:tray-working", show);
		return () => window.removeEventListener("gobbo:tray-working", show);
	}, []);

	// The live region follows state changes, not progress ticks: a percent update leaves the message untouched and the
	// failure toast keeps its own announcement, so a screen reader does not hear the same work twice.
	const previous = useRef(counts);
	const [announcement, setAnnouncement] = useState("");
	useEffect(() => {
		const next = taskAnnouncement(previous.current, counts, (key) => t(key));
		previous.current = counts;

		if (next) setAnnouncement(next);
	}, [counts, t]);

	return (
		<>
			{/* Kept mounted while the card is hidden, so the first task is still announced. */}
			<p className="sr-only" role="status" aria-live="polite">
				{announcement}
			</p>
			{tasks.length > 0 && (
				<section
					aria-label={t("tasks.title")}
					aria-busy={counts.running > 0}
					className="bg-card text-card-foreground fixed right-4 bottom-12 z-40 w-[min(24rem,calc(100vw-2rem))] rounded-xl border shadow-lg"
				>
					<header className="flex items-center gap-2 py-2 pr-2 pl-3.5">
						<Summary counts={counts} />
						<Button
							variant="ghost"
							size="icon"
							className="ml-auto size-7 shrink-0"
							aria-label={open ? t("tasks.hide") : t("tasks.show")}
							aria-expanded={open}
							onClick={() => setOpen(!open)}
						>
							{open ? <ChevronDown /> : <ChevronUp />}
						</Button>
					</header>
					{open && (
						<div className="grid max-h-[min(26rem,45vh)] gap-4 overflow-y-auto overscroll-contain border-t px-3.5 pt-3 pb-3.5">
							{games.map((game) => (
								<section key={game.game ?? "global"} data-game={game.game ?? undefined} aria-label={game.gameName} className="grid gap-2.5">
									{game.gameName && (
										<h3 className="text-muted-foreground truncate text-xs font-medium" title={game.gameName}>
											{game.gameName}
										</h3>
									)}
									<ul className="grid gap-3.5">
										{game.tasks.map((task) => (
											<TaskRow key={task.id} task={task} />
										))}
									</ul>
								</section>
							))}
						</div>
					)}
				</section>
			)}
		</>
	);
}

/// The card's title is the state of the work in one sentence, each part in its state's colour, so the card still reads
/// when it is folded: "1 running, 1 needs you, 1 failed".
function Summary({ counts }: { counts: TaskCounts }) {
	const { t } = useTranslation();
	const parts: Array<{ key: string; text: string; className: string }> = [];

	if (counts.running > 0)
		parts.push({
			key: "running",
			text: t("tasks.summary.running", { count: counts.running }),
			className: "text-foreground",
		});

	if (counts.waiting > 0)
		parts.push({
			key: "waiting",
			text: t("tasks.summary.waiting", { count: counts.waiting }),
			className: "text-draft",
		});

	if (counts.failed > 0)
		parts.push({
			key: "failed",
			text: t("tasks.summary.failed", { count: counts.failed }),
			className: "text-destructive",
		});

	return (
		<p className="flex min-w-0 flex-wrap items-center text-sm font-medium">
			{counts.running > 0 && <Spinner role="presentation" aria-hidden="true" className="text-primary mr-2 size-3.5" />}
			{parts.map((part, index) => (
				<span key={part.key} className={part.className}>
					{part.text}
					{index < parts.length - 1 && <span className="text-muted-foreground mr-1">{","}</span>}
				</span>
			))}
		</p>
	);
}

/// One task: what it works on, how far it got and, when it stopped, why. Long text is cut to the card's width and shown
/// in full on hover; the reason of a failure wraps instead, because it is the one line the user needs to read.
function TaskRow({ task }: { task: Task }) {
	const { t } = useTranslation();
	// A row of global work has no game group above it, so it names its game itself.
	const detail = task.detail ?? (task.game ? undefined : task.gameName);
	const actions = task.onOpen ?? task.onRetry ?? task.onCancel ?? task.onDismiss;
	const showPercent = task.state === "running" && task.percent !== undefined;
	const tracked = task.stages !== undefined && task.stages.current >= 0;
	return (
		<li className="grid gap-1.5">
			<div className="flex min-w-0 items-start gap-2">
				<TaskIcon kind={task.kind} state={task.state} />
				<div className="grid min-w-0 flex-1 gap-0.5">
					<p className="truncate text-sm font-medium" title={task.title}>
						{task.title}
					</p>
					{detail && (
						<p className="text-muted-foreground truncate font-mono text-xs" title={detail}>
							{detail}
						</p>
					)}
				</div>
				{showPercent && (
					<span className="text-muted-foreground mt-0.5 shrink-0 font-mono text-xs tabular-nums">{`${String(task.percent)}%`}</span>
				)}
			</div>
			<div className="grid gap-1.5 pl-6">
				{tracked && task.stages ? (
					<StageTrack stages={task.stages} state={task.state} label={task.title} />
				) : (
					showPercent && <Progress value={task.percent} aria-label={task.title} className="h-1" />
				)}
				{task.state === "waiting" && <p className="text-draft text-xs">{t("tasks.waitingForYou")}</p>}
				{task.error && (
					<p className="text-destructive line-clamp-3 text-xs text-pretty break-words" title={task.error}>
						{task.error}
					</p>
				)}
				{actions && (
					<div className="flex flex-wrap items-center gap-1.5 pt-0.5">
						{task.onOpen && (
							<Button variant="outline" size="sm" onClick={task.onOpen}>
								{task.openLabel ?? t("tasks.openWindow")}
							</Button>
						)}
						{task.onRetry && (
							<Button variant="outline" size="sm" onClick={task.onRetry}>
								{task.actionLabel ?? t("common.retry")}
							</Button>
						)}
						{task.onCancel && (
							<Button variant="ghost" size="sm" onClick={task.onCancel}>
								{task.cancelLabel ?? t("common.cancel")}
							</Button>
						)}
						{task.onDismiss && (
							<Button variant="ghost" size="sm" onClick={task.onDismiss}>
								{t("tasks.dismiss")}
							</Button>
						)}
					</div>
				)}
			</div>
		</li>
	);
}

/// The steps of one task as a segmented track, with their names under it. Finished steps are filled, the current step is
/// half-filled while it runs and red when the work stopped there, later steps stay empty. The current name is the only
/// one in full colour, so the track reads without colour too.
function StageTrack({ stages, state, label }: { stages: TaskStages; state: TaskState; label: string }) {
	const { names, current } = stages;
	const columns = {
		gridTemplateColumns: `repeat(${names.length}, minmax(0, 1fr))`,
	};
	return (
		<div className="grid gap-1">
			<div
				className="grid gap-0.5"
				style={columns}
				role="progressbar"
				aria-label={label}
				aria-valuemin={0}
				aria-valuemax={names.length}
				aria-valuenow={Math.min(current, names.length)}
				aria-valuetext={names[current] ?? names.at(-1)}
			>
				{names.map((name, index) => (
					<span
						key={name}
						className={cn(
							"h-1 rounded-full transition-colors duration-150",
							index < current && "bg-primary",
							index === current && (state === "failed" ? "bg-destructive" : "bg-primary/45"),
							index > current && "bg-border",
						)}
					/>
				))}
			</div>
			<div className="grid gap-0.5 text-xs" style={columns} aria-hidden="true">
				{names.map((name, index) => (
					<span key={name} className={cn("truncate", index === current ? "text-foreground" : "text-subtle-foreground")}>
						{name}
					</span>
				))}
			</div>
		</div>
	);
}

/// A running row shows what kind of work it is; a row that waits or failed shows its state instead, in its colour and
/// next to the text that says the same, so the state never depends on colour alone.
function TaskIcon({ kind, state }: { kind: TaskKind; state: TaskState }) {
	const className = "mt-0.5 size-4 shrink-0";

	if (state === "failed") return <CircleAlert className={cn(className, "text-destructive")} aria-hidden />;

	if (state === "waiting") return <Hourglass className={cn(className, "text-draft")} aria-hidden />;
	return <KindIcon kind={kind} className={cn(className, "text-muted-foreground")} />;
}

function KindIcon({ kind, className }: { kind: TaskKind; className: string }) {
	switch (kind) {
		case "apply":
			return <Layers className={className} aria-hidden />;
		case "launch":
			return <Play className={className} aria-hidden />;
		case "stop":
			return <OctagonX className={className} aria-hidden />;
		case "import":
			return <Upload className={className} aria-hidden />;
		case "update":
			return <CircleArrowUp className={className} aria-hidden />;
		case "scan":
			return <HardDrive className={className} aria-hidden />;
		case "download":
		case "install":
			return <Download className={className} aria-hidden />;
		case "collection":
			return <Boxes className={className} aria-hidden />;
		default:
			return <span className={className} aria-hidden />;
	}
}
