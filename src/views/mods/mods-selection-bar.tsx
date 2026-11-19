import { Eye, EyeOff, Layers } from "lucide-react";
import { useUiActions } from "@/app/contexts/ui-context";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { useTranslation } from "@/i18n";
import type { Mod } from "@/lib/mods";

/// The bar that appears under the list when several mods are selected. It only offers bulk actions that make sense for a
/// selection; a single mod is edited in the inspector.
export function ModsSelectionBar({
	picked,
	selection,
	hidden,
	onSetHidden,
	onSetEnabled,
	onSelect,
}: {
	/// The selected mods that still exist in the profile.
	picked: Mod[];
	selection: string[];
	/// The mods the list currently hides, from the same rule the rows use.
	hidden: ReadonlySet<string>;
	onSetHidden: (ids: string[], hide: boolean) => void;
	onSetEnabled: (id: string, enabled: boolean) => void | Promise<void>;
	onSelect: (mods: string[]) => void;
}) {
	const { t } = useTranslation();
	const { openDialog } = useUiActions();
	/// Each toggle joins the profile queue; the backend merges the fresh status after every one.
	const setEnabled = (enabled: boolean) => {
		for (const id of selection) void onSetEnabled(id, enabled);
	};
	const pickedOn = picked.filter((mod) => mod.enabled).length;
	const hiddenIds = picked.filter((mod) => hidden.has(mod.id)).map((mod) => mod.id);
	// An enabled mod is never hidden, so hiding only the off ones here matches the per-mod menu.
	const hideIds = picked.filter((mod) => !(mod.enabled || hidden.has(mod.id))).map((mod) => mod.id);
	// Showing wins: any hidden mod in the selection makes this a "show" action; otherwise it hides the off ones.
	// When neither applies (every selected mod is on) the button stays visible but disabled, so the bulk action is
	// always in the same place. The title explains the prerequisite until a dedicated hint key exists.
	const showing = hiddenIds.length > 0;
	const applicable = showing || hideIds.length > 0;

	if (picked.length < 2) return null;

	return (
		<div
			role="toolbar"
			aria-label={t("mods.selected", { count: picked.length })}
			className="bg-muted flex shrink-0 flex-wrap items-center gap-2 border-t px-6 py-2 text-sm"
		>
			<span className="font-medium">{t("mods.selected", { count: picked.length })}</span>
			<span className="text-muted-foreground text-xs">{t("mods.selectedOn", { on: pickedOn, count: picked.length })}</span>
			<div className="ml-auto flex items-center gap-2">
				<ButtonGroup>
					<Button size="sm" variant="outline" disabled={pickedOn === picked.length} onClick={() => setEnabled(true)}>
						{t("common.enable")}
					</Button>
					<Button size="sm" variant="outline" disabled={pickedOn === 0} onClick={() => setEnabled(false)}>
						{t("common.disable")}
					</Button>
				</ButtonGroup>
				<Button
					size="sm"
					variant="ghost"
					disabled={!applicable}
					title={applicable ? undefined : t("mods.hideDisabledHint")}
					onClick={applicable ? () => onSetHidden(showing ? hiddenIds : hideIds, !showing) : undefined}
				>
					{showing ? <Eye aria-hidden /> : <EyeOff aria-hidden />}
					{showing ? t("mods.unhide") : t("mods.hide")}
				</Button>
				<Button size="sm" variant="ghost" onClick={() => openDialog({ kind: "group-edit", members: selection })}>
					<Layers aria-hidden />
					{t("mods.group.action")}
				</Button>
				<Button size="sm" variant="ghost" onClick={() => onSelect([])}>
					{t("mods.clearSelection")}
				</Button>
			</div>
		</div>
	);
}
