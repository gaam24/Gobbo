import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { cn } from "cn";
import { ArrowDownAZ, ChevronDown, ChevronRight, GripVertical, PencilLine, ToggleLeft, ToggleRight, Trash2 } from "lucide-react";
import { type CSSProperties, memo, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuLabel,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { Input } from "@/components/ui/input";
import { useTranslation } from "@/i18n";

/// What a separator row does: rename or remove it, switch its group on or off, sort it, and fold the group.
export interface SeparatorActions {
	onRenameSeparator: (id: string, name: string) => void | Promise<void>;
	/// The new separator's name field closed, renamed or not.
	onRenameDone: () => void;
	onRemoveSeparator: (id: string) => void | Promise<void>;
	onGroupEnabled: (id: string, enabled: boolean) => void | Promise<void>;
	onSortGroup: (id: string) => void | Promise<void>;
	onToggleGroup: (id: string) => void;
}

/// A separator: a header over the mods below it, up to the next one. It moves like a mod; its menu renames or removes
/// it and switches its whole group on or off. The arrow folds the group.
export const SeparatorRow = memo(function SeparatorRow({
	id,
	name,
	count,
	collapsed,
	fresh,
	className,
	positionStyle,
	position,
	setSize,
	onRenameSeparator,
	onRenameDone,
	onRemoveSeparator,
	onGroupEnabled,
	onSortGroup,
	onToggleGroup,
}: SeparatorActions & {
	id: string;
	name: string;
	count: number;
	collapsed: boolean;
	/// Just made from the menu: its name field opens at once.
	fresh: boolean;
	/// Extra classes for the header, e.g. `col-span-full` in the grid.
	className?: string;
	/// The absolute position in a virtualized grid.
	positionStyle?: CSSProperties;
	/// The row's position in the whole list and the list size, for a screen reader while rows are virtualized.
	position?: number;
	setSize?: number;
}) {
	const { t } = useTranslation();
	const [editing, setEditing] = useState(fresh);
	const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({ id });
	const style = { transform: CSS.Transform.toString(transform), transition };
	const Chevron = collapsed ? ChevronRight : ChevronDown;

	/// Escape leaves the name as it was. Enter and Escape both end in the blur, so a name is saved once.
	const cancelled = useRef(false);
	const rename = (value: string) => {
		setEditing(false);
		onRenameDone();
		const trimmed = value.trim();
		if (!cancelled.current && trimmed && trimmed !== name) void onRenameSeparator(id, trimmed);
		cancelled.current = false;
	};

	return (
		<ContextMenu>
			<ContextMenuTrigger asChild>
				{/* biome-ignore lint/a11y/useSemanticElements: the virtual list and the grid layout need div rows. */}
				<div
					ref={setNodeRef}
					style={{ ...style, ...positionStyle }}
					role="row"
					tabIndex={-1}
					aria-rowindex={position === undefined ? undefined : position + 1}
					aria-posinset={position}
					aria-setsize={setSize}
					className={cn(
						"bg-card flex h-10 items-center gap-2 border-b px-6 py-1.5",
						isDragging && "relative z-10 opacity-90 shadow-md",
						className,
					)}
				>
					<Button
						ref={setActivatorNodeRef}
						variant="ghost"
						size="icon-sm"
						aria-label={t("separators.drag", { name })}
						className="text-subtle-foreground cursor-grab touch-none active:cursor-grabbing"
						{...attributes}
						{...listeners}
						tabIndex={-1}
					>
						<GripVertical className="size-4" aria-hidden />
					</Button>
					<Button
						variant="ghost"
						size="icon-sm"
						aria-expanded={!collapsed}
						aria-label={t(collapsed ? "separators.expand" : "separators.collapse", { name })}
						className="text-muted-foreground aria-expanded:text-muted-foreground aria-expanded:bg-transparent"
						onClick={() => onToggleGroup(id)}
					>
						<Chevron className="size-4" aria-hidden />
					</Button>
					{editing ? (
						<Input
							autoFocus
							defaultValue={name}
							aria-label={t("separators.rename")}
							className="h-8 max-w-72"
							onFocus={(event) => event.currentTarget.select()}
							onBlur={(event) => rename(event.currentTarget.value)}
							onKeyDown={(event) => {
								if (event.key === "Escape") cancelled.current = true;
								if (event.key === "Enter" || event.key === "Escape") event.currentTarget.blur();
							}}
						/>
					) : (
						<button
							type="button"
							className="focus-visible:ring-ring truncate rounded-sm text-left text-sm font-semibold outline-none focus-visible:ring-2"
							onDoubleClick={() => setEditing(true)}
							onClick={() => onToggleGroup(id)}
						>
							{name}
						</button>
					)}
					<span className="text-subtle-foreground font-mono text-xs tabular-nums">{count}</span>
				</div>
			</ContextMenuTrigger>
			<ContextMenuContent>
				<ContextMenuItem onSelect={() => setEditing(true)}>
					<PencilLine /> {t("separators.rename")}
				</ContextMenuItem>
				<ContextMenuSeparator />
				<ContextMenuLabel>{t("separators.menu.group")}</ContextMenuLabel>
				<ContextMenuItem disabled={count === 0} onSelect={() => void onGroupEnabled(id, true)}>
					<ToggleRight /> {t("separators.enableAll")}
				</ContextMenuItem>
				<ContextMenuItem disabled={count === 0} onSelect={() => void onGroupEnabled(id, false)}>
					<ToggleLeft /> {t("separators.disableAll")}
				</ContextMenuItem>
				<ContextMenuItem disabled={count < 2} onSelect={() => void onSortGroup(id)}>
					<ArrowDownAZ /> {t("separators.sortGroup")}
				</ContextMenuItem>
				<ContextMenuSeparator />
				<ContextMenuItem variant="destructive" onSelect={() => void onRemoveSeparator(id)}>
					<Trash2 /> {t("separators.remove")}
				</ContextMenuItem>
			</ContextMenuContent>
		</ContextMenu>
	);
});
