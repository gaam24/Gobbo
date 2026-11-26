import { cn } from "cn";
import { Bell, Download, HardDrive, Plus, Settings } from "lucide-react";
import type { ReactNode } from "react";
import type { AppView } from "@/app/prefs";
import type { GameView, ManagedGame } from "@/bindings";
import { GameTile } from "@/components/game-tile";
import { GobboMark } from "@/components/gobbo-mark";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useTranslation } from "@/i18n";
import { type Notice, unreadSummary } from "@/lib/notices";
import { shortcutHint } from "@/lib/shortcuts";

/// What the downloads button shows: see `downloadSummary`.
interface DownloadsBadge {
	running: number;
	percent: number | null;
	waiting: number;
	failed: number;
}

/// The game rail on the left: Games at the top, one tile per added game, and downloads, notifications and the Gobbo
/// settings at the bottom.
export function GameRail({
	games,
	managed,
	activeId,
	activeState = null,
	view,
	paths,
	notices,
	downloads,
	onSelect,
	onView,
	onOpen,
	onCopy,
	onRemove,
}: {
	/// Games added to Gobbo. Only these have tiles; the supported but not added ones are in Games.
	games: GameView[];
	managed: ManagedGame[];
	activeId: string | undefined;
	/// What the active game is doing, for the dot on its tile: it runs, or its list has changes waiting.
	activeState?: "running" | "draft" | null;
	view: AppView;
	/// The active game's folders, because `Status` loads on its change; the other games have no paths.
	paths: {
		library: string;
		archives: string;
		profile: string | null;
		overwrite: string | null;
	} | null;
	notices: Notice[];
	downloads: DownloadsBadge;
	onSelect: (id: string) => void;
	onView: (view: "games" | "app" | "downloads" | "notifications" | "storage") => void;
	onOpen: (gameId: string, path: string) => void | Promise<void>;
	onCopy: (path: string) => void | Promise<void>;
	onRemove: (id: string) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	let badgeTone = "bg-primary";

	if (downloads.failed > 0) badgeTone = "bg-destructive";
	else if (downloads.running > 0) badgeTone = "bg-foreground";
	const unread = unreadSummary(notices);

	return (
		<nav aria-label={t("app.addedGames")} className="bg-card flex w-18 shrink-0 flex-col items-center gap-2 border-r py-3">
			<Button
				variant="ghost"
				aria-label={t("games.title")}
				title={t("games.title")}
				aria-current={view === "games" ? "page" : undefined}
				className="h-auto w-16 shrink-0 flex-col gap-1.5 px-1 py-2"
				onClick={() => onView("games")}
			>
				{/* The wrapper fixes the mark's box: the button's generic `svg` rule would otherwise shrink it to 16 px. */}
				<span className="flex h-5 w-9 items-center justify-center">
					<GobboMark className="size-full" />
				</span>
				<span className="text-xs leading-4 font-semibold">{t("app.shortName")}</span>
			</Button>
			{/* The top padding leaves room for the first tile's ring: `overflow-y-auto` would otherwise clip it. */}
			<div className="flex min-h-0 w-full flex-1 flex-col items-center gap-2 overflow-y-auto pt-1">
				{games.map((game) => {
					const entry = managed.find((item) => item.id === game.id);
					return (
						<GameTile
							key={game.id}
							game={game}
							managedRoot={entry?.game_root ?? ""}
							ready={entry?.ready ?? false}
							active={game.id === activeId && (view === "mods" || view === "settings")}
							state={game.id === activeId ? activeState : null}
							paths={game.id === activeId ? paths : null}
							onSelect={() => onSelect(game.id)}
							onOpen={(path) => onOpen(game.id, path)}
							onCopy={onCopy}
							onRemove={onRemove}
						/>
					);
				})}
				<RailButton
					label={t("app.addGame")}
					current={false}
					className="border-subtle-foreground border border-dashed"
					onClick={() => onView("games")}
				>
					<Plus className="size-5" aria-hidden />
				</RailButton>
			</div>
			<div className="flex w-full shrink-0 flex-col items-center gap-2">
				<RailButton label={t("downloads.title")} current={view === "downloads"} className="relative" onClick={() => onView("downloads")}>
					<Download className="size-5" aria-hidden />
					{downloads.running + downloads.waiting + downloads.failed > 0 && (
						<span
							className={cn(
								"text-background absolute -top-0.5 -right-0.5 grid min-w-5 place-items-center rounded-full px-1 text-xs font-medium",
								badgeTone,
							)}
						>
							{downloads.running + downloads.waiting + downloads.failed}
						</span>
					)}
					{downloads.percent !== null && (
						<span aria-hidden className="bg-muted absolute inset-x-2.5 bottom-1.5 h-1 overflow-hidden rounded-full">
							<span className="bg-foreground block h-full rounded-full" style={{ width: `${downloads.percent}%` }} />
						</span>
					)}
				</RailButton>
				<RailButton
					label={unread.count > 0 ? t("notifications.unread", { count: unread.count }) : t("notifications.title")}
					current={view === "notifications"}
					className="relative"
					onClick={() => onView("notifications")}
				>
					<Bell className="size-5" aria-hidden />
					{unread.count > 0 && (
						<span
							className={cn(
								"text-background absolute -top-0.5 -right-0.5 grid min-w-5 place-items-center rounded-full px-1 text-xs font-medium",
								unread.error ? "bg-destructive" : "bg-foreground",
							)}
						>
							{unread.count}
						</span>
					)}
				</RailButton>
				<RailButton label={t("storage.title")} current={view === "storage"} onClick={() => onView("storage")}>
					<HardDrive className="size-5" aria-hidden />
				</RailButton>
				<RailButton
					label={t("shortcuts.withKey", { label: t("app.settings"), key: shortcutHint("settings") })}
					current={view === "app"}
					onClick={() => onView("app")}
				>
					<Settings className="size-5" aria-hidden />
				</RailButton>
			</div>
		</nav>
	);
}

/// A rail button without text: the label is its accessible name and its tooltip.
function RailButton({
	label,
	current,
	disabled,
	className,
	onClick,
	children,
}: {
	label: string;
	current: boolean;
	disabled?: boolean;
	className?: string;
	onClick: () => void;
	children: ReactNode;
}) {
	return (
		<Tooltip>
			<TooltipTrigger asChild>
				<Button
					variant="ghost"
					aria-label={label}
					aria-current={current ? "page" : undefined}
					disabled={disabled}
					className={cn("size-12", current && "bg-muted text-foreground", className)}
					onClick={onClick}
				>
					{children}
				</Button>
			</TooltipTrigger>
			<TooltipContent side="right">{label}</TooltipContent>
		</Tooltip>
	);
}
