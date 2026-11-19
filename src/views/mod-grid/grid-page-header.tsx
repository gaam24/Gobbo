import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { cn } from "cn";
import type { CSSProperties } from "react";
import { useTranslation } from "@/i18n";
import type { Group } from "@/lib/mods";
import { GroupMenu, PageDragHandle, PageFoldButton, PageMenu, PageToggle } from "@/views/mod-table/page-controls";

interface GridGroupHeaderProps {
	/// A source page or a stored user group. Both render one compact header over their cards.
	group: Group;
	folded: boolean;
	filtering: boolean;
	positionStyle?: CSSProperties;
	onToggle: (enabled: boolean) => void | Promise<void>;
	onFold: () => void;
	onOpenSource: (provider: string, modId: string, tab?: "files") => void;
	onDeletePage: () => void;
	onRenameGroup: () => void;
	onUngroup: () => void;
	onSetTranslationWinner: () => void;
}

/// The compact header of a source page or a user group in the grid: the same tri-state switch, fold, menu and drag as the
/// table's header, above the cards of its parts. It groups the cards and is not a mod of its own.
export function GridPageHeader({
	group,
	folded,
	filtering,
	positionStyle,
	onToggle,
	onFold,
	onOpenSource,
	onDeletePage,
	onRenameGroup,
	onUngroup,
	onSetTranslationWinner,
}: GridGroupHeaderProps) {
	const { t } = useTranslation();
	const mods = group.parts;
	const name = group.name;
	const author = group.page?.author ?? null;
	const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({
		id: group.id,
		disabled: filtering,
	});
	const style = { transform: CSS.Transform.toString(transform), transition };
	const all = mods.every((mod) => mod.enabled);
	const none = mods.every((mod) => !mod.enabled);
	const source = mods.find((mod) => mod.sources.length > 0)?.sources[0];

	return (
		<li
			ref={setNodeRef}
			style={{ ...style, ...positionStyle }}
			data-grid-page
			className={cn(
				"bg-muted/40 flex h-10 min-w-0 items-center gap-1.5 rounded-lg border px-2",
				isDragging && "relative z-10 opacity-90 shadow-md",
			)}
		>
			<PageDragHandle
				name={name}
				filtering={filtering}
				setActivatorNodeRef={setActivatorNodeRef}
				attributes={attributes}
				listeners={listeners}
				className="shrink-0"
			/>
			<PageFoldButton name={name} folded={folded} onFold={onFold} />
			<button
				type="button"
				onClick={onFold}
				className="focus-visible:ring-ring flex min-w-0 flex-1 items-center gap-2 rounded-sm px-1 py-1 text-left outline-none focus-visible:ring-2"
			>
				<span className="truncate text-sm font-semibold" title={name}>
					{name}
				</span>
				<span className="text-subtle-foreground shrink-0 text-xs">
					{author && (
						<>
							{author}
							<span aria-hidden>{" · "}</span>
						</>
					)}
					{group.page ? t("mods.page.parts", { count: mods.length }) : t("mods.group.mods", { count: mods.length })}
				</span>
			</button>
			<PageToggle all={all} none={none} onToggle={() => void onToggle(!all)} label={group.page ? undefined : t("mods.group.toggle")} />
			{group.page ? (
				<PageMenu
					name={name}
					folded={folded}
					all={all}
					hasSource={source !== undefined}
					onFold={onFold}
					onToggleAll={() => void onToggle(!all)}
					onOpenSource={() => {
						if (source) onOpenSource(source.provider, source.mod_id, "files");
					}}
					onDeletePage={onDeletePage}
				/>
			) : (
				<GroupMenu
					name={name}
					folded={folded}
					all={all}
					kind={group.group?.kind ?? "bundle"}
					onFold={onFold}
					onToggleAll={() => void onToggle(!all)}
					onRename={onRenameGroup}
					onSetTranslationWinner={onSetTranslationWinner}
					onUngroup={onUngroup}
				/>
			)}
		</li>
	);
}
