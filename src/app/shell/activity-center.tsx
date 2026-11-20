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
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";

import { useActivity } from "@/app/hooks/use-activity";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { groupedTasks, TASK_GROUP_LABEL, type Task, type TaskKind, type TaskState, taskAnnouncement, taskCounts } from "@/lib/tasks";

/// The one place long-running work is visible, on every screen. It floats above the content, so it does not take a row
/// from the list, and it can be folded to a small pill when the user wants the space back.
export function ActivityCenter() {
	const { t } = useTranslation();
	const tasks = useActivity();
	const groups = useMemo(() => groupedTasks(tasks), [tasks]);
	const counts = useMemo(() => taskCounts(tasks), [tasks]);
	// The card floats over the content, so it opens by itself only for work in progress or work that waits for the user.
	// Failures alone keep it folded: the header still shows how many need attention, and a click shows them.
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
					<header className="flex items-center gap-2 px-3 py-2.5">
						<span className="text-sm font-medium">{t("tasks.title")}</span>
						<div className="flex min-w-0 flex-wrap items-center gap-1">
							{counts.running > 0 && (
								<SummaryBadge
									variant="secondary"
									count={counts.running}
									label={t(TASK_GROUP_LABEL.running)}
									icon={<Spinner role="presentation" aria-hidden="true" className="text-primary size-3" />}
								/>
							)}
							{counts.waiting > 0 && (
								<SummaryBadge
									variant="outline"
									count={counts.waiting}
									label={t(TASK_GROUP_LABEL.waiting)}
									icon={<Hourglass aria-hidden className="text-draft size-3" />}
									className="text-draft"
								/>
							)}
							{counts.failed > 0 && (
								<SummaryBadge
									variant="destructive"
									count={counts.failed}
									label={t(TASK_GROUP_LABEL.failed)}
									icon={<CircleAlert aria-hidden className="size-3" />}
								/>
							)}
						</div>
						<Button
							variant="ghost"
							size="icon"
							className="ml-auto size-7"
							aria-label={open ? t("tasks.hide") : t("tasks.show")}
							aria-expanded={open}
							onClick={() => setOpen(!open)}
						>
							{open ? <ChevronDown /> : <ChevronUp />}
						</Button>
					</header>
					{open && (
						<div className="grid max-h-[min(26rem,45vh)] gap-3 overflow-y-auto overscroll-contain border-t px-3 py-3">
							{groups.map((group) => (
								<div key={group.state} data-state={group.state} className="grid gap-1.5">
									<h3 className="text-muted-foreground flex items-center gap-1.5 text-xs font-medium">
										{t(TASK_GROUP_LABEL[group.state])}
										<span className="font-mono tabular-nums">{group.count}</span>
									</h3>
									{group.games.map((game) => (
										<div key={game.game ?? "global"} data-game={game.game ?? undefined} className="grid gap-1.5">
											{game.gameName && (
												<p className="text-subtle-foreground truncate pl-1 text-xs" title={game.gameName}>
													{game.gameName}
													<span className="text-muted-foreground ml-1 font-mono tabular-nums">{game.count}</span>
												</p>
											)}
											<ul className="grid gap-2">
												{game.tasks.map((task) => (
													<TaskRow key={task.id} task={task} />
												))}
											</ul>
										</div>
									))}
								</div>
							))}
						</div>
					)}
				</section>
			)}
		</>
	);
}

function SummaryBadge({
	variant,
	count,
	label,
	icon,
	className,
}: {
	variant: "secondary" | "outline" | "destructive";
	count: number;
	label: string;
	icon: ReactNode;
	className?: string;
}) {
	return (
		<Badge variant={variant} className={cn("gap-1", className)}>
			{icon}
			<span className="font-mono tabular-nums">{count}</span>
			<span>{label}</span>
		</Badge>
	);
}

/// One task: what it is, what it works on and, for a failure, why. Long text is cut to the card's width and shown in
/// full on hover; the reason of a failure wraps instead, because it is the one line the user needs to read.
function TaskRow({ task }: { task: Task }) {
	const { t } = useTranslation();
	// A row of global work has no game group above it, so it names its game itself.
	const detail = task.detail ?? (task.game ? undefined : task.gameName);
	const actions = task.onOpen ?? task.onRetry ?? task.onCancel ?? task.onDismiss;
	return (
		<li className="flex items-start gap-2 py-0.5">
			<TaskIcon kind={task.kind} state={task.state} />
			<div className="grid min-w-0 flex-1 gap-1">
				<div className="grid min-w-0 gap-0.5">
					{task.state === "waiting" && <span className="text-draft text-xs font-medium">{t("tasks.waitingForYou")}</span>}
					<p className="truncate text-sm font-medium" title={task.title}>
						{task.title}
					</p>
					{detail && (
						<p className="text-muted-foreground truncate font-mono text-xs" title={detail}>
							{detail}
						</p>
					)}
					{task.error && (
						<p className="text-destructive line-clamp-3 text-xs text-pretty break-words" title={task.error}>
							{task.error}
						</p>
					)}
				</div>
				{task.state === "running" && task.percent !== undefined && (
					<Progress value={task.percent} aria-label={task.title} aria-valuetext={detail ?? `${task.percent}%`} />
				)}
				{actions && (
					<div className="flex flex-wrap items-center gap-1.5">
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

/// A running row is identified by what it is doing. A failed or waiting row leads with its state instead, and keeps the
/// kind icon small beside it so the work is still identifiable at a glance.
function TaskIcon({ kind, state }: { kind: TaskKind; state: TaskState }) {
	if (state === "running") return <KindIcon kind={kind} className="text-muted-foreground mt-0.5 size-4 shrink-0" />;
	return (
		<span className="mt-0.5 flex shrink-0 items-center gap-1" aria-hidden="true">
			{state === "failed" ? <CircleAlert className="text-destructive size-4" /> : <Hourglass className="text-draft size-4" />}
			<KindIcon kind={kind} className="text-subtle-foreground size-3" />
		</span>
	);
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
			return null;
	}
}
