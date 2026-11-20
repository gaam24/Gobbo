import { cn } from "cn";
import { Bell, Download, HardDrive, Plus, Settings2 } from "lucide-react";
import { type ReactNode, useState } from "react";
import type { AppView } from "@/app/prefs";
import type { GameView, ManagedGame } from "@/bindings";
import { ConfirmRemoveGame } from "@/components/confirm-remove-game";
import { GameIcon } from "@/components/game-icon";
import { GobboMark } from "@/components/gobbo-mark";
import { Button } from "@/components/ui/button";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuLabel,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useTranslation } from "@/i18n";
import { type Notice, unreadSummary } from "@/lib/notices";

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
	view: AppView;
	/// The active game's folders, because `Status` loads on its change; the other games have no paths.
	paths: { library: string; archives: string; profile: string | null; overwrite: string | null } | null;
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
					current={view === "games"}
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
				<RailButton label={t("app.settings")} current={view === "app"} onClick={() => onView("app")}>
					<Settings2 className="size-5" aria-hidden />
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

/// A game tile: its icon, a right-click menu with folder actions, and removal with confirmation.
function GameTile({
	game,
	managedRoot,
	ready,
	active,
	paths,
	onSelect,
	onOpen,
	onCopy,
	onRemove,
}: {
	game: GameView;
	/// The game folder from the added list; empty when the game has no folder set.
	managedRoot: string;
	/// Whether the folder still exists.
	ready: boolean;
	active: boolean;
	paths: { library: string; archives: string; profile: string | null; overwrite: string | null } | null;
	onSelect: () => void;
	onOpen: (path: string) => void | Promise<void>;
	onCopy: (path: string) => void | Promise<void>;
	onRemove: (id: string) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const [confirmRemove, setConfirmRemove] = useState(false);

	return (
		<ContextMenu>
			<ContextMenuTrigger asChild>
				<Button
					variant="ghost"
					aria-label={game.name}
					aria-current={active ? "page" : undefined}
					title={game.name}
					className={cn("size-12", active && "bg-muted ring-muted-foreground ring-1")}
					onClick={onSelect}
				>
					<GameIcon id={game.id} name={game.name} className="size-7 rounded-md" />
				</Button>
			</ContextMenuTrigger>
			<ContextMenuContent>
				<ContextMenuLabel>{t("games.menu.open")}</ContextMenuLabel>
				<ContextMenuItem
					disabled={!(managedRoot && ready)}
					onSelect={() => {
						if (managedRoot && ready) void onOpen(managedRoot);
					}}
				>
					{t("games.openFolder")}
				</ContextMenuItem>
				<ContextMenuItem disabled={!paths} onSelect={() => paths && void onOpen(paths.library)}>
					{t("games.openLibrary")}
				</ContextMenuItem>
				<ContextMenuItem disabled={!paths} onSelect={() => paths && void onOpen(paths.archives)}>
					{t("games.openArchives")}
				</ContextMenuItem>
				<ContextMenuItem disabled={!paths?.profile} onSelect={() => paths?.profile && void onOpen(paths.profile)}>
					{t("games.openProfile")}
				</ContextMenuItem>
				<ContextMenuItem disabled={!paths?.overwrite} onSelect={() => paths?.overwrite && void onOpen(paths.overwrite)}>
					{t("games.openOverwrite")}
				</ContextMenuItem>
				<ContextMenuSeparator />
				<ContextMenuItem disabled={!managedRoot} onSelect={() => void onCopy(managedRoot)}>
					{t("games.copyPath")}
				</ContextMenuItem>
				<ContextMenuSeparator />
				<ContextMenuItem variant="destructive" onSelect={() => setConfirmRemove(true)}>
					{t("games.remove")}
				</ContextMenuItem>
			</ContextMenuContent>
			<ConfirmRemoveGame name={game.name} open={confirmRemove} onOpenChange={setConfirmRemove} onConfirm={() => void onRemove(game.id)} />
		</ContextMenu>
	);
}
