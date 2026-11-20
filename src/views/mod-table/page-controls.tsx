import type { DraggableAttributes, DraggableSyntheticListeners } from "@dnd-kit/core";
import { cn } from "cn";
import {
	ChevronDown,
	ChevronRight,
	EllipsisVertical,
	ExternalLink,
	GripVertical,
	Languages,
	ListTree,
	Minus,
	Pencil,
	Power,
	Trash2,
} from "lucide-react";
import type { ModGroupKind } from "@/bindings";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useTranslation } from "@/i18n";

/// The grip that drags a page header as one block. The table's header and the grid's compact header share it.
export function PageDragHandle({
	name,
	filtering,
	setActivatorNodeRef,
	attributes,
	listeners,
	className,
}: {
	name: string;
	filtering: boolean;
	setActivatorNodeRef: (element: HTMLElement | null) => void;
	attributes: DraggableAttributes;
	listeners: DraggableSyntheticListeners | undefined;
	className?: string;
}) {
	const { t } = useTranslation();
	return (
		<Button
			ref={setActivatorNodeRef}
			variant="ghost"
			size="icon-sm"
			aria-label={t("mods.dragHandle", { name })}
			title={filtering ? t("mods.dragDisabled") : t("mods.dragHandle", { name })}
			disabled={filtering}
			className={cn(
				"text-subtle-foreground cursor-grab touch-none active:cursor-grabbing disabled:cursor-default disabled:opacity-40",
				className,
			)}
			{...attributes}
			{...listeners}
		>
			<GripVertical className="size-4" aria-hidden />
		</Button>
	);
}

/// The fold control of a page header, shared by the table row and the grid header.
export function PageFoldButton({ name, folded, onFold }: { name: string; folded: boolean; onFold: () => void }) {
	const { t } = useTranslation();
	return (
		<Button
			variant="ghost"
			size="icon-sm"
			aria-expanded={!folded}
			aria-label={folded ? t("mods.page.expand", { name }) : t("mods.page.fold", { name })}
			className="text-subtle-foreground aria-expanded:text-subtle-foreground shrink-0 aria-expanded:bg-transparent"
			onClick={onFold}
		>
			{folded ? <ChevronRight className="size-4" aria-hidden /> : <ChevronDown className="size-4" aria-hidden />}
		</Button>
	);
}

/// The tri-state switch of a page or user-group header: all parts on, none, or a mix. It has no stored state of its own.
/// `label` names the group for a screen reader; a page keeps the shared default.
export function PageToggle({ all, none, onToggle, label }: { all: boolean; none: boolean; onToggle: () => void; label?: string }) {
	const { t } = useTranslation();
	const mixed = !(all || none);
	let title = t("mods.page.enableAll");

	if (all) title = t("mods.page.disableAll");
	else if (mixed) title = t("mods.page.mixed");
	// The same size and colours as the mod rows' switch, so a header and its parts read as one column.
	let thumb = "bg-foreground translate-x-0";

	if (all) thumb = "bg-primary-foreground translate-x-[calc(100%-2px)]";
	else if (mixed) thumb = "bg-primary-foreground translate-x-[7px]";
	return (
		<>
			{/* biome-ignore lint/a11y/useSemanticElements: the page toggle is a tri-state switch; a native checkbox cannot keep the switch track and thumb. */}
			<button
				type="button"
				role="checkbox"
				aria-checked={mixed ? "mixed" : all}
				aria-label={label ?? t("mods.page.toggle")}
				title={title}
				onClick={onToggle}
				className={cn(
					"focus-visible:border-ring focus-visible:ring-ring/50 relative inline-flex h-[18.4px] w-8 shrink-0 items-center rounded-full border border-transparent transition-all outline-none focus-visible:ring-3",
					all || mixed ? "bg-primary" : "bg-input/80",
				)}
			>
				<span className={cn("pointer-events-none grid size-4 place-items-center rounded-full transition-transform", thumb)}>
					{mixed && <Minus className="text-primary size-2.5" aria-hidden />}
				</span>
			</button>
		</>
	);
}

/// The menu of a page header: fold, switch every part, open the page and delete the whole page. The table and the grid
/// share it, so both headers offer the same actions.
export function PageMenu({
	name,
	folded,
	all,
	hasSource,
	onFold,
	onToggleAll,
	onOpenSource,
	onDeletePage,
}: {
	name: string;
	folded: boolean;
	all: boolean;
	hasSource: boolean;
	onFold: () => void;
	onToggleAll: () => void;
	onOpenSource: () => void;
	onDeletePage: () => void;
}) {
	const { t } = useTranslation();
	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="ghost" size="icon-sm" aria-label={t("mods.page.menu", { name })}>
					<EllipsisVertical aria-hidden />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="start" className="w-56">
				<DropdownMenuItem onSelect={onFold}>
					<ListTree /> {folded ? t("mods.page.expand", { name }) : t("mods.page.fold", { name })}
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={onToggleAll}>
					<Power /> {all ? t("mods.page.disableAll") : t("mods.page.enableAll")}
				</DropdownMenuItem>
				{hasSource && (
					<DropdownMenuItem onSelect={onOpenSource}>
						<ExternalLink /> {t("mods.page.open")}
					</DropdownMenuItem>
				)}
				<DropdownMenuSeparator />
				<DropdownMenuItem variant="destructive" onSelect={onDeletePage}>
					<Trash2 /> {t("mods.page.delete")}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}

/// The menu of a user-group header: fold, switch every part, rename, the translation action when it applies, and
/// removing the group. The table and the grid share it, as they share `PageMenu` for a source page.
export function GroupMenu({
	name,
	folded,
	all,
	kind,
	onFold,
	onToggleAll,
	onRename,
	onSetTranslationWinner,
	onUngroup,
}: {
	name: string;
	folded: boolean;
	all: boolean;
	kind: ModGroupKind;
	onFold: () => void;
	onToggleAll: () => void;
	onRename: () => void;
	onSetTranslationWinner: () => void;
	onUngroup: () => void;
}) {
	const { t } = useTranslation();
	return (
		<DropdownMenu>
			<DropdownMenuTrigger asChild>
				<Button variant="ghost" size="icon-sm" aria-label={t("mods.page.menu", { name })}>
					<EllipsisVertical aria-hidden />
				</Button>
			</DropdownMenuTrigger>
			<DropdownMenuContent align="start" className="w-56">
				<DropdownMenuItem onSelect={onFold}>
					<ListTree /> {folded ? t("mods.page.expand", { name }) : t("mods.page.fold", { name })}
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={onToggleAll}>
					<Power /> {all ? t("mods.page.disableAll") : t("mods.page.enableAll")}
				</DropdownMenuItem>
				<DropdownMenuItem onSelect={onRename}>
					<Pencil /> {t("mods.group.rename")}
				</DropdownMenuItem>
				{kind === "translation" && (
					<DropdownMenuItem onSelect={onSetTranslationWinner}>
						<Languages /> {t("mods.group.setTranslationWinner")}
					</DropdownMenuItem>
				)}
				<DropdownMenuSeparator />
				<DropdownMenuItem onSelect={onUngroup}>
					<Trash2 /> {t("mods.group.remove")}
				</DropdownMenuItem>
			</DropdownMenuContent>
		</DropdownMenu>
	);
}
