import { cn } from "cn";
import { EllipsisVertical, FileUp, Plus, RefreshCw, Search, SearchCheck } from "lucide-react";
import { Suspense, useEffect, useMemo, useState } from "react";
import { useUiActions } from "@/app/contexts/ui-context";
import { CatalogDialog } from "@/app/lazy-views";
import type { DefinitionProblem_Serialize, GameScan, GameView, ManagedGame } from "@/bindings";
import { FailureAlert } from "@/components/app/failure-alert";
import { ListGroupHeader, PageEmpty } from "@/components/app/page-layout";
import { ConfirmRemoveGame } from "@/components/confirm-remove-game";
import { PageFrame } from "@/components/page-frame";
import { Alert, AlertAction, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { useTranslation } from "@/i18n";
import { ARCHIVE_EXTENSIONS } from "@/lib/archives";
import { errorText } from "@/lib/ipc";
import { formatTime } from "@/lib/mods";
import { pickFiles } from "@/lib/tauri";
import { buildGameEntries, filterGameEntries, GAME_ROW_GRID, GAME_STATUSES } from "@/views/games/entries";
import { GameRow } from "@/views/games/game-row";
import { LocateDialog } from "@/views/games/locate-dialog";
import { hasCatalogUpdates, useCatalog } from "@/views/games/use-catalog";

/// Set once the silent launch check ran, so reopening the page does not ask GitHub again until the app restarts.
let catalogCheckedThisLaunch = false;

/// Every supported game, in three groups by status: added (the only ones that can be modded), found on this computer,
/// and not found. The search narrows all groups; the status groups replace a separate filter.
export function GamesPanel({
	games,
	managed,
	scans,
	scanError,
	definitionProblems,
	catalogError,
	checkCatalogOnLaunch,
	onReload,
	scannedAt,
	scanning,
	onScan,
	onAdd,
	onCheck,
	onSelect,
	onRemove,
	onOpen,
	onCopy,
	onCreate,
	onEdit,
}: {
	games: GameView[];
	managed: ManagedGame[];
	scans: GameScan[] | null;
	/// The failure of the last scan, shown with a retry button.
	scanError: string | null;
	/// User definitions that did not load; the other games still work.
	definitionProblems: DefinitionProblem_Serialize[];
	/// The failure of the game catalog, shown with a retry that reads it again.
	catalogError: string | null;
	/// Whether the official catalog is checked the first time this page opens after the app starts.
	checkCatalogOnLaunch: boolean;
	/// Re-reads the definitions and the added games, also used when a catalog install finishes.
	onReload: () => void | Promise<void>;
	/// The scan is started by the page, not awaited; the button only reports its state.
	onScan: () => void;
	/// When `scans` was produced, for the "Searched at" line.
	scannedAt: Date | null;
	/// A scan is running; the button waits for it.
	scanning: boolean;
	/// Resolves to `true` when the game was added, so a dialog can close itself.
	onAdd: (id: string, gameRoot: string) => Promise<boolean>;
	/// Checks a folder without adding the game. Resolves to the marker file found, or `null` when the game names none.
	onCheck: (id: string, gameRoot: string) => Promise<string | null>;
	/// Opens an added game on its mod list, the same as its tile in the rail.
	onSelect: (id: string) => void;
	onRemove: (id: string) => void | Promise<void>;
	onOpen: (gameId: string, path: string) => void | Promise<void>;
	onCopy: (path: string) => void | Promise<void>;
	/// Opens the editor for a definition the user writes or changes.
	onCreate: () => void;
	onEdit: (id: string) => void;
}) {
	const { t, i18n } = useTranslation();
	const { openDialog } = useUiActions();
	const [query, setQuery] = useState("");
	const [confirmRemove, setConfirmRemove] = useState<{
		id: string;
		name: string;
	} | null>(null);
	const [locating, setLocating] = useState<{ id: string; name: string } | null>(null);
	const [catalogOpen, setCatalogOpen] = useState(false);
	const { status: catalogStatus, checking, progress, check, install } = useCatalog({ onInstalled: onReload });

	// Opening the page reads the definitions again, so a file the user just added works without a restart.
	useEffect(() => {
		void onReload();
		onScan();
	}, [onReload, onScan]);

	// One silent catalog check per app launch, the first time this page opens. A manual check works either way.
	useEffect(() => {
		if (!checkCatalogOnLaunch || catalogCheckedThisLaunch) return;
		catalogCheckedThisLaunch = true;
		void check(true);
	}, [check, checkCatalogOnLaunch]);

	const entries = useMemo(() => buildGameEntries(games, managed, scans, scanError !== null), [games, managed, scans, scanError]);
	const visible = filterGameEntries(entries, "all", query);
	const groups = GAME_STATUSES.map((status) => ({
		status,
		rows: visible.filter((entry) => entry.status === status).toSorted((a, b) => a.game.name.localeCompare(b.game.name, i18n.language)),
	})).filter((group) => group.rows.length > 0);

	const scanTime = scannedAt ? formatTime(scannedAt.getTime(), i18n.language) : null;

	const importDefinition = async () => {
		const [path] = await pickFiles(t("settings.game.exportFilter"), ARCHIVE_EXTENSIONS, false);

		if (path) openDialog({ kind: "definition-import", path });
	};

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col">
			<PageFrame
				title={t("games.title")}
				hint={t("games.hint")}
				meta={scanTime ? t("games.searchedAt", { time: scanTime }) : null}
				action={
					<>
						<Button variant="outline" onClick={onScan} disabled={scanning}>
							<RefreshCw /> {t("games.rescan")}
						</Button>
						<Button variant="outline" onClick={onCreate}>
							<Plus /> {t("games.addOwn")}
						</Button>
						{/* Importing a definition and checking the catalog are rarer, so they wait in the menu. */}
						<DropdownMenu>
							<DropdownMenuTrigger asChild>
								<Button variant="ghost" size="icon" aria-label={t("mods.more")} title={t("mods.more")}>
									<EllipsisVertical aria-hidden />
								</Button>
							</DropdownMenuTrigger>
							<DropdownMenuContent align="end" className="w-64">
								<DropdownMenuItem onSelect={() => void importDefinition()}>
									<FileUp /> {t("games.importDefinition")}
								</DropdownMenuItem>
								<DropdownMenuSeparator />
								<DropdownMenuItem disabled={checking} onSelect={() => void check(false)}>
									<SearchCheck /> {t("catalog.checkNow")}
								</DropdownMenuItem>
							</DropdownMenuContent>
						</DropdownMenu>
					</>
				}
				toolbar={
					<InputGroup className="w-72">
						<InputGroupInput
							type="search"
							aria-label={t("games.search")}
							placeholder={t("games.search")}
							value={query}
							spellCheck={false}
							onChange={(event) => setQuery(event.currentTarget.value)}
						/>
						<InputGroupAddon>
							<Search aria-hidden />
						</InputGroupAddon>
					</InputGroup>
				}
			>
				{(catalogError !== null || definitionProblems.length > 0 || scanError !== null) && (
					<div className="grid gap-3 px-6 pt-4">
						{catalogError && (
							<FailureAlert title={t("games.catalogFailed")} onRetry={() => void onReload()}>
								{catalogError}
							</FailureAlert>
						)}

						{definitionProblems.length > 0 && (
							<FailureAlert title={t("games.definitionProblems")} onRetry={() => void onReload()}>
								<ul className="grid gap-1">
									{definitionProblems.map((problem) => (
										<li key={problem.path}>
											<span className="font-mono text-xs break-all">{problem.path}</span>
											<span className="block">{errorText(problem.error)}</span>
										</li>
									))}
								</ul>
							</FailureAlert>
						)}

						{scanError && (
							<FailureAlert title={t("games.scanFailed")} onRetry={onScan} retrying={scanning}>
								{scanError}
							</FailureAlert>
						)}
					</div>
				)}

				{hasCatalogUpdates(catalogStatus) && (
					<div className="px-6 pt-4">
						<Alert>
							<AlertTitle>
								{t("catalog.banner", {
									changed: catalogStatus?.changed.length ?? 0,
									available: catalogStatus?.available.length ?? 0,
								})}
							</AlertTitle>
							<AlertAction>
								<Button variant="outline" size="sm" aria-label={t("catalog.open")} onClick={() => setCatalogOpen(true)}>
									{t("catalog.open")}
								</Button>
							</AlertAction>
						</Alert>
					</div>
				)}

				<div className="min-w-0 overflow-x-auto">
					<div className="min-w-220">
						<div aria-hidden className={cn(GAME_ROW_GRID, "text-muted-foreground h-9 border-b text-xs")}>
							<span />
							<span>{t("games.column.game")}</span>
							<span>{t("games.column.folder")}</span>
							<span />
						</div>

						{groups.map((group) => (
							<section key={group.status} aria-label={t(`games.status.${group.status}`)}>
								<ListGroupHeader title={t(`games.status.${group.status}`)} count={group.rows.length} />
								<ul>
									{group.rows.map((entry) => (
										<GameRow
											key={entry.game.id}
											entry={entry}
											onAdd={onAdd}
											onSelect={onSelect}
											onOpen={onOpen}
											onCopy={onCopy}
											onRemove={(id, name) => setConfirmRemove({ id, name })}
											onLocate={(id, name) => setLocating({ id, name })}
											onEdit={onEdit}
										/>
									))}
								</ul>
							</section>
						))}

						{groups.length === 0 && <PageEmpty icon={Search} title={t("games.noResults")} />}
					</div>
				</div>

				{visible.length !== entries.length && (
					<footer className="text-muted-foreground border-t px-6 py-2 text-xs">
						{t("games.showing", {
							shown: visible.length,
							total: entries.length,
						})}
					</footer>
				)}
			</PageFrame>

			{locating && (
				<LocateDialog
					key={locating.id}
					id={locating.id}
					name={locating.name}
					onAdd={onAdd}
					onCheck={onCheck}
					onClose={() => setLocating(null)}
				/>
			)}

			<ConfirmRemoveGame
				name={confirmRemove?.name ?? ""}
				open={confirmRemove !== null}
				onOpenChange={(open) => !open && setConfirmRemove(null)}
				onConfirm={() => {
					if (confirmRemove) void onRemove(confirmRemove.id);
				}}
			/>

			{catalogOpen && (
				<Suspense fallback={null}>
					<CatalogDialog status={catalogStatus} progress={progress} onInstall={install} onClose={() => setCatalogOpen(false)} />
				</Suspense>
			)}
		</div>
	);
}
