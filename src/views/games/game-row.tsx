import { cn } from "cn";
import { Copy, EllipsisVertical, FolderOpen, Pencil, Trash2 } from "lucide-react";
import type { ReactNode } from "react";
import { PathText } from "@/components/app/path-text";
import { StatusBadge } from "@/components/app/status-badge";
import { GameIcon } from "@/components/game-icon";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuSeparator, ContextMenuTrigger } from "@/components/ui/context-menu";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useTranslation } from "@/i18n";
import { verifiedHint } from "@/lib/game-definition";
import { GAME_ROW_GRID, type GameEntry } from "@/views/games/entries";

/// One game. The group it sits in already names its status, so the row shows only what the group does not: a missing
/// folder, the folder itself, and the action. The one frequent action stays a button (Add, Locate); the rest sits in
/// the row menu, which the right-click menu mirrors. Added and found games have folder actions.
export function GameRow({
	entry,
	onAdd,
	onSelect,
	onOpen,
	onCopy,
	onRemove,
	onLocate,
	onEdit,
}: {
	entry: GameEntry;
	onAdd: (id: string, gameRoot: string) => Promise<boolean>;
	onSelect: (id: string) => void;
	onOpen: (gameId: string, path: string) => void | Promise<void>;
	onCopy: (path: string) => void | Promise<void>;
	onRemove: (id: string, name: string) => void;
	onLocate: (id: string, name: string) => void;
	onEdit: (id: string) => void;
}) {
	const { t } = useTranslation();
	const { game, status, root, ready } = entry;

	let action: ReactNode = null;

	if (status === "found") {
		action = (
			<Button variant="outline" size="sm" className="border-primary text-primary" onClick={() => root && void onAdd(game.id, root)}>
				{t("games.action.add")}
			</Button>
		);
	} else if (status === "missing") {
		action = (
			<Button variant="outline" size="sm" onClick={() => onLocate(game.id, game.name)}>
				{t("games.action.locate")}
			</Button>
		);
	}

	const hasMenu = root !== null || status === "added" || game.origin === "user";

	const row = (
		<li className={cn(GAME_ROW_GRID, "min-h-14 border-b py-2")}>
			<GameIcon id={game.id} name={game.name} className="size-8 rounded-md" />
			<div className="min-w-0">
				<span className="flex min-w-0 items-center gap-2">
					{status === "added" ? (
						<Button
							variant="link"
							className="text-foreground h-auto min-w-0 justify-start p-0 font-medium"
							title={t("games.openGame", { name: game.name })}
							onClick={() => onSelect(game.id)}
						>
							<span className="truncate">{game.name}</span>
						</Button>
					) : (
						<span className="truncate text-sm font-medium">{game.name}</span>
					)}
					{game.origin === "user" && (
						<Badge variant="outline" className="shrink-0">
							{t("games.userDefinition")}
						</Badge>
					)}
					{game.origin === "official" && (
						<Badge variant="outline" className="shrink-0">
							{t("games.catalogOrigin")}
						</Badge>
					)}
					{game.verified ? (
						<span className="shrink-0" title={verifiedHint(t, game.verified)}>
							<StatusBadge tone="success">{t("games.verified")}</StatusBadge>
						</span>
					) : (
						game.origin === "user" && (
							<Badge variant="outline" className="shrink-0">
								{t("games.unverified")}
							</Badge>
						)
					)}
					{status === "added" && !ready && (
						<StatusBadge tone="danger" className="shrink-0">
							{t("games.noFolder")}
						</StatusBadge>
					)}
				</span>
				<span className="text-subtle-foreground block truncate font-mono text-xs">{game.id}</span>
			</div>
			{root ? (
				<PathText path={root} className="text-muted-foreground" />
			) : (
				<span className="text-muted-foreground truncate font-mono text-xs">
					{t(status === "added" ? "games.rootUnset" : "games.notInSteam")}
				</span>
			)}
			<div className="flex items-center justify-end gap-1">
				{action}
				{hasMenu && (
					<DropdownMenu>
						<DropdownMenuTrigger asChild>
							<Button variant="ghost" size="icon-sm" aria-label={t("mods.more")} title={t("mods.more")}>
								<EllipsisVertical aria-hidden />
							</Button>
						</DropdownMenuTrigger>
						<DropdownMenuContent align="end" className="w-64">
							{root && (
								<DropdownMenuItem
									disabled={status !== "added" || !ready}
									onSelect={() => {
										if (root && status === "added" && ready) void onOpen(game.id, root);
									}}
								>
									<FolderOpen /> {t("games.openFolder")}
								</DropdownMenuItem>
							)}
							{root && (
								<DropdownMenuItem onSelect={() => void onCopy(root)}>
									<Copy /> {t("games.copyPath")}
								</DropdownMenuItem>
							)}
							{game.origin === "user" && (
								<DropdownMenuItem onSelect={() => onEdit(game.id)}>
									<Pencil /> {t("games.action.edit")}
								</DropdownMenuItem>
							)}
							{status === "added" && (
								<>
									<DropdownMenuSeparator />
									<DropdownMenuItem variant="destructive" onSelect={() => onRemove(game.id, game.name)}>
										<Trash2 /> {t("games.action.remove")}
									</DropdownMenuItem>
								</>
							)}
						</DropdownMenuContent>
					</DropdownMenu>
				)}
			</div>
		</li>
	);

	if (!root) return row;

	return (
		<ContextMenu>
			<ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
			<ContextMenuContent>
				<ContextMenuItem
					disabled={status !== "added" || !ready}
					onSelect={() => {
						if (root && status === "added" && ready) void onOpen(game.id, root);
					}}
				>
					<FolderOpen /> {t("games.openFolder")}
				</ContextMenuItem>
				<ContextMenuItem onSelect={() => void onCopy(root)}>
					<Copy /> {t("games.copyPath")}
				</ContextMenuItem>
				{game.origin === "user" && (
					<ContextMenuItem onSelect={() => onEdit(game.id)}>
						<Pencil /> {t("games.action.edit")}
					</ContextMenuItem>
				)}
				{status === "added" && (
					<>
						<ContextMenuSeparator />
						<ContextMenuItem variant="destructive" onSelect={() => onRemove(game.id, game.name)}>
							{t("games.action.remove")}
						</ContextMenuItem>
					</>
				)}
			</ContextMenuContent>
		</ContextMenu>
	);
}
