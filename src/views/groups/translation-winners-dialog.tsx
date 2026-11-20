import { useState } from "react";
import { useWorkspace } from "@/app/contexts/workspace-context";
import { commands } from "@/bindings";
import { BoxList, BoxListRow } from "@/components/app/content-blocks";
import { PathText } from "@/components/app/path-text";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { FieldError } from "@/components/ui/field";
import { ItemContent, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { messageOf, unwrap } from "@/lib/ipc";
import { modName } from "@/lib/mods";
import { sharedConflictPaths } from "@/lib/translation-hints";
import { useLoad } from "@/lib/use-load";

/// Picks which copy of every path a translation shares with its base reaches the game. The two reads are the only Tauri
/// calls a dialog owns; the choice itself goes through the queued action.
export function TranslationWinnersDialog({ gameId, groupId, onClose }: { gameId: string; groupId: string; onClose: () => void }) {
	const { t } = useTranslation();
	const { status, gameActions } = useWorkspace();
	const [failure, setFailure] = useState<string | null>(null);
	const group = status?.groups.find((entry) => entry.id === groupId);
	// Member order is the rule: the first is the base, the rest are translations. With several translations only the
	// first is used, so one action can set all the winners of one shared set of paths.
	const members = group?.members ?? [];
	const baseId = members[0];
	const translationId = members[1];
	const nameOf = (id: string | undefined) => (id ? modName(status?.mods, id) : "");
	// modDetails names the selected version by folder, while a group holds family ids.
	const baseFolder = baseId ? status?.mods.find((mod) => mod.id === baseId)?.folder : undefined;
	const translationFolder = translationId ? status?.mods.find((mod) => mod.id === translationId)?.folder : undefined;
	const key = baseFolder !== undefined && translationFolder !== undefined ? `winners:${gameId}:${baseFolder}:${translationFolder}` : null;
	const details = useLoad(
		key,
		async () => {
			const [base, translation] = await Promise.all([
				unwrap(commands.modDetails(gameId, baseFolder ?? "")),
				unwrap(commands.modDetails(gameId, translationFolder ?? "")),
			]);
			return { base, translation };
		},
		{ onError: (error) => setFailure(messageOf(error)) },
	);
	const paths = details ? sharedConflictPaths(details.base, details.translation) : [];
	const loading = details === undefined && failure === null;

	const confirm = () => {
		if (!translationId) return;
		onClose();
		void gameActions.setFileWinners(paths, translationId);
	};

	return (
		<ModalDialog title={t("mods.winners.translationTitle")} description={t("mods.winners.translationHint")} width="lg" onClose={onClose}>
			{members.length > 2 && <p className="text-sm">{t("mods.winners.usingFirst", { name: nameOf(translationId) })}</p>}
			{failure && <FieldError>{failure}</FieldError>}
			{loading && <p className="text-muted-foreground text-sm">{t("mods.winners.loading")}</p>}
			{details && paths.length === 0 && <p className="text-muted-foreground text-sm">{t("mods.winners.empty")}</p>}
			{details && paths.length > 0 && (
				<BoxList scroll tall>
					{paths.map((path) => (
						<BoxListRow key={path}>
							<ItemContent>
								<ItemTitle className="min-w-0 font-normal">
									<PathText path={path} />
								</ItemTitle>
							</ItemContent>
						</BoxListRow>
					))}
				</BoxList>
			)}
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.cancel")}
				</Button>
				<Button disabled={paths.length === 0} onClick={confirm}>
					{t("mods.winners.confirm")}
				</Button>
			</DialogFooter>
		</ModalDialog>
	);
}
