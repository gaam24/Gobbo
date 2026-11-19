import {
	ArrowDown,
	ArrowUp,
	Download,
	ExternalLink,
	Eye,
	EyeOff,
	FileMinus,
	FileSearch,
	FolderArchive,
	FolderOpen,
	Languages,
	Layers,
	ListPlus,
	PackageOpen,
	PanelRight,
	Pencil,
	Power,
	PowerOff,
	RefreshCw,
	Settings2,
	Trash2,
} from "lucide-react";
import { type MouseEvent as ReactMouseEvent, type ReactNode, useCallback, useRef, useState } from "react";
import type { ModGroup, UpdateReport } from "@/bindings";
import {
	ContextMenu,
	ContextMenuCheckboxItem,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuLabel,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { useTranslation } from "@/i18n";
import type { Mod } from "@/lib/mods";
import { anyReady } from "@/sources/ready";
import type { ModRowActions } from "@/views/mod-table/mod-row";

/// The actions a mod's menu runs. The table's rows and the grid's cards pass the same ones.
export type ModMenuActions = Pick<
	ModRowActions,
	| "groupMenu"
	| "onShowDetails"
	| "onToggle"
	| "onMove"
	| "onAddSeparatorAbove"
	| "onOpenFolder"
	| "onRevealFile"
	| "onReinstall"
	| "onArchiveReport"
	| "onChangeInstaller"
	| "onOpenSource"
	| "onCheckUpdate"
	| "onUpdateMod"
	| "onIgnoreVersion"
	| "onUnignoreVersion"
	| "onSetHidden"
	| "onRemoveFromLibrary"
	| "onDeleteVersion"
	| "readyProviders"
>;

/// Whether the event comes from one of `controls` inside the row or card.
function inside(event: ReactMouseEvent, controls: string) {
	return event.target instanceof Element && event.target.closest(controls) !== null;
}

/// The pointer of a whole row or card: a click picks the mod, a double click shows its details. The controls inside keep
/// their own clicks; the name button is the keyboard way in.
export function itemClicks(modId: string, { onPick, onShowDetails }: Pick<ModRowActions, "onPick" | "onShowDetails">) {
	return {
		onClick: (event: ReactMouseEvent) => {
			if (!inside(event, "button, a, input, select, [role='combobox'], [role='switch']")) onPick(modId, event);
		},
		onDoubleClick: (event: ReactMouseEvent) => {
			if (!inside(event, "button, [role='switch'], [role='combobox'], a")) onShowDetails(modId);
		},
	};
}

interface ModMenuProps extends ModMenuActions {
	mod: Mod;
	update?: UpdateReport;
	filtering: boolean;
	/// The user hid the mod from this list.
	hidden: boolean;
	isFirst: boolean;
	isLast: boolean;
	libraryDir: string;
	archivesDir: string;
	/// The element that opens the menu on a right click.
	children: ReactNode;
}

/// The items of a mod's context menu, the same in the table and in the grid. Exported so a test can render the items
/// without a browser: the Radix menu mounts its content only after the first open.
export function ModMenuContent({
	mod,
	update,
	filtering,
	hidden,
	isFirst,
	isLast,
	libraryDir,
	archivesDir,
	groupMenu,
	readyProviders,
	onShowDetails,
	onToggle,
	onMove,
	onAddSeparatorAbove,
	onOpenFolder,
	onRevealFile,
	onReinstall,
	onArchiveReport,
	onChangeInstaller,
	onOpenSource,
	onCheckUpdate,
	onUpdateMod,
	onIgnoreVersion,
	onUnignoreVersion,
	onSetHidden,
	onRemoveFromLibrary,
	onDeleteVersion,
	deferClose,
}: Omit<ModMenuProps, "children"> & { deferClose: (action: () => void) => void }) {
	const { t } = useTranslation();
	const source = mod.source;
	/// The lookup reads the newest status, so the group is resolved when the menu opens, not from the row's last render.
	const group: ModGroup | null = groupMenu.groupForMenu(mod.id);
	const member = group?.members.includes(mod.id);
	/// A right click on one of several selected mods still gets a group action, even when none of them is grouped yet.
	const selection = groupMenu.groupSelection();
	const groupSelected = group === null && selection.length > 1 && selection.includes(mod.id);

	return (
		<>
			{group !== null && (
				<>
					<ContextMenuLabel className="flex items-center gap-1.5">
						{t("mods.menu.groups")}
						<span className="text-subtle-foreground font-normal">{group.name}</span>
					</ContextMenuLabel>
					<ContextMenuItem onSelect={() => groupMenu.onEditGroup(group.id, group.members)}>
						<Pencil /> {t("mods.group.rename")}
					</ContextMenuItem>
					{group.kind === "translation" && (
						<ContextMenuItem onSelect={() => groupMenu.onSetTranslationWinner(group.id)}>
							<Languages /> {t("mods.group.setTranslationWinner")}
						</ContextMenuItem>
					)}
					<ContextMenuItem onSelect={() => groupMenu.onUngroupMods(group.id)}>
						<Trash2 /> {t("mods.group.remove")}
					</ContextMenuItem>
					{/* One checkbox covers both directions; the section label above names the group it belongs to. */}
					<ContextMenuCheckboxItem checked={member} onSelect={() => groupMenu.onSetModGroup(mod.id, member ? null : group.id)}>
						{t("mods.group.action")}
					</ContextMenuCheckboxItem>
					<ContextMenuSeparator />
				</>
			)}
			{groupSelected && (
				<>
					<ContextMenuLabel>{t("mods.menu.groups")}</ContextMenuLabel>
					<ContextMenuItem onSelect={() => groupMenu.onGroupSelected(selection)}>
						<Layers /> {t("mods.group.groupSelected")}
					</ContextMenuItem>
					<ContextMenuSeparator />
				</>
			)}
			<ContextMenuLabel>{t("mods.menu.mod")}</ContextMenuLabel>
			<ContextMenuItem onSelect={() => onShowDetails(mod.id)}>
				<PanelRight /> {t("mods.showDetails")}
			</ContextMenuItem>
			<ContextMenuItem onSelect={() => void onToggle(mod.id, !mod.enabled)}>
				{mod.enabled ? <PowerOff /> : <Power />} {mod.enabled ? t("common.disable") : t("common.enable")}
			</ContextMenuItem>
			{hidden ? (
				<ContextMenuItem onSelect={() => onSetHidden([mod.id], false)}>
					<Eye /> {t("mods.unhide")}
				</ContextMenuItem>
			) : (
				!mod.enabled && (
					<ContextMenuItem onSelect={() => onSetHidden([mod.id], true)}>
						<EyeOff /> {t("mods.hide")}
					</ContextMenuItem>
				)
			)}
			<ContextMenuSeparator />
			<ContextMenuLabel>{t("mods.menu.order")}</ContextMenuLabel>
			<ContextMenuItem disabled={filtering || isFirst} onSelect={() => void onMove(mod.id, -1)}>
				<ArrowUp /> {t("mods.moveUp.label")}
			</ContextMenuItem>
			<ContextMenuItem disabled={filtering || isLast} onSelect={() => void onMove(mod.id, 1)}>
				<ArrowDown /> {t("mods.moveDown.label")}
			</ContextMenuItem>
			<ContextMenuItem
				disabled={filtering}
				onSelect={() => {
					deferClose(() => onAddSeparatorAbove(mod.id));
				}}
			>
				<ListPlus /> {t("separators.addAbove")}
			</ContextMenuItem>
			<ContextMenuSeparator />
			<ContextMenuLabel>{t("mods.menu.files")}</ContextMenuLabel>
			<ContextMenuItem onSelect={() => void onOpenFolder(`${libraryDir}/${mod.folder}`)}>
				<FolderOpen /> {t("mods.openFolder")}
			</ContextMenuItem>
			<ContextMenuItem
				disabled={!mod.archive_file}
				onSelect={() => mod.archive_file && void onRevealFile(`${archivesDir}/${mod.archive_file}`)}
			>
				<FolderArchive /> {t("mods.showArchive")}
			</ContextMenuItem>
			<ContextMenuItem disabled={!mod.archive_file} onSelect={() => void onReinstall(mod.folder)}>
				<PackageOpen /> {t("mods.reinstall")}
			</ContextMenuItem>
			<ContextMenuItem disabled={!mod.archive_file} onSelect={() => onArchiveReport(mod.folder, mod.info.name)}>
				<FileSearch /> {t("mods.archiveReport")}
			</ContextMenuItem>
			{mod.fomod_installer && (
				<ContextMenuItem onSelect={() => onChangeInstaller(mod.id, mod.folder, mod.info.name)}>
					<Settings2 /> {t("mods.changeInstaller")}
				</ContextMenuItem>
			)}
			<ContextMenuSeparator />
			<ContextMenuLabel>{t("mods.menu.provider")}</ContextMenuLabel>
			{source && (
				<ContextMenuItem onSelect={() => onOpenSource(source.provider, source.mod_id)}>
					<ExternalLink /> {t("mods.sourcePage")}
				</ContextMenuItem>
			)}
			{update?.latest && update.has_update && (
				<>
					{!update.latest_installed && (
						<ContextMenuItem onSelect={() => void onUpdateMod(mod.id, mod.info.name, update)}>
							<Download /> {t("mods.version.updateNow", { version: update.latest })}
						</ContextMenuItem>
					)}
					<ContextMenuItem onSelect={() => void onIgnoreVersion(mod.id, update.provider)}>
						<EyeOff /> {t("mods.version.ignore")}
					</ContextMenuItem>
				</>
			)}
			{update?.state === "ignored" && (
				<ContextMenuItem onSelect={() => void onUnignoreVersion(mod.id, update.provider)}>
					<Eye /> {t("mods.version.showIgnored")}
				</ContextMenuItem>
			)}
			<ContextMenuItem
				disabled={
					!anyReady(
						readyProviders,
						mod.sources.map((entry) => entry.provider),
					)
				}
				onSelect={() => void onCheckUpdate(mod.id)}
			>
				<RefreshCw /> {t("mods.checkUpdate")}
			</ContextMenuItem>
			<ContextMenuSeparator />
			<ContextMenuLabel>{t("mods.menu.delete")}</ContextMenuLabel>
			{mod.version_count > 1 && (
				<ContextMenuItem variant="destructive" onSelect={() => onDeleteVersion(mod.id, mod.folder)}>
					<FileMinus /> {t("mods.version.deleteCurrent")}
				</ContextMenuItem>
			)}
			<ContextMenuItem variant="destructive" onSelect={() => onRemoveFromLibrary(mod.id)}>
				<Trash2 /> {t("mods.deleteMod")}
			</ContextMenuItem>
		</>
	);
}

/// The context menu of one mod, the same in the table and in the grid.
export function ModMenu({ children, ...props }: ModMenuProps) {
	/// Runs once the menu has closed. The open menu keeps the focus to itself, so a new separator's name field could not
	/// take it before, and the closing menu must not give it back to the row after.
	const afterClose = useRef<(() => void) | null>(null);
	const deferClose = useCallback((action: () => void) => {
		afterClose.current = action;
	}, []);
	/// The items are built only after the first open. A closed Radix menu still creates its content element tree for
	/// every row, and two thousand rows made that a large part of the first render.
	const [mounted, setMounted] = useState(false);

	return (
		<ContextMenu
			onOpenChange={(open) => {
				if (open) setMounted(true);
			}}
		>
			<ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
			{mounted && (
				<ContextMenuContent
					onCloseAutoFocus={(event) => {
						const action = afterClose.current;
						if (!action) return;
						afterClose.current = null;
						event.preventDefault();
						action();
					}}
				>
					<ModMenuContent {...props} deferClose={deferClose} />
				</ContextMenuContent>
			)}
		</ContextMenu>
	);
}
