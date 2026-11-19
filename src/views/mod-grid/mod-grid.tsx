import { closestCenter, DndContext, type DragEndEvent, PointerSensor, useSensor, useSensors } from "@dnd-kit/core";
import { rectSortingStrategy, SortableContext } from "@dnd-kit/sortable";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { type GridItem, type Group, gridItems, tableItems } from "@/lib/mods";
import { useScrollMargin } from "@/lib/use-scroll-margin";
import { useVirtualWindow } from "@/lib/use-virtual-window";
import { GridPageHeader } from "@/views/mod-grid/grid-page-header";
import { ModCard } from "@/views/mod-grid/mod-card";
import type { ModListProps } from "@/views/mod-table/mod-list-props";
import { NoMatch } from "@/views/mod-table/no-match";
import { SeparatorRow } from "@/views/mod-table/separator-row";

/// Cards per row come from the same `auto-fill` rule as the old CSS grid, with the gap between them measured here so
/// the virtual layout can place every card.
const PADDING_X = 24;
const PADDING_Y = 16;
const GAP = 12;
const MIN_CARD = 192;
const SEPARATOR = 40;
/// The compact page header is one fixed row; its `h-10` class must match.
const PAGE_HEADER = 40;
const INITIAL_WIDTH = 800;

/// The height a grid unit reserves; the classes of the separator and the page header must match these numbers.
function unitHeight(unit: GridItem, cardHeight: number): number {
	if (unit.kind === "separator") return SEPARATOR;
	if (unit.kind === "page" || unit.kind === "group") return PAGE_HEADER;
	return cardHeight;
}

/// The mod list as a grid of cards. Only the rows around the viewport are mounted; the rest keep their space, so the
/// scrollbar and keyboard jumps still reach every card. Separators take a full row, as they do in the normal grid.
export function ModGrid({
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
	onReorder,
	freshSeparator,
	firstModId,
	lastModId,
	onClearFilters,
	onPick,
	onToggle,
	onMove,
	onOpenFolder,
	onRevealFile,
	onOpenSource,
	onCheckUpdate,
	onUpdateMod,
	onIgnoreVersion,
	onUnignoreVersion,
	onSetHidden,
	hidden,
	readyProviders,
	onRemoveFromLibrary,
	onDeleteVersion,
	onReinstall,
	onArchiveReport,
	onChangeInstaller,
	onAddSeparatorAbove,
	onShowDetails,
	onRenameSeparator,
	onRenameDone,
	onRemoveSeparator,
	onGroupEnabled,
	onSortGroup,
	onToggleGroup,
}: ModListProps) {
	/// The mouse only, as in the table: a drag starts after a few pixels, so a click still picks.
	const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 4 } }));
	const selectionSet = useMemo(() => new Set(selection), [selection]);
	const positions = useMemo(() => new Map(rows.map((mod, index) => [mod.id, index + 1])), [rows]);
	const focused = selection[selection.length - 1] ?? null;
	const grid = useRef<HTMLOListElement>(null);
	const [width, setWidth] = useState(0);
	const [measured, setMeasured] = useState<{ width: number; height: number } | null>(null);

	useLayoutEffect(() => {
		const element = grid.current;
		if (!element) return;
		// The card width changes the list's height on every resize. Observing the list itself would report that height
		// as a resize and could echo the measurement back into the layout, so the stable container is observed.
		const container = element.parentElement ?? element;
		const update = () => setWidth(Math.floor(element.getBoundingClientRect().width));
		const frame = requestAnimationFrame(update);
		const observer = new ResizeObserver(update);
		observer.observe(container);
		window.addEventListener("resize", update);
		return () => {
			cancelAnimationFrame(frame);
			window.removeEventListener("resize", update);
			observer.disconnect();
		};
	}, []);

	const available = Math.max(0, (width > 0 ? width : INITIAL_WIDTH) - 2 * PADDING_X);
	const columns = Math.max(1, Math.floor((available + GAP) / (MIN_CARD + GAP)));
	const cardWidth = Math.max(1, (available - (columns - 1) * GAP) / columns);
	/// A new card width means the picture (16:9) and therefore the row height change; the first rendered card measures
	/// the exact value, and the estimate only covers the first frame.
	const estimate = Math.round((cardWidth * 9) / 16 + 98);
	const cardHeight = measured !== null && Math.abs(measured.width - cardWidth) < 2 ? measured.height : estimate;

	/// The table's page rule decides the headers, the folds and the filtered groups; the grid only packs the cards below.
	const table = useMemo(() => tableItems(rows, foldedPages, groups), [foldedPages, groups, rows]);
	const units = useMemo(() => gridItems(table, columns), [columns, table]);
	const rowIds = useMemo(() => table.map((item) => item.id), [table]);

	const scrollMargin = useScrollMargin(scrollRef, grid);
	const {
		items: virtualUnits,
		totalSize,
		scrollToIndex,
	} = useVirtualWindow({
		count: units.length,
		getScrollElement: () => scrollRef?.current ?? null,
		estimateSize: (index) => {
			const unit = units[index];
			return unit ? unitHeight(unit, cardHeight) + GAP : GAP;
		},
		scrollMargin,
		paddingStart: PADDING_Y,
		paddingEnd: PADDING_Y - GAP,
		overscan: 4,
		// The grid is server-rendered in tests and on the first frame; without a rect the virtualizer would mount no cards.
		initialRect: { width: INITIAL_WIDTH, height: 2240 },
	});

	/// One rendered card is enough: all cards of a row have the same width, fixed picture ratio and truncated text.
	useLayoutEffect(() => {
		const card = grid.current?.querySelector<HTMLElement>("[data-grid-card]");
		if (!card) return;
		const frame = requestAnimationFrame(() => {
			const height = Math.round(card.getBoundingClientRect().height);
			if (height > 0 && Math.abs(height - cardHeight) > 1) {
				setMeasured({ width: cardWidth, height });
			}
		});
		return () => cancelAnimationFrame(frame);
	}, [cardHeight, cardWidth]);

	const focusedUnit =
		focused === null ? -1 : units.findIndex((unit) => unit.kind === "cards" && unit.mods.some((mod) => mod.id === focused));
	const lastFocused = useRef(-1);
	useEffect(() => {
		if (focusedUnit < 0 || lastFocused.current === focusedUnit) return;
		lastFocused.current = focusedUnit;
		scrollToIndex(focusedUnit, { align: "center" });
	}, [focusedUnit, scrollToIndex]);

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

	return (
		<div className="flex min-h-0 min-w-0 flex-1 flex-col">
			{rows.length === 0 && <NoMatch onClearFilters={onClearFilters} />}
			<DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={dropped}>
				<SortableContext items={rowIds} strategy={rectSortingStrategy}>
					<ol ref={grid} className="relative" style={{ height: totalSize }}>
						{virtualUnits.flatMap((item) => {
							const at = item.index;
							const unit = units[at];
							if (!unit) return [];
							const top = item.start - scrollMargin;
							if (unit.kind === "page" || unit.kind === "group") {
								// A page row keeps its details on `page`; wrap them in the same `Group` a user row already is.
								const header: Group =
									unit.kind === "page"
										? { source: "page", id: unit.id, name: unit.page.name, parts: unit.page.mods, page: unit.page }
										: unit.group;
								const storedId = unit.kind === "group" ? (unit.group.group?.id ?? unit.id) : null;
								return [
									<GridPageHeader
										key={unit.id}
										group={header}
										folded={foldedPages.has(unit.id)}
										filtering={filtering}
										positionStyle={{ position: "absolute", top, left: PADDING_X, right: PADDING_X }}
										onToggle={(enabled) => void onSetEnabled(unit.id, enabled)}
										onFold={() => onTogglePage(unit.id)}
										onOpenSource={onOpenSource}
										onDeletePage={() => {
											if (unit.kind === "page") onDeletePage(unit.page.pageId);
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
									/>,
								];
							}
							if (unit.kind === "separator") {
								return [
									<SeparatorRow
										key={unit.id}
										id={unit.id}
										name={unit.name}
										count={groupSizes.get(unit.id) ?? 0}
										collapsed={collapsed.has(unit.id)}
										fresh={freshSeparator === unit.id}
										className="col-span-full px-3"
										positionStyle={{ position: "absolute", top, left: PADDING_X, right: PADDING_X }}
										onRenameSeparator={onRenameSeparator}
										onRenameDone={onRenameDone}
										onRemoveSeparator={onRemoveSeparator}
										onGroupEnabled={onGroupEnabled}
										onSortGroup={onSortGroup}
										onToggleGroup={onToggleGroup}
									/>,
								];
							}
							return unit.mods.map((mod, column) => (
								<ModCard
									key={mod.id}
									gameId={gameId}
									groupMenu={groupMenu}
									mod={mod}
									picked={selectionSet.has(mod.id)}
									focused={focused === mod.id}
									filtering={filtering}
									hidden={hidden.has(mod.id)}
									isFirst={mod.id === firstModId}
									isLast={mod.id === lastModId}
									position={positions.get(mod.id)}
									setSize={rows.length}
									update={updates[mod.id]}
									libraryDir={libraryDir}
									archivesDir={archivesDir}
									positionStyle={{
										position: "absolute",
										top,
										left: PADDING_X + column * (cardWidth + GAP),
										width: cardWidth,
									}}
									onPick={onPick}
									onToggle={onToggle}
									onMove={onMove}
									onOpenFolder={onOpenFolder}
									onRevealFile={onRevealFile}
									onOpenSource={onOpenSource}
									onCheckUpdate={onCheckUpdate}
									onUpdateMod={onUpdateMod}
									onIgnoreVersion={onIgnoreVersion}
									onUnignoreVersion={onUnignoreVersion}
									onSetHidden={onSetHidden}
									readyProviders={readyProviders}
									onRemoveFromLibrary={onRemoveFromLibrary}
									onDeleteVersion={onDeleteVersion}
									onReinstall={onReinstall}
									onArchiveReport={onArchiveReport}
									onChangeInstaller={onChangeInstaller}
									onAddSeparatorAbove={onAddSeparatorAbove}
									onShowDetails={onShowDetails}
								/>
							));
						})}
					</ol>
				</SortableContext>
			</DndContext>
		</div>
	);
}
