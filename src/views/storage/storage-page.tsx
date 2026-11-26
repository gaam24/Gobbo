import { EllipsisVertical, FolderOpen, HardDrive, RefreshCw, Trash2, Undo2 } from "lucide-react";
import { useState } from "react";
import type { GameDataUsage, GameView } from "@/bindings";
import { ListGroupHeader, ListRow, PageEmpty } from "@/components/app/page-layout";
import { ConfirmDialog } from "@/components/confirm-dialog";
import { GameIcon } from "@/components/game-icon";
import { PageFrame } from "@/components/page-frame";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { ItemActions, ItemContent, ItemMedia, ItemTitle } from "@/components/ui/item";
import { Progress } from "@/components/ui/progress";
import { Spinner } from "@/components/ui/spinner";
import { useTranslation } from "@/i18n";
import { formatSize } from "@/lib/mods";

/// The game data folders in Gobbo: active games and the ones removed from the list. The size is read when the page
/// opens; a removed folder stays here until the user restores it or deletes its data.
export function StorageView({
	games,
	usage,
	loading,
	failed,
	locale,
	onRefresh,
	onRestore,
	onDelete,
	onOpen,
}: {
	games: GameView[];
	usage: GameDataUsage[] | null;
	loading: boolean;
	failed: boolean;
	locale: string;
	onRefresh: () => Promise<boolean>;
	onRestore: (id: string) => Promise<boolean>;
	onDelete: (id: string) => Promise<boolean>;
	onOpen: (gameId: string, path: string) => void | Promise<void>;
}) {
	const { t } = useTranslation();
	const [confirmDelete, setConfirmDelete] = useState<{
		id: string;
		name: string;
	} | null>(null);

	const names = new Map(games.map((game) => [game.id, game.name]));
	const active = usage?.filter((entry) => !entry.removed) ?? [];
	const removed = usage?.filter((entry) => entry.removed) ?? [];
	const total = usage?.reduce((sum, entry) => sum + entry.size_kb, 0) ?? 0;
	// The two groups get headings only when both kinds exist; a single kind is a plain list.
	const grouped = active.length > 0 && removed.length > 0;

	const renderRow = (entry: GameDataUsage) => {
		const name = names.get(entry.id) ?? entry.id;
		return (
			<ListRow asChild key={`${entry.id}:${entry.removed ? "removed" : "active"}`}>
				<li>
					<ItemMedia>
						<GameIcon id={entry.id} name={name} className="size-10 rounded-md" />
					</ItemMedia>
					<ItemContent className="min-w-0">
						<ItemTitle className="w-full">
							<span className="truncate">{name}</span>
						</ItemTitle>
						{/* Every game's folder sits under the same data folder, so the path repeated the same prefix on each row. The
						    share of the total says which game to look at; the path is on the folder button. */}
						<Progress
							value={total > 0 ? (entry.size_kb / total) * 100 : 0}
							aria-label={t("storage.share", { name })}
							className="mt-1 h-1.5 max-w-80"
						/>
					</ItemContent>
					<span className="text-muted-foreground shrink-0 font-mono text-xs tabular-nums">{formatSize(entry.size_kb, locale)}</span>
					<ItemActions className="shrink-0">
						<Button
							variant="ghost"
							size="icon-sm"
							aria-label={t("storage.open")}
							title={t("storage.open")}
							onClick={() => void onOpen(entry.id, entry.path)}
						>
							<FolderOpen aria-hidden />
						</Button>
						{entry.removed && (
							<>
								<Button variant="outline" size="sm" onClick={() => void onRestore(entry.id)}>
									<Undo2 /> {t("storage.restore")}
								</Button>
								<DropdownMenu>
									<DropdownMenuTrigger asChild>
										<Button variant="ghost" size="icon-sm" aria-label={t("mods.more")} title={t("mods.more")}>
											<EllipsisVertical aria-hidden />
										</Button>
									</DropdownMenuTrigger>
									<DropdownMenuContent align="end" className="w-56">
										<DropdownMenuItem variant="destructive" onSelect={() => setConfirmDelete({ id: entry.id, name })}>
											<Trash2 /> {t("storage.delete")}
										</DropdownMenuItem>
									</DropdownMenuContent>
								</DropdownMenu>
							</>
						)}
					</ItemActions>
				</li>
			</ListRow>
		);
	};

	const renderGroup = (title: string, rows: GameDataUsage[]) => (
		<section aria-label={title}>
			{grouped && <ListGroupHeader title={title} count={rows.length} />}
			<ul>{rows.map((entry) => renderRow(entry))}</ul>
		</section>
	);

	return (
		<PageFrame
			title={t("storage.title")}
			hint={t("storage.hint")}
			meta={usage ? t("storage.total", { size: formatSize(total, locale) }) : null}
			action={
				<Button variant="outline" size="sm" onClick={() => void onRefresh()} disabled={loading}>
					{loading ? <Spinner aria-hidden /> : <RefreshCw />}
					{t("storage.refresh")}
				</Button>
			}
		>
			{usage === null && loading && (
				<div className="text-muted-foreground flex items-center justify-center gap-2 py-12 text-sm">
					<Spinner aria-hidden />
					{t("storage.calculating")}
				</div>
			)}

			{usage === null && !loading && failed && (
				<PageEmpty
					icon={HardDrive}
					title={t("storage.loadFailed")}
					action={
						<Button variant="outline" size="sm" onClick={() => void onRefresh()}>
							<RefreshCw /> {t("storage.refresh")}
						</Button>
					}
				/>
			)}

			{usage?.length === 0 && <PageEmpty icon={HardDrive} title={t("storage.empty")} description={t("storage.emptyHint")} />}

			{usage && usage.length > 0 && (
				<>
					{active.length > 0 && renderGroup(t("storage.activeBadge"), active)}
					{removed.length > 0 && renderGroup(t("storage.removedBadge"), removed)}
				</>
			)}

			<ConfirmDialog
				open={confirmDelete !== null}
				onOpenChange={(open) => !open && setConfirmDelete(null)}
				title={t("storage.deleteTitle", { name: confirmDelete?.name ?? "" })}
				description={t("storage.deleteBody", {
					name: confirmDelete?.name ?? "",
				})}
				confirmLabel={t("storage.delete")}
				onConfirm={() => {
					const entry = confirmDelete;
					setConfirmDelete(null);

					if (entry) void onDelete(entry.id);
				}}
			/>
		</PageFrame>
	);
}
