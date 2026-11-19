import { ArrowDownAZ, CheckCheck, ChevronsDownUp, ChevronsUpDown, ListTree, SplitSquareVertical, Upload } from "lucide-react";
import type { ComponentProps } from "react";
import { ContextMenuContent, ContextMenuItem, ContextMenuLabel, ContextMenuSeparator } from "@/components/ui/context-menu";
import { useTranslation } from "@/i18n";
import type { Mod } from "@/lib/mods";

/// The menu of the empty part of the list. Row menus are separate; this one holds actions that apply to the list as a
/// whole, and the closing menu defers the actions that open a field.
export function ModsListMenu({
	shown,
	mods,
	filtering,
	hasSeparators,
	onSelect,
	onAddSeparatorAtEnd,
	onImport,
	onSortByName,
	onSortByRules,
	onFoldAll,
	onCloseAutoFocus,
}: {
	shown: Mod[];
	mods: Mod[];
	filtering: boolean;
	hasSeparators: boolean;
	onSelect: (mods: string[]) => void;
	onAddSeparatorAtEnd: () => void;
	onImport: () => void | Promise<void>;
	onSortByName: () => void | Promise<void>;
	onSortByRules: () => void | Promise<void>;
	onFoldAll: (fold: boolean) => void;
	onCloseAutoFocus: ComponentProps<typeof ContextMenuContent>["onCloseAutoFocus"];
}) {
	const { t } = useTranslation();

	return (
		<ContextMenuContent onCloseAutoFocus={onCloseAutoFocus}>
			<ContextMenuLabel>{t("mods.menu.list")}</ContextMenuLabel>
			<ContextMenuItem disabled={shown.length === 0} onSelect={() => onSelect(shown.map((mod) => mod.id))}>
				<CheckCheck /> {t("mods.selectAll")}
			</ContextMenuItem>
			<ContextMenuItem disabled={filtering} onSelect={onAddSeparatorAtEnd}>
				<SplitSquareVertical /> {t("separators.addAtEnd")}
			</ContextMenuItem>
			<ContextMenuItem onSelect={() => void onImport()}>
				<Upload /> {t("mods.import")}
			</ContextMenuItem>
			<ContextMenuSeparator />
			<ContextMenuLabel>{t("mods.menu.sort")}</ContextMenuLabel>
			<ContextMenuItem disabled={filtering || mods.length < 2} onSelect={() => void onSortByName()}>
				<ArrowDownAZ /> {t("mods.sortAlpha")}
			</ContextMenuItem>
			<ContextMenuItem disabled={filtering || mods.length < 2} onSelect={() => void onSortByRules()}>
				<ListTree /> {t("mods.sortByRules")}
			</ContextMenuItem>
			{hasSeparators && (
				<>
					<ContextMenuSeparator />
					<ContextMenuLabel>{t("mods.menu.groups")}</ContextMenuLabel>
					<ContextMenuItem onSelect={() => onFoldAll(true)}>
						<ChevronsDownUp /> {t("separators.foldAll")}
					</ContextMenuItem>
					<ContextMenuItem onSelect={() => onFoldAll(false)}>
						<ChevronsUpDown /> {t("separators.unfoldAll")}
					</ContextMenuItem>
				</>
			)}
		</ContextMenuContent>
	);
}
