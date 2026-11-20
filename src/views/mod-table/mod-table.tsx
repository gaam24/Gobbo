import { closestCenter, DndContext, type DragEndEvent, PointerSensor, useSensor, useSensors } from "@dnd-kit/core";
import { horizontalListSortingStrategy, SortableContext, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { cn } from "cn";
import { Columns3, Eye, Lock, MoveHorizontal, RotateCcw } from "lucide-react";
import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuCheckboxItem,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useTranslation } from "@/i18n";
import {
	COLUMNS,
	type ColumnId,
	type ColumnLayout,
	type ColumnSpec,
	DEFAULT_LAYOUT,
	EXTRA_COLUMNS,
	isColumnId,
	moveColumnTo,
	orderedColumns,
	stepColumn,
} from "@/lib/mod-columns";
import { type Group, type TableItem, tableItems } from "@/lib/mods";
import { useModExtras } from "@/lib/use-mod-extras";
import { useScrollMargin } from "@/lib/use-scroll-margin";
import { useVirtualWindow } from "@/lib/use-virtual-window";
import { ColumnHeader } from "@/views/mod-table/column-header";
import {
	COLUMN_GROUPS,
	COLUMN_ICONS,
	FIT_MAX,
	fitting,
	neededWidth,
	ROW,
	rememberLayout,
	sampleRows,
	sessionLayout,
	trackList,
	trackVariable,
} from "@/views/mod-table/layout";
import type { ModListProps } from "@/views/mod-table/mod-list-props";
import { ModRow } from "@/views/mod-table/mod-row";
import { NoMatch } from "@/views/mod-table/no-match";
import { PageRow } from "@/views/mod-table/page-row";
import { SeparatorRow } from "@/views/mod-table/separator-row";

/// Mod list as a table: a row per mod, a header that is not sticky, and rows with a grip, a toggle and a name button.
/// The edge of each header cell resizes its column. The Columns menu shows and hides columns. The grip is for the mouse;
/// the arrow keys with Alt move a focused mod too. Only the rows around the viewport are mounted.
export function ModTable({
	gameId,
	groupMenu,
	scrollRef,
	rows,
	groups,
	collapsed,
	foldedPages,
	groupSizes,
	onTogglePage,
	onDeletePage,
	onSetEnabled,
	onRenameGroup,
	onUngroup,
	onSetTranslationWinner,
	selection,
	filtering,
	updates,
	libraryDir,
	archivesDir,
	locale,
	onPick,
	onToggle,
	onMove,
	onOpenFolder,
	onRevealFile,
	onOpenSource,
	onCheckUpdate,
	onUpdateMod,
	onOpenConflicts,
	onIgnoreVersion,
	onUnignoreVersion,
	onSetHidden,
	hidden: hiddenMods,
	readyProviders,
	onRemoveFromLibrary,
	onSelectVersion,
	onDeleteVersion,
	onReinstall,
	onArchiveReport,
	onChangeInstaller,
	onReorder,
	freshSeparator,
	firstModId,
	lastModId,
	onClearFilters,
	onShowDetails,
	onAddSeparatorAbove,
	onRenameSeparator,
	onRenameDone,
	onRemoveSeparator,
	onGroupEnabled,
	onSortGroup,
	onToggleGroup,
}: ModListProps) {
	const { t } = useTranslation();
	const [layout, setLayoutState] = useState(sessionLayout);
	const setLayout = (next: ColumnLayout) => {
		rememberLayout(next);
		setLayoutState(next);
	};
	const ordered = useMemo(() => orderedColumns(layout.order), [layout.order]);
	const chosen = useMemo(() => ordered.filter((column) => !layout.hidden.includes(column.id)), [ordered, layout.hidden]);
	/// The width the list has: columns that do not fit give way, so a narrow window needs no sideways scrolling.
	const scroller = useRef<HTMLDivElement>(null);
	const [room, setRoom] = useState<number | null>(null);
	useEffect(() => {
		const element = scroller.current;

		if (!element) return;
		const observer = new ResizeObserver(([entry]) => {
			if (entry) setRoom(Math.floor(entry.contentRect.width));
		});
		observer.observe(element);
		return () => observer.disconnect();
	}, []);
	const columns = useMemo(() => fitting(chosen, layout.widths, room), [chosen, layout.widths, room]);
	const squeezed = chosen.filter((column) => !columns.includes(column));
	const needed = neededWidth(columns, layout.widths);
	const table = useMemo(() => tableItems(rows, foldedPages, groups), [foldedPages, groups, rows]);
	const columnIds = useMemo(() => columns.map((column) => column.id), [columns]);
	const rowIds = useMemo(() => table.map((row) => row.id), [table]);
	const focusedIndex =
		selection.length === 0 ? 0 : table.findIndex((row) => row.kind === "mod" && row.id === selection[selection.length - 1]);
	/// Every row has one of two fixed heights, matching the classes in the row components.
	const heights = useMemo(() => table.map((row) => (row.kind === "separator" ? 40 : 56)), [table]);
	const [showAllRows, setShowAllRows] = useState(false);
	const listRef = useRef<HTMLDivElement>(null);
	const scrollMargin = useScrollMargin(scrollRef, listRef);
	const { items, totalSize, scrollToIndex } = useVirtualWindow({
		count: heights.length,
		getScrollElement: () => scrollRef?.current ?? null,
		estimateSize: (index) => heights[index] ?? 56,
		scrollMargin,
		overscan: 8,
		// Server rendering (and the first frame before the scroll element is measured) has no rect. Keep a window so
		// rows are present in the first paint instead of an empty list that pops in on the next frame.
		initialRect: { width: 0, height: 2240 },
	});
	// A column fit mounts the whole list for one frame; the virtualizer has no window while that happens.
	const virtualRows = showAllRows ? [] : items;
	const first = virtualRows.length > 0 ? (virtualRows[0]?.index ?? 0) : 0;
	const last = virtualRows.length > 0 ? (virtualRows[virtualRows.length - 1]?.index ?? 0) + 1 : Math.min(heights.length, 40);
	const total = totalSize;
	/// A column that is not in the light row needs the extras of the mounted rows. While every row is mounted for a
	/// column fit, the window is not a real viewport, so the extras wait for it to finish.
	const needsExtras = columns.some((column) => EXTRA_COLUMNS.has(column.id));
	const extrasFolders = useMemo(() => {
		if (!needsExtras || showAllRows) return [];
		const folders = new Set<string>();

		for (const row of table.slice(first, last)) {
			if (row.kind === "mod") folders.add(row.mod.folder);
			else if (row.kind === "page") for (const mod of row.page.mods) folders.add(mod.folder);
			else if (row.kind === "group") for (const mod of row.group.parts) folders.add(mod.folder);
		}

		return [...folders];
	}, [first, last, needsExtras, showAllRows, table]);
	const extras = useModExtras(gameId, rows, extrasFolders);
	/// A focused row reached from the palette may be far outside the window; put it in the middle and let the scroll
	/// handler mount it. A status refresh rebuilds the offsets but must not pull the list back to the focused row while
	/// the user reads another part of it.
	const lastFocused = useRef<number | null>(null);
	useEffect(() => {
		if (focusedIndex < 0 || lastFocused.current === focusedIndex) return;
		lastFocused.current = focusedIndex;
		scrollToIndex(focusedIndex, { align: "center" });
	}, [focusedIndex, scrollToIndex]);
	/// Home/End and the palette can select a row that the window mounts only after the scroll. Focus it after that
	/// commit, not in the key handler, where the row does not exist yet.
	useEffect(() => {
		const id = selection.at(-1);

		if (!id) return;
		const frame = requestAnimationFrame(() => {
			listRef.current?.querySelector<HTMLElement>(`[data-mod="${CSS.escape(id)}"]`)?.focus();
		});
		return () => cancelAnimationFrame(frame);
	}, [selection]);
	/// Only a list too narrow even for the columns that stay scrolls sideways; otherwise the header can stick on top.
	const overflowing = room !== null && needed > room;
	const template = trackList(columns, layout.widths);
	/// Position in the whole profile, by family: one pass instead of a `findIndex` per row.
	const selectionSet = new Set(selection);
	const focused = selection[selection.length - 1] ?? null;
	/// The mouse only: a drag starts after a few pixels, so a click on the grip still does nothing.
	const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }));

	/// The side of a drop is only the drag direction; the backend resolves page and user-group blocks from the two ids.
	const dropped = (event: DragEndEvent) => {
		const target = event.over?.id;

		if (target === undefined) return;
		const from = String(event.active.id);
		const fromAt = rows.findIndex((item) => item.id === from);
		const toAt = rows.findIndex((item) => item.id === target);

		if (fromAt < 0 || toAt < 0) return;
		onReorder(from, String(target), fromAt < toAt ? "below" : "above");
	};

	/// A header dropped on another header takes its place. Hidden columns keep theirs.
	const reorderColumns = (event: DragEndEvent) => {
		const dragged = event.active.id;
		const target = event.over?.id;

		if (isColumnId(dragged) && isColumnId(target) && dragged !== target) {
			setLayout({
				...layout,
				order: moveColumnTo(layout.order, dragged, target),
			});
		}
	};
	/// One step left or right from a header's menu, past the columns on screen only.
	const stepColumnBy = (id: ColumnId, step: -1 | 1) =>
		setLayout({
			...layout,
			order: stepColumn(
				layout.order,
				columns.map((column) => column.id),
				id,
				step,
			),
		});
	const showAllColumns = () => setLayout({ ...layout, hidden: [] });

	const resize = (column: ColumnSpec, width: number) => setLayout({ ...layout, widths: { ...layout.widths, [column.id]: width } });

	/// Fits columns to their widest cell, the header included. Each row is a grid of its own, so the columns are measured
	/// with a `max-content` track in every row, and the widest one wins. Mod keeps filling the space when it is not named.
	const grid = useRef<HTMLDivElement>(null);
	const fitColumnsNow = (ids: readonly ColumnId[]) => {
		const element = grid.current;
		// Fitting measures the rows on screen; with a filter or a folded group that is not the whole list, so the saved
		// width would truncate names as soon as the filter is cleared.
		if (!element || filtering) return;
		const sizing = Object.fromEntries(ids.map((id) => [id, "max-content"]));
		element.style.setProperty("--mod-columns", trackList(columns, layout.widths, sizing));
		// A row off the screen skips its layout (`content-visibility`), so its cells would measure as zero. Fitting is an
		// explicit action; lay out an even sample (at most FIT_SAMPLE) and let the browser skip the rest. Measuring every
		// row of a 10k list would freeze the window for one click.
		const rowElements = sampleRows([...element.querySelectorAll<HTMLElement>("[data-grid-row]")]);

		for (const row of rowElements) row.style.contentVisibility = "visible";
		// The header row is always there; without at least one mod row there is nothing to measure.
		if (rowElements.length <= 1) {
			element.style.setProperty("--mod-columns", template);

			for (const row of rowElements) row.style.removeProperty("content-visibility");
			return;
		}

		const widths = { ...layout.widths };

		for (const id of ids) {
			const at = columns.findIndex((column) => column.id === id);
			const column = columns[at];

			if (!column) continue;
			// The grip is the first child of every row.
			const widest = Math.max(0, ...rowElements.map((row) => row.children[at + 1]?.getBoundingClientRect().width ?? 0));
			widths[id] = Math.min(FIT_MAX, Math.max(column.min, Math.ceil(widest) + 2));
		}

		for (const row of rowElements) row.style.removeProperty("content-visibility");
		element.style.setProperty("--mod-columns", template);
		setLayout({ ...layout, widths });
	};
	const fitColumns = (ids: readonly ColumnId[]) => {
		// Fitting looks at every row of the whole list, so the reserved rows are mounted for the measurement and the
		// window returns once the widths are read.
		if (last - first < table.length) {
			setShowAllRows(true);
			requestAnimationFrame(() => {
				fitColumnsNow(ids);
				setShowAllRows(false);
			});
			return;
		}

		fitColumnsNow(ids);
	};
	const fitAllColumns = () => fitColumns(columns.filter((column) => column.id !== "mod").map((column) => column.id));

	const toggleColumn = (id: ColumnId) =>
		setLayout({
			...layout,
			hidden: layout.hidden.includes(id) ? layout.hidden.filter((hidden) => hidden !== id) : [...layout.hidden, id],
		});

	const columnsMenu = (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="ghost" size="icon-sm" aria-label={t("mods.columns")} title={t("mods.columns")}>
					<Columns3 />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent className="max-h-[70vh] w-64 overflow-y-auto" align="start">
				<DropdownMenuLabel>{t("mods.columns")}</DropdownMenuLabel>
				<p className="text-subtle-foreground px-2 pb-1 text-xs">{t("mods.columnsHint")}</p>
				{squeezed.length > 0 && (
					<p className="text-muted-foreground px-2 pb-1 text-xs">
						{t("mods.columnsSqueezed", {
							list: squeezed.map((column) => t(`mods.col.${column.id}`)).join(", "),
						})}
					</p>
				)}
				{COLUMN_GROUPS.map((group, index) => (
					<Fragment key={group}>
						{index > 0 && <DropdownMenuSeparator />}
						<DropdownMenuLabel className="text-subtle-foreground text-xs tracking-wider uppercase">
							{t(`mods.colGroup.${group}`)}
						</DropdownMenuLabel>
						{COLUMNS.filter((column) => column.group === group).map((column) => {
							const Icon = COLUMN_ICONS[column.id];
							return (
								<DropdownMenuCheckboxItem
									key={column.id}
									checked={column.pinned === true || !layout.hidden.includes(column.id)}
									disabled={column.pinned === true}
									onCheckedChange={() => toggleColumn(column.id)}
									// The menu stays open, so several columns can be switched in one go.
									onSelect={(event) => event.preventDefault()}
								>
									<Icon aria-hidden />
									{t(`mods.col.${column.id}`)}
									{column.pinned && <Lock aria-label={t("mods.columnPinned")} className="text-subtle-foreground ml-auto" />}
								</DropdownMenuCheckboxItem>
							);
						})}
					</Fragment>
				))}
				<DropdownMenuSeparator />
				<DropdownMenuLabel>{t("mods.columnsMenu.layout")}</DropdownMenuLabel>
				<DropdownMenuItem onSelect={showAllColumns}>
					<Eye /> {t("mods.columnsShowAll")}
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={fitAllColumns}>
					<MoveHorizontal /> {t("mods.columnsFit")}
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={() => setLayout(DEFAULT_LAYOUT)}>
					<RotateCcw /> {t("mods.columnsReset")}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);

	/// A selected row carries the Tab stop; when it is filtered or scrolled away, the first mounted mod row does.
	const focusedVisible = focused !== null && table.some((row) => row.kind === "mod" && row.id === focused);
	const firstModRow = table.find((row) => row.kind === "mod")?.id;
	const renderRow = (row: TableItem, index: number) => {
		if (row.kind === "separator") {
			return (
				<SeparatorRow
					key={row.id}
					id={row.id}
					position={index + 1}
					setSize={table.length}
					name={row.name}
					count={groupSizes.get(row.id) ?? 0}
					collapsed={collapsed.has(row.id)}
					fresh={freshSeparator === row.id}
					onRenameSeparator={onRenameSeparator}
					onRenameDone={onRenameDone}
					onRemoveSeparator={onRemoveSeparator}
					onGroupEnabled={onGroupEnabled}
					onSortGroup={onSortGroup}
					onToggleGroup={onToggleGroup}
				/>
			);
		}

		if (row.kind === "page" || row.kind === "group") {
			// `tableItems` keeps the page details on a page row, so wrap them in the same `Group` a user row already is.
			const header: Group =
				row.kind === "page"
					? {
							source: "page",
							id: row.id,
							name: row.page.name,
							parts: row.page.mods,
							page: row.page,
						}
					: row.group;
			const storedId = row.kind === "group" ? (row.group.group?.id ?? row.id) : null;
			const source = header.parts.find((mod) => mod.sources.length > 0)?.sources[0];
			return (
				<PageRow
					key={row.id}
					group={header}
					position={index + 1}
					setSize={table.length}
					columns={columns}
					folded={foldedPages.has(row.id)}
					filtering={filtering}
					updates={updates}
					extras={extras}
					locale={locale}
					onToggle={(enabled) => void onSetEnabled(row.id, enabled)}
					onFold={() => onTogglePage(row.id)}
					onOpenSource={() => {
						if (source) onOpenSource(source.provider, source.mod_id, "files");
					}}
					onDeletePage={() => {
						if (row.kind === "page") onDeletePage(row.page.pageId);
					}}
					onRenameGroup={() => {
						if (storedId !== null) onRenameGroup(storedId);
					}}
					onUngroup={() => {
						if (storedId !== null) onUngroup(storedId);
					}}
					onSetTranslationWinner={() => {
						if (storedId !== null) onSetTranslationWinner(storedId);
					}}
				/>
			);
		}

		return (
			<ModRow
				key={row.id}
				gameId={gameId}
				groupMenu={groupMenu}
				mod={row.mod}
				columns={columns}
				position={index + 1}
				setSize={table.length}
				picked={selectionSet.has(row.id)}
				focused={focused === row.id}
				tabbable={focused === row.id || (!focusedVisible && row.id === firstModRow)}
				filtering={filtering}
				isFirst={row.id === firstModId}
				isLast={row.id === lastModId}
				nested={row.nested === true}
				hidden={hiddenMods.has(row.id)}
				update={updates[row.id]}
				extras={extras.get(row.mod.folder)}
				libraryDir={libraryDir}
				archivesDir={archivesDir}
				locale={locale}
				onPick={onPick}
				onToggle={onToggle}
				onMove={onMove}
				onOpenFolder={onOpenFolder}
				onRevealFile={onRevealFile}
				onOpenSource={onOpenSource}
				onCheckUpdate={onCheckUpdate}
				onUpdateMod={onUpdateMod}
				onOpenConflicts={onOpenConflicts}
				onIgnoreVersion={onIgnoreVersion}
				onUnignoreVersion={onUnignoreVersion}
				onSetHidden={onSetHidden}
				readyProviders={readyProviders}
				onRemoveFromLibrary={onRemoveFromLibrary}
				onSelectVersion={onSelectVersion}
				onDeleteVersion={onDeleteVersion}
				onReinstall={onReinstall}
				onArchiveReport={onArchiveReport}
				onChangeInstaller={onChangeInstaller}
				onAddSeparatorAbove={onAddSeparatorAbove}
				onShowDetails={onShowDetails}
			/>
		);
	};

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col">
			<div ref={scroller} className={cn("min-w-0 flex-1", overflowing && "overflow-x-auto")}>
				{/* biome-ignore lint/a11y/useSemanticElements: the virtual list and the grid layout need div rows. */}
				<div
					ref={grid}
					role="table"
					aria-label={t("mods.table")}
					aria-rowcount={table.length + 1}
					aria-colcount={columns.length + 1}
					className="flex min-h-full flex-col"
					style={{ ...trackVariable(template), minWidth: needed }}
				>
					<DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={reorderColumns}>
						<SortableContext items={columnIds} strategy={horizontalListSortingStrategy}>
							{/* biome-ignore lint/a11y/useSemanticElements: the virtual list and the grid layout need div rows. */}
							<div
								data-grid-row
								role="row"
								tabIndex={-1}
								aria-rowindex={1}
								className={cn(ROW, "text-subtle-foreground bg-background sticky top-0 z-10 h-10 items-stretch border-b text-xs")}
							>
								{/* biome-ignore lint/a11y/useSemanticElements: the virtual list and the grid layout need div rows. */}
								<span role="columnheader" tabIndex={-1} className="flex items-center">
									{columnsMenu}
								</span>
								{columns.map((column, index) => (
									<ColumnHeader
										key={column.id}
										column={column}
										width={layout.widths[column.id]}
										first={index === 0}
										last={index === columns.length - 1}
										onResize={(width) => resize(column, width)}
										onFit={() => fitColumns([column.id])}
										onFitAll={fitAllColumns}
										onHide={column.pinned ? undefined : () => toggleColumn(column.id)}
										onStep={(step) => stepColumnBy(column.id, step)}
										onReset={() => setLayout(DEFAULT_LAYOUT)}
									/>
								))}
							</div>
						</SortableContext>
					</DndContext>
					{rows.length === 0 && <NoMatch onClearFilters={onClearFilters} />}
					<DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={dropped}>
						<SortableContext items={rowIds} strategy={verticalListSortingStrategy}>
							{/* biome-ignore lint/a11y/useSemanticElements: the virtual list and the grid layout need div rows. */}
							<div ref={listRef} role="rowgroup" style={{ height: total, position: "relative" }}>
								{showAllRows
									? table.map((row, index) => renderRow(row, index))
									: virtualRows.map((item) => {
											const row = table[item.index];

											if (!row) return null;
											return (
												<div
													key={item.key}
													role="presentation"
													style={{
														position: "absolute",
														top: 0,
														left: 0,
														width: "100%",
														height: item.size,
														transform: `translateY(${item.start - scrollMargin}px)`,
													}}
												>
													{renderRow(row, item.index)}
												</div>
											);
										})}
							</div>
						</SortableContext>
					</DndContext>
				</div>
			</div>
		</div>
	);
}
