import { CircleAlert, CircleCheck, CircleX, ExternalLink } from "lucide-react";
import type { CheckState, PreflightCheck } from "@/bindings";
import { BoxListRow } from "@/components/app/content-blocks";
import { Button } from "@/components/ui/button";
import { useTranslation } from "@/i18n";
import { translate } from "@/lib/ipc";

const ICON = {
	ok: CircleCheck,
	warning: CircleAlert,
	problem: CircleX,
} as const satisfies Record<CheckState, typeof CircleCheck>;

export function CheckRow({
	check,
	free,
	onOpenSettings,
	onShowMods,
	onOpenStorage,
	onGetRequirement,
}: {
	check: PreflightCheck;
	free: string | null;
	onOpenSettings: () => void;
	onShowMods: () => void;
	onOpenStorage: () => void;
	onGetRequirement: (path: string) => void;
}) {
	const { t } = useTranslation();
	const Icon = ICON[check.state];
	const action = (() => {
		if (check.state === "ok") return null;

		if (check.id === "game_folder" || check.id === "settings_folder" || check.id === "mods_folder" || check.id === "overlay") {
			return (
				<Button variant="outline" size="sm" onClick={onOpenSettings}>
					{t("setupCheck.openSettings")}
				</Button>
			);
		}

		if (check.id === "loader" && check.requirement_path) {
			const path = check.requirement_path;
			const name = check.requirement_name ?? path;
			return (
				<Button variant="outline" size="sm" title={t("setupCheck.getHint", { name })} onClick={() => onGetRequirement(path)}>
					<ExternalLink /> {t("setupCheck.get", { name })}
				</Button>
			);
		}

		if (check.id === "loader" || check.id === "problems" || check.id === "mods") {
			return (
				<Button variant="outline" size="sm" onClick={onShowMods}>
					{t("setupCheck.showMods")}
				</Button>
			);
		}

		if (check.id === "space") {
			return (
				<Button variant="outline" size="sm" onClick={onOpenStorage}>
					{t("setupCheck.openStorage")}
				</Button>
			);
		}

		return null;
	})();

	return (
		<BoxListRow className="flex-nowrap items-start gap-2 py-2.5">
			<Icon
				className={check.state === "problem" ? "text-destructive mt-0.5 size-4 shrink-0" : "text-muted-foreground mt-0.5 size-4 shrink-0"}
				aria-hidden
			/>
			<div className="grid min-w-0 flex-1 gap-0.5">
				<p className="text-sm">{translate(check.message)}</p>
				{free && <p className="text-muted-foreground font-mono text-xs tabular-nums">{t("setupCheck.freeSpace", { free })}</p>}
			</div>
			{action}
		</BoxListRow>
	);
}
