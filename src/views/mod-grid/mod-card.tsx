import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { cn } from "cn";
import { GripVertical } from "lucide-react";
import { type CSSProperties, memo } from "react";
import type { UpdateReport } from "@/bindings";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { useTranslation } from "@/i18n";
import type { Mod } from "@/lib/mods";
import { CardPicture, type CardSignal } from "@/views/mod-grid/card-picture";
import { itemClicks, ModMenu, type ModMenuActions } from "@/views/mod-table/mod-menu";
import type { ModRowActions } from "@/views/mod-table/mod-row";

/// The actions a card runs: picking the mod, and the same menu as a row of the table.
export type ModCardActions = ModMenuActions & Pick<ModRowActions, "onPick">;

export interface ModCardProps extends ModCardActions {
	gameId: string;
	mod: Mod;
	picked: boolean;
	focused: boolean;
	filtering: boolean;
	/// The user hid the mod from this list.
	hidden: boolean;
	isFirst: boolean;
	isLast: boolean;
	update: UpdateReport | undefined;
	libraryDir: string;
	archivesDir: string;
	/// The absolute position in a virtualized grid; `undefined` in a normally laid-out grid.
	positionStyle?: CSSProperties;
	/// The card's position among the shown mods and that count, for a screen reader while the grid is virtualized.
	position?: number;
	setSize?: number;
}

/// One mod as a card: its picture, name, category and switch. The name picks the mod, the grip drags it, and the menu has
/// the actions of a table row. A problem with the mod shows on the picture, a missing requirement before a new version.
export const ModCard = memo(function ModCard({
	gameId,
	mod,
	picked,
	focused,
	filtering,
	hidden,
	isFirst,
	isLast,
	update,
	libraryDir,
	archivesDir,
	positionStyle,
	position,
	setSize,
	...actions
}: ModCardProps) {
	const { onPick, onToggle } = actions;
	const { t } = useTranslation();
	const { attributes, listeners, setNodeRef, setActivatorNodeRef, transform, transition, isDragging } = useSortable({
		id: mod.id,
		disabled: filtering,
	});
	const style = { transform: CSS.Transform.toString(transform), transition };
	let signal: CardSignal | null = null;

	if (mod.unmet_requirements > 0) {
		signal = { kind: "missing", text: t("mods.col.missing") };
	} else if (update?.has_update && update.latest) {
		const installed = update.latest_installed;
		signal = {
			kind: installed ? "installed" : "update",
			text: installed
				? t("mods.version.installed", { version: update.latest })
				: t("mods.version.updateAvailable", { version: update.latest }),
		};
	}

	const category = mod.source?.category;

	return (
		<ModMenu
			mod={mod}
			update={update}
			filtering={filtering}
			hidden={hidden}
			isFirst={isFirst}
			isLast={isLast}
			libraryDir={libraryDir}
			archivesDir={archivesDir}
			{...actions}
		>
			<li
				ref={setNodeRef}
				style={{ ...style, ...positionStyle }}
				data-grid-card
				aria-posinset={position}
				aria-setsize={setSize}
				{...itemClicks(mod.id, actions)}
				className={cn(
					// Offscreen cards keep a measured size instead of a layout pass; the browser paints them when they come near.
					"bg-card grid min-w-0 cursor-default content-start gap-2 rounded-lg border p-2 [contain-intrinsic-size:auto_13rem] [content-visibility:auto]",
					picked ? "bg-muted" : "hover:bg-muted/50",
					// The mark belongs to the whole card: the focused one, and any card that holds keyboard focus.
					focused && "ring-ring ring-2",
					"has-focus-visible:ring-ring has-focus-visible:ring-2",
					isDragging && "relative z-10 opacity-90 shadow-md",
				)}
			>
				<CardPicture gameId={gameId} source={mod.source} enabled={mod.enabled} signal={signal} />
				<button
					type="button"
					data-mod={mod.id}
					aria-current={focused ? "true" : undefined}
					aria-pressed={picked}
					// No ring of its own: the card shows the focus, so the mark covers the whole card.
					className="grid min-w-0 gap-0.5 rounded-sm px-1 text-left outline-none"
					onClick={(event) => onPick(mod.id, event)}
				>
					{/* One line for the name; the full name is in the tooltip and in the details panel. The category line is always there, so the cards stay the same height. */}
					<span className={cn("truncate text-sm leading-5 font-medium", !mod.enabled && "text-muted-foreground")} title={mod.info.name}>
						{mod.info.name}
					</span>
					<span className="text-muted-foreground h-4 truncate text-xs leading-4">{category}</span>
				</button>
				<div className="flex items-center justify-between gap-2 px-1">
					<Button
						ref={setActivatorNodeRef}
						variant="ghost"
						size="icon-sm"
						aria-label={t("mods.dragHandle", { name: mod.info.name })}
						title={filtering ? t("mods.dragDisabled") : t("mods.dragHandle", { name: mod.info.name })}
						disabled={filtering}
						className="text-subtle-foreground cursor-grab touch-none active:cursor-grabbing"
						{...attributes}
						{...listeners}
					>
						<GripVertical aria-hidden />
					</Button>
					<Switch
						checked={mod.enabled}
						aria-label={t("mods.toggle", { name: mod.info.name })}
						onCheckedChange={(enabled) => void onToggle(mod.id, enabled)}
					/>
				</div>
			</li>
		</ModMenu>
	);
});
