import { ListTree } from "lucide-react";
import { type ReactNode, useState } from "react";

import { useWorkspace } from "@/app/contexts/workspace-context";
import { commands } from "@/bindings";
import { BoxList, BoxListRow, StatusLine } from "@/components/app/content-blocks";
import { ModalDialog } from "@/components/modal-dialog";
import { Button } from "@/components/ui/button";
import { DialogFooter } from "@/components/ui/dialog";
import { Empty, EmptyHeader, EmptyTitle } from "@/components/ui/empty";
import { ItemContent, ItemTitle } from "@/components/ui/item";
import { useTranslation } from "@/i18n";
import { unwrap } from "@/lib/ipc";
import { useLoad } from "@/lib/use-load";

/// What sorting by the rules would move, before the list changes. The sort itself is one history entry, so the order
/// shown here is the backup that Undo brings back.
export function SortPreviewDialog({ gameId, onClose }: { gameId: string; onClose: () => void }) {
	const { t } = useTranslation();
	const { gameActions } = useWorkspace();
	const [busy, setBusy] = useState(false);
	const preview = useLoad(`sort:${gameId}`, () => unwrap(commands.previewSort(gameId)));

	const sort = async () => {
		setBusy(true);
		await gameActions.sortByRules();
		setBusy(false);
		onClose();
	};

	let body: ReactNode;

	if (!preview) {
		body = <StatusLine tone="loading">{t("sortPreview.loading")}</StatusLine>;
	} else if (preview.moves.length === 0) {
		body = (
			<Empty>
				<EmptyHeader>
					<EmptyTitle>{t("sortPreview.empty")}</EmptyTitle>
				</EmptyHeader>
			</Empty>
		);
	} else {
		body = (
			<>
				<p className="text-sm">{t("sortPreview.moves", { count: preview.moves.length })}</p>
				<BoxList scroll tall>
					{preview.moves.map((move) => (
						<BoxListRow key={move.id}>
							<ItemContent className="min-w-0 flex-row items-center justify-between gap-3">
								<ItemTitle className="min-w-0 truncate font-normal">{move.name}</ItemTitle>
								<span className="text-muted-foreground shrink-0 font-mono text-xs tabular-nums">
									{move.from}
									{" → "}
									{move.to}
								</span>
							</ItemContent>
						</BoxListRow>
					))}
				</BoxList>
				<p className="text-muted-foreground text-xs">{t("sortPreview.backup")}</p>
			</>
		);
	}

	return (
		<ModalDialog title={t("sortPreview.title")} description={t("sortPreview.hint")} width="lg" onClose={onClose}>
			{body}
			<DialogFooter>
				<Button variant="outline" onClick={onClose}>
					{t("common.close")}
				</Button>
				{preview && preview.moves.length > 0 && (
					<Button disabled={busy} onClick={() => void sort()}>
						<ListTree aria-hidden />
						{busy ? t("sortPreview.sorting") : t("sortPreview.confirm")}
					</Button>
				)}
			</DialogFooter>
		</ModalDialog>
	);
}
