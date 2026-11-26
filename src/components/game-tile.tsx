import { cn } from "cn";
import { useState } from "react";
import type { GameView } from "@/bindings";
import { ConfirmRemoveGame } from "@/components/confirm-remove-game";
import { GameIcon } from "@/components/game-icon";
import { Button } from "@/components/ui/button";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuLabel,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { useTranslation } from "@/i18n";

/// A game tile: its icon, a right-click menu with folder actions, and removal with confirmation.
export function GameTile({
	game,
	managedRoot,
	ready,
	active,
	state,
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
	state: "running" | "draft" | null;
	paths: {
		library: string;
		archives: string;
		profile: string | null;
		overwrite: string | null;
	} | null;
	onSelect: () => void;
	onOpen: (path: string) => void | Promise<void>;
	onCopy: (path: string) => void | Promise<void>;
	onRemove: (id: string) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const [confirmRemove, setConfirmRemove] = useState(false);
	let label = game.name;

	if (state === "running") label = t("app.railRunning", { name: game.name });
	else if (state === "draft") label = t("app.railDraft", { name: game.name });

	return (
		<ContextMenu>
			<ContextMenuTrigger asChild>
				<Button
					variant="ghost"
					aria-label={label}
					aria-current={active ? "page" : undefined}
					title={label}
					className={cn("relative size-12", active && "bg-muted ring-foreground/60 ring-1")}
					onClick={onSelect}
				>
					<GameIcon id={game.id} name={game.name} className="size-7 rounded-md" />
					{/* The dot repeats what the label says: the game runs (green) or has changes waiting. */}
					{state && (
						<span
							aria-hidden
							className={cn(
								"ring-card absolute right-1.5 bottom-1.5 size-2.5 rounded-full ring-2",
								state === "running" ? "bg-primary" : "bg-draft",
							)}
						/>
					)}
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
