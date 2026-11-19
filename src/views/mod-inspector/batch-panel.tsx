import type { Status } from "@/bindings";
import { Button } from "@/components/ui/button";
import { ButtonGroup } from "@/components/ui/button-group";
import { useTranslation } from "@/i18n";
/// Actions for several selected mods at once. The backend expands a page header to its parts.
export function BatchPanel({
	status,
	selection,
	onSetEnabled,
	onSelect,
}: {
	status: Status;
	selection: string[];
	onSetEnabled: (ids: string[], enabled: boolean) => void | Promise<void>;
	onSelect: (folders: string[]) => void;
}) {
	const { t } = useTranslation();
	const setEnabled = (enabled: boolean) => onSetEnabled(selection, enabled);
	// A Set keeps this linear: `selection.includes` made a 10k selection take ~174 ms per render.
	const selected = new Set(selection);
	const picked = status.mods.filter((mod) => selected.has(mod.id));
	const on = picked.filter((mod) => mod.enabled).length;

	return (
		<section className="grid content-start gap-6 p-5">
			<h2 className="text-base font-semibold">{t("mods.selected", { count: selection.length })}</h2>
			<ButtonGroup>
				<Button variant="outline" size="sm" disabled={on === picked.length} onClick={() => void setEnabled(true)}>
					{t("common.enable")}
				</Button>
				<Button variant="outline" size="sm" disabled={on === 0} onClick={() => void setEnabled(false)}>
					{t("common.disable")}
				</Button>
			</ButtonGroup>
			<Button variant="ghost" size="sm" className="justify-self-start" onClick={() => onSelect([])}>
				{t("mods.clearSelection")}
			</Button>
		</section>
	);
}
