import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { cn } from "cn";
import { ArrowLeft, ArrowRight, EyeOff, MoveHorizontal, RotateCcw } from "lucide-react";
import type { CSSProperties } from "react";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuLabel,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { useTranslation } from "@/i18n";
import type { ColumnSpec } from "@/lib/mod-columns";
import { ColumnHandle } from "@/views/mod-table/column-handle";

/// A column's header. Drag it to move the column along the row; its edge resizes it. Its menu fits, moves, hides and
/// resets the column, and the moves are the way to reorder from the keyboard.
export function ColumnHeader({
	column,
	width,
	first,
	last,
	onResize,
	onFit,
	onFitAll,
	onHide,
	onStep,
	onReset,
}: {
	column: ColumnSpec;
	/// The width the user set, if any. A column that still fills the space has none.
	width?: number;
	/// The ends of the columns on screen: nothing moves further out.
	first: boolean;
	last: boolean;
	onResize: (width: number) => void;
	onFit: () => void;
	onFitAll: () => void;
	/// Missing for a pinned column, which cannot be hidden.
	onHide?: () => void;
	onStep: (step: -1 | 1) => void;
	onReset: () => void;
}) {
	const { t } = useTranslation();
	const name = t(`mods.col.${column.id}`);
	const { listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: column.id });
	// The header moves sideways only, so the vertical part of the drag is left out.
	const style: CSSProperties = { transform: CSS.Transform.toString(transform && { ...transform, y: 0 }), transition };

	return (
		<ContextMenu>
			<ContextMenuTrigger asChild>
				{/* biome-ignore lint/a11y/useSemanticElements: a table header would leave the grid layout and drag. */}
				<span
					ref={setNodeRef}
					role="columnheader"
					tabIndex={-1}
					style={style}
					title={t("mods.columnDrag")}
					className={cn(
						"relative flex min-w-0 cursor-grab touch-none items-center self-stretch rounded-sm select-none active:cursor-grabbing",
						column.end && "justify-end",
						isDragging && "bg-muted text-foreground z-20 opacity-90 shadow-md",
					)}
					{...listeners}
				>
					<span className="truncate">{name}</span>
					<ColumnHandle name={name} width={width} min={column.min} onResize={onResize} onFit={onFit} />
				</span>
			</ContextMenuTrigger>
			<ContextMenuContent>
				<ContextMenuLabel>{t("mods.columnsMenu.size")}</ContextMenuLabel>
				<ContextMenuItem onSelect={onFit}>
					<MoveHorizontal /> {t("mods.columnFit", { name })}
				</ContextMenuItem>
				<ContextMenuItem onSelect={onFitAll}>{t("mods.columnsFit")}</ContextMenuItem>
				<ContextMenuSeparator />
				<ContextMenuLabel>{t("mods.columnsMenu.position")}</ContextMenuLabel>
				<ContextMenuItem disabled={first} onSelect={() => onStep(-1)}>
					<ArrowLeft /> {t("mods.columnMoveLeft")}
				</ContextMenuItem>
				<ContextMenuItem disabled={last} onSelect={() => onStep(1)}>
					<ArrowRight /> {t("mods.columnMoveRight")}
				</ContextMenuItem>
				{onHide && (
					<>
						<ContextMenuSeparator />
						<ContextMenuItem onSelect={onHide}>
							<EyeOff /> {t("mods.columnHide")}
						</ContextMenuItem>
					</>
				)}
				<ContextMenuSeparator />
				<ContextMenuItem onSelect={onReset}>
					<RotateCcw /> {t("mods.columnsReset")}
				</ContextMenuItem>
			</ContextMenuContent>
		</ContextMenu>
	);
}
