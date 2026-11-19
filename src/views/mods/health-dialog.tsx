import { CircleAlert, CircleX } from "lucide-react";
import type { ReactNode } from "react";

import { useNavigation } from "@/app/contexts/navigation-context";
import { useUiActions } from "@/app/contexts/ui-context";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { commands, type HealthActionKind, type HealthItem } from "@/bindings";
import { BoxList, BoxListRow } from "@/components/app/content-blocks";
import { DialogCloseFooter, ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { ItemActions, ItemContent, ItemMedia, ItemTitle } from "@/components/ui/item";
import { type TranslationKey, useTranslation } from "@/i18n";
import { translateCode, unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";

const ACTION_LABEL: Record<HealthActionKind, TranslationKey> = {
	open_settings: "health.action.openSettings",
	show_mods: "health.action.showMods",
	open_overwrite: "health.action.openOverwrite",
	apply: "health.action.apply",
};

/// One list of the things that block an apply and the warnings that do not. Every row has the same action the rest of the
/// window would offer, so a problem is fixed from here without hunting for the screen.
export function HealthDialog({ gameId, onClose }: { gameId: string; onClose: () => void }) {
	const { t } = useTranslation();
	const { openGameSettings, openMods } = useNavigation();
	const { openDialog } = useUiActions();
	const { gameActions } = useWorkspace();
	const report = useLoad(`health:${gameId}`, () => unwrap(commands.healthCheck(gameId)));

	const run = (kind: HealthActionKind) => {
		onClose();
		if (kind === "open_settings") openGameSettings();
		else if (kind === "show_mods") openMods();
		else if (kind === "open_overwrite") openDialog({ kind: "overwrite" });
		else void gameActions.runDeploy();
	};

	let body: ReactNode;
	if (!report) {
		body = <p className="text-muted-foreground text-sm">{t("health.loading")}</p>;
	} else if (report.items.length === 0) {
		body = <p className="text-muted-foreground text-sm">{t("health.empty")}</p>;
	} else {
		body = (
			<BoxList>
				{report.items.map((item) => (
					<HealthRow key={item.id} item={item} onRun={run} />
				))}
			</BoxList>
		);
	}

	return (
		<ModalDialog title={t("health.title")} description={t("health.hint")} width="lg" onClose={onClose}>
			{body}
			<DialogCloseFooter onClose={onClose} />
		</ModalDialog>
	);
}

function HealthRow({ item, onRun }: { item: HealthItem; onRun: (kind: HealthActionKind) => void }) {
	const { t } = useTranslation();
	const Icon = item.severity === "problem" ? CircleX : CircleAlert;
	const action = item.action;
	return (
		<BoxListRow>
			<ItemMedia>
				<Icon className={item.severity === "problem" ? "text-destructive size-4" : "text-muted-foreground size-4"} aria-hidden />
			</ItemMedia>
			<ItemContent className="min-w-0">
				<ItemTitle className="w-full text-sm font-normal">{translateCode(item.message.code, item.message.params)}</ItemTitle>
			</ItemContent>
			{action && (
				<ItemActions>
					<Button variant="outline" size="sm" onClick={() => onRun(action.kind)}>
						{t(ACTION_LABEL[action.kind])}
					</Button>
				</ItemActions>
			)}
		</BoxListRow>
	);
}
